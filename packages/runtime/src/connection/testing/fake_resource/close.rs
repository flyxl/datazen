//! op 9 `closeResource` 的实现（fake-runtime-fixtures.md §3.1、§5.3 规则 2/3/6、
//! connection-management.md CM-74 / §9.4）。
//!
//! 从 `ops.rs` **整段**搬出来单独立文件：关闭是九个操作里唯一同时踩 permit 口径、
//! 归池判据和释放顺序三项规范的操作，把它和「创建 / 执行 / 重置」那类操作摊在一个
//! 文件里，评审 `§9.4(b)` 判据时会被另外八个操作的上下文淹没。
//!
//! 依赖方向不变：只向下依赖 `connection::{port, session, types, execution, error}`
//! 与同目录的 `handles` / `state` / `script`，仍然不写任何网络或凭据代码（§13）。

use crate::connection::error::ProviderError;
use crate::connection::execution::EffectOutcome;
use crate::connection::port::{
    BudgetClass, CloseReceipt, CloseResourceRequest, PermitId, ResourceRelease,
};
use crate::connection::session::{SessionHandleRef, SessionState, TransactionState};
use crate::connection::testing::journal::{HandleAction, ResourceEvent};
use crate::connection::types::{DbSessionId, OwnerRef, PoolKeyFingerprint};

use super::script::{FaultKind, ResourceOp};
use super::state::{FakeResourceState, PermitEventPlan};
use super::FakeResourceProvider;

impl FakeResourceProvider {
    /// §5.3 规则 2/3/6：只有 `Closed` 归还 permit；`CloseUnconfirmed` 不归还。
    ///
    /// CM-74：句柄注销**无条件**发生，且排在 `Closed` 与 permit 归还**之前**
    /// （journal 里句柄事件排在资源终态之前）。§9.3 竞态脚本要求回滚时，额外清掉事务态。
    pub fn close_resource(
        &self,
        request: &CloseResourceRequest,
    ) -> Result<CloseReceipt, ProviderError> {
        // F11：关闭未确认 —— 资源留在预算占用里（§5.3 规则 3：余额不变）。
        // 判定只取脚本，不碰资源表：注入额度在这里消耗一次。
        let unconfirmed = matches!(
            self.script.take(ResourceOp::Close),
            Some((_, FaultKind::CloseUnconfirmed { .. }))
        );
        let rolled_back = self.script.rollback_before_release();

        // **唯一一次**资源表求值：§3.1 凭证校验、注销前的句柄数、状态迁移、permit 归还计划
        // 全在这一段临界区里完成，中间没有观察者，也就没有 TOCTOU。
        let facts = self.evaluate_close(request, unconfirmed, rolled_back)?;

        if unconfirmed {
            // §4.2 F11 / §5.3 规则 3：资源留在预算占用里，只记 `CloseUnconfirmed`。
            // **不**记 `Closed`、**不**写 `ReturnedToPool`、**不**归还 permit、
            // **不**注销句柄、**不**收 lease/session 登记 —— 未确认的关闭没资格做这些。
            if let Some(owner) = facts.owner.as_ref() {
                self.journal.record_resource_event(
                    &request.handle.resource_id,
                    ResourceEvent::CloseUnconfirmed,
                    owner,
                    facts.pool_key,
                    facts.budget_class,
                );
            }
            return Ok(CloseReceipt {
                db_session_id: facts.db_session_id,
                state: SessionState::Closing,
                effect_outcome: EffectOutcome::Unknown,
                resource_release: ResourceRelease::Pending,
            });
        }

        // CM-74 顺序：`handle closed` → `ReturnedToPool?` / `Closed` → `permit -1`。
        for open in &facts.deregistered {
            self.journal.record_handle(
                open,
                HandleAction::Closed,
                "物理关闭前注销句柄（CM-74 / §9.3）",
            );
        }
        if let Some(owner) = facts.owner.as_ref() {
            // §5.3 规则 2 前置：归池必须有「协议已排空 + 无登记句柄」的证据。
            //
            // 「无登记句柄」读**注销之前**的实测值，不是刚注销完的 `registered_handles()`
            // （那恒为 0，读它等于没判），也不是宿主在请求里自述的 `registered_handles`
            // —— §9.4(b) 对此有专门判据，见 `evaluate_close` 的注释。
            //
            // 读注销前快照是判据的字面要求而非取巧：CM-74 对同一条断言既要求句柄先于
            // 物理关闭注销，又要求「driver 报 `Clean` 而宿主仍有已登记句柄时宿主检查必须
            // 失败（§9.4）」。后者问的是「关闭开始前资源上挂没挂着句柄」，两个判据都是
            // 关闭**之前**的事实，所以快照与注销同处一把锁。
            if facts.drained && facts.handles_before_close == 0 {
                self.journal.record_resource_event(
                    &request.handle.resource_id,
                    ResourceEvent::ReturnedToPool {
                        protocol_drained: true,
                        registered_handles: facts.handles_before_close,
                    },
                    owner,
                    facts.pool_key.clone(),
                    facts.budget_class.clone(),
                );
            }
            self.journal.record_resource_event(
                &request.handle.resource_id,
                ResourceEvent::Closed,
                owner,
                facts.pool_key.clone(),
                facts.budget_class.clone(),
            );
        }

        // 归还 permit —— 只在 `Closed` 时；`Accounting::release()` 自己保证至多一次
        // （已归还过则返回 `None`）。计划连同 permitId/budgetClass 都来自那一次持有，
        // 没有第二个来源。
        if let Some((permit_id, plan, budget_class)) = facts.permit {
            self.journal
                .record_permit(&permit_id, plan.delta, plan.reason, budget_class);
        }

        // 关闭路径必须收回孤立句柄（I7）、会话登记（I4）与 lease（I3），台账才收得口。
        self.journal
            .recover_orphans_on_close(&request.handle.resource_id, "closeResource");
        // 还挂着句柄就被关掉的资源（§9.3 真实线程竞态）：句柄随会话一起死，必须一起收回（I5）。
        // slot 里的句柄上面已经在关闭前注销过了，这里是 journal 侧登记册的兜底：
        // 覆盖 runtime 登记了句柄、但本进程没有对应 slot 条目的那部分。
        self.journal
            .reclaim_registered_handles_on_close(&request.handle.resource_id, "closeResource");
        self.journal.close_active_session(&facts.db_session_id);
        for lease in self.leases_of(&request.handle.resource_id) {
            self.journal.release_lease(&lease);
        }

        Ok(CloseReceipt {
            db_session_id: facts.db_session_id,
            state: SessionState::Closed,
            // 诚实：真的回滚过才报 `RolledBack`。
            effect_outcome: if rolled_back || facts.had_transaction {
                EffectOutcome::RolledBack
            } else {
                EffectOutcome::Completed
            },
            resource_release: ResourceRelease::Confirmed,
        })
    }

    /// 在**一次**持有资源表期间，把一次关闭请求要记账的全部事实算出来。
    ///
    /// 旧实现是**三次**求值：`resolve()` 克隆一份、归池判定 `get_mut()` 第二份、归还
    /// permit 再 `get_mut()` 第三份。顶层 `resources` 只在 `create_resource`（`ops.rs`
    /// 的 `insert`）插入，全目录没有任何 `remove` / `retain` / `clear` 作用在它身上，
    /// 所以后两次 `get_mut` **恒为 `Some`** —— 第二份的 `None` 分支是**可证明的死代码**，
    /// 而它返回的恰恰是最危险的一组值：
    /// `request.protocol_drained` + `resource.registered_handles()` + 空注销清单，
    /// 即「拿请求里的自述值当实测值、且一条句柄都不注销」。这样的死分支不是无害冗余，
    /// 它是「一旦可达就静默说谎」的形状，留着等于邀请下一个人把 `resources` 改成会
    /// 删除资源的行为，而那一改动没人会连带复核这里的兜底值。
    ///
    /// 现在第二次查表没有了，`None` 变成「未知 resourceId 的关闭」唯一真正的入口：
    /// 它报 `SessionLost`，且发生在**任何**台账写入之前（用例
    /// `closing_an_unknown_resource_id_is_rejected_before_any_journal_write`）。
    ///
    /// §9.2 的 `handle_from_other_resource` 反例在这条路上依然被拒：`resolve()` 此前
    /// 把 `request.handle.resource_id` 同时当作「声称的 id」和「实际查表的 key」，
    /// 两个身份恒等，跨资源句柄用例会因查不到资源而先报 `SessionLost`；新路径不再依赖
    /// 这一点 —— `verify()` 拿 `request.handle.resource_id` 与表项里的
    /// `runtime_epoch` / `owner` 对照，伪造的外来句柄同样过不去。
    fn evaluate_close(
        &self,
        request: &CloseResourceRequest,
        unconfirmed: bool,
        rolled_back: bool,
    ) -> Result<CloseFacts, ProviderError> {
        let key = request.handle.resource_id.as_str();
        let mut resources = self.lock();
        let slot = resources.get_mut(key).ok_or_else(|| {
            ProviderError::SessionLost(format!(
                "资源 {key} 不存在，closeResource 无从判定归池前置（§9.4）"
            ))
        })?;
        // §3.1：`ResourceHandle` 只由提供方签发，每个操作都要校验三要素
        // （resourceId / dbSessionId / runtimeEpoch）。
        // 无 owner 的资源写不出参与校验的 owner，报 `SessionLost`（与 `resolve` 同口径）。
        let owner_ref: &OwnerRef = slot.owner.as_ref().ok_or_else(|| {
            ProviderError::SessionLost(format!("资源 {key} 没有 owner，无法验证句柄"))
        })?;
        request
            .handle
            .verify(&request.handle.resource_id, &slot.runtime_epoch, owner_ref)?;

        let facts = CloseFacts {
            owner: slot.owner.clone(),
            pool_key: slot.pool_key.clone(),
            budget_class: slot.budget_class(),
            db_session_id: slot.db_session_id.clone(),
            had_transaction: slot.has_open_transaction(),
            drained: false,
            handles_before_close: 0,
            deregistered: Vec::new(),
            permit: None,
        };

        if unconfirmed {
            // §4.2 F11：资源留在预算占用里，只推进到 `Closing`，排空标志一律作废。
            slot.state = FakeResourceState::Closing;
            slot.protocol_drained = false;
            return Ok(facts);
        }

        // 幂等关闭：资源可能已经是 `Closed`（fixtures §3 契约表「幂等：首次 `Closed`，
        // 重复调用仍 `Closed`」/ 连接 §3 INV-10「关闭幂等，预算只释放一次」），
        // 所以这里不设「已关闭即拒」的门闸 —— 重复关闭仍然成功，只是 permit 不会二次归还。
        slot.protocol_drained = request.protocol_drained && slot.protocol_drained;
        let mut facts = facts;
        facts.drained = slot.protocol_drained;
        // 归池判定的输入：注销**之前**这张资源上还挂着几个句柄。
        facts.handles_before_close = slot.registered_handles();
        // CM-74：句柄随资源一起死，必须在物理关闭前注销（journal 先记 `closed`）。
        // 这一步**无条件** —— 它是关闭语义，不是回滚语义。
        facts.deregistered = slot
            .handles
            .values_mut()
            .map(|handle| {
                handle.closed = true;
                handle.clone()
            })
            .collect();
        slot.handles.clear();
        // 回滚语义仍以脚本标志为门：真的回滚过才清事务态。
        if rolled_back {
            slot.transaction_state = TransactionState::None;
        }
        slot.state = FakeResourceState::Closed;
        facts.permit = slot.accounting.release().map(|plan| {
            (
                slot.accounting.permit_id.clone(),
                plan,
                slot.accounting.budget_class,
            )
        });
        Ok(facts)
    }
}

/// 一次关闭请求要落进台账的全部事实，每个字段都取自**同一次**资源表持有。
struct CloseFacts {
    /// 无 owner 的资源（夹具里的异常形状）写不出带 owner 的台账事件，
    /// 调用方据此跳过记账而不是伪造一个 owner。
    owner: Option<OwnerRef>,
    pool_key: PoolKeyFingerprint,
    budget_class: BudgetClass,
    db_session_id: DbSessionId,
    /// 关闭前是否还开着事务 —— 决定回执的 `effect_outcome`。
    had_transaction: bool,
    /// 排空标志取两个来源的合取：宿主声明 `true` **且** 资源上已经排空过。
    drained: bool,
    /// 注销前的实测句柄数，归池判据（§9.4「已登记句柄为空」）的唯一输入。
    handles_before_close: usize,
    /// 本次关闭注销掉的句柄，按注销顺序。
    deregistered: Vec<SessionHandleRef>,
    /// 待写入的 permit 归还事件；已归还过则为 `None`（`Accounting::release()` 保证）。
    permit: Option<(PermitId, PermitEventPlan, BudgetClass)>,
}
