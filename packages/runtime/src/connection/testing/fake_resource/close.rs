//! `closeResource` 的实现（fake-runtime-fixtures.md §3.1、§5.1、§5.3 规则 2/3/6、
//! connection-management.md §6.5 / §9.4）。
//!
//! 九个操作里关闭这一条最重：它同时承载 §5.3 的 permit 归还口径（只有 `Closed` 归还）、
//! CM-74 的释放顺序（句柄注销排在 `Closed` 与 permit 归还之前）、§4.2 F11 的「关闭未确认」
//! 与 §9.4(b) 的归池前置判据，所以从 `ops.rs` 单独成文件 —— 与
//! [`transaction`](super::transaction) 承载 op 6（F8 事务）是同一处置。
//!
//! 判定仍走 `FakeResource::prepare_close`（§3.1 的 `resourceId` + `runtimeEpoch` + owner
//! 三校验 + 全部状态迁移），故障注入只改返回值、状态迁移与 journal 写入照常 ——
//! 与 `ops.rs` 里其余操作同一条纪律（§5.3 的断言因此读的是同一本台账）。

use crate::connection::error::ProviderError;
use crate::connection::execution::EffectOutcome;
use crate::connection::port::{CloseOutcome, CloseReceipt, CloseResourceRequest, ResourceRelease};
use crate::connection::session::SessionState;
use crate::connection::testing::journal::{HandleAction, ResourceEvent};
use crate::connection::types::DbSessionId;

use super::script::{FaultKind, ResourceOp};
use super::FakeResourceProvider;

impl FakeResourceProvider {
    // ---- 9. closeResource ----

    /// §5.3 规则 2/3/6：只有 `Closed` 归还 permit；`CloseUnconfirmed` 不归还。
    ///
    /// CM-74：句柄注销**无条件**发生，且排在 `Closed` 与 permit 归还**之前**
    /// （journal 里句柄事件排在资源终态之前）。§9.3 竞态脚本要求回滚时，额外清掉事务态。
    ///
    /// # §9.4(b)：判据的两个输入都来自**一次**求值，分叉永不放行归池
    ///
    /// [`ClosePrecondition`] 由 [`FakeResource::prepare_close`] 在**实际注销句柄的那一把锁里**
    /// 一次算出，同时带出「被调方实测的登记数」与「宿主在 [`CloseResourceRequest`] 里声称的
    /// 同一个量」。归池判据（[`ClosePrecondition::returns_to_pool`]）**两个都消费**：
    /// 实测必须为 0，且两份账必须一致。§9.4 把前置写成 AND（「宿主检查全部通过 **AND**
    /// driver 返回 Clean」「任一失败都关闭」），而 §6.5 规定句柄只能经登记/注销改变 ——
    /// 所以账本分叉本身就说明前置不成立，绝不能归池。
    ///
    /// 关键方向是「宿主声称 0、资源上其实还挂着句柄」（§4.2 F10 的形状）：此时判据按**实测**
    /// 拒绝归池。若判据改读**声明**，这一笔就会伪称「宿主检查全部通过」而把带句柄的资源放回池子。
    /// 承重用例是
    /// [`pooling_refuses_a_host_ledger_that_undercounts_the_registered_handles`](crate::connection::testing::fake_resource::close_cases::pooling_refuses_a_host_ledger_that_undercounts_the_registered_handles)、
    /// 它的姊妹例
    /// [`pooling_refuses_a_stale_host_ledger_that_overcounts_the_registered_handles`](crate::connection::testing::fake_resource::close_cases::pooling_refuses_a_stale_host_ledger_that_overcounts_the_registered_handles)
    /// 与对照正例
    /// [`a_clean_resource_with_matching_ledgers_returns_to_the_pool`](crate::connection::testing::fake_resource::close_cases::a_clean_resource_with_matching_ledgers_returns_to_the_pool)。
    ///
    /// 同一次锁里还消掉了另外几处「快照 vs 重读」的双源：`owner`、`pool_key`、
    /// `budget_class`、`db_session_id`、`had_transaction` 现在都来自这**一个**临界区，
    /// 不再是 `resolve()` 的克隆 + 后面的第二次 `get_mut` + 归还 permit 时的第三次 `get_mut`。
    ///
    /// # 曾经可证明的 `None` 分支现在是真实分支
    ///
    /// 旧实现的 `None` 分支不可达：顶层 `resources` 只在 `create_resource` 插入、从不删除，
    /// 而 `resolve()` 已在前一步查过同一张表，所以后面的 `get_mut` 恒为 `Some`；
    /// 两支产出**不同**的判据输入，正是「可分叉」的另一半。
    /// 现在**没有第二次查表**：这一段 `ok_or_else` 是「未知 resourceId 的关闭」唯一真正的入口，
    /// 并且有用例覆盖 ——
    /// [`closing_an_unknown_resource_id_is_rejected_before_any_journal_write`](crate::connection::testing::fake_resource::close_cases::closing_an_unknown_resource_id_is_rejected_before_any_journal_write)。
    /// 它返回 `ProviderError::SessionLost`（不 panic）：§3.1 要求每个操作都校验凭证，
    /// 而「资源根本不在册」是**调用方**能造出来的输入，不是夹具内部的不变量，
    /// 按 panic-policy 也不该用 `unwrap()/expect()` 把输入错误升级成 panic。
    ///
    /// 注意这**不是**新增的可达性：旧实现的 `resolve()` 在同一步就以同样的错误码拒绝了
    /// 未知 `resourceId`。变的是**拒绝发生在哪一次查表上** —— 从「先 `resolve()` 拿快照、
    /// 再 `get_mut` 拿第二次」变成「只有一次 `get_mut`」，于是那个 `None` 分支从
    /// 恒不成立变成**唯一入口**。
    ///
    /// # F11 走的是同一个 `prepare_close`，只是 `unconfirmed = true`
    ///
    /// 资源**不**销毁：留在 `Closing`、`protocol_drained = false`、句柄保持登记、permit
    /// **不**归还（§5.3 规则 3：余额不变）。注销之所以不做，是因为未确认的关闭不能伪称
    /// 「句柄随资源一起死了」——那会让后续隔离/重试路径读到一张「句柄已清空、其实没关成」
    /// 的资源。`close_unconfirmed_*` 那组用例钉的就是这一整套后果。
    ///
    /// # F14：重复关闭只记账一次，回执照发
    ///
    /// 第二次 `closeResource` 打在已经 `Closed` 的资源上，旧实现**仍然**记 `ReturnedToPool`
    /// 与 `Closed` 各一条。permit 之所以没被退两次，只是因为 `Accounting::release()`
    /// 恰好是幂等的 —— 那是 budget 模块替关闭路径兜的底，不是关闭路径自己的判据。
    /// 后果是台账里凭空多出一整轮关闭：两条 `Closed`、两条 `ReturnedToPool`、
    /// 一次已经发生过第二次的物理归池。判负用例是
    /// [`closing_twice_is_idempotent_and_never_pools_or_refunds_twice`](crate::connection::testing::fake_resource::close_cases::closing_twice_is_idempotent_and_never_pools_or_refunds_twice)。
    ///
    /// 门闸是 [`ClosePrecondition::freshly_closed`]（`!was_closed`，同锁实测），它同时管住
    /// `ReturnedToPool`（经 [`ClosePrecondition::returns_to_pool`]）与 `Closed`。**回执不变**：
    /// 第二次仍是 `Closed` / `Confirmed` / `Completed`，符合 fixtures §3 契约表与连接 §3 INV-10。
    /// `Quarantined → close` 与 `CloseUnconfirmed → close` 的 `was_closed` 都是假，不受影响。
    pub fn close_resource(
        &self,
        request: &CloseResourceRequest,
    ) -> Result<CloseReceipt, ProviderError> {
        // F11 的判定只取脚本，不碰资源表：注入额度在这里消耗一次。
        let unconfirmed = matches!(
            self.script.take(ResourceOp::Close),
            Some((_, FaultKind::CloseUnconfirmed { .. }))
        );
        let rolled_back = self.script.rollback_before_release();
        let key = request.handle.resource_id.as_str().to_owned();

        // 唯一一次资源表求值：校验凭证、测注销前的句柄数、迁移状态、决定 permit 归还，
        // 全在同一段临界区里完成（顺序由 `prepare_close` 固定）。
        // 资源不在表里 ⇒ `SessionLost`。这一支在旧实现里是**可证明的死代码**：
        // 顶层 `resources` 只在 `create_resource`（`ops.rs` 的 `insert`）插入、从不删除，
        // 而前面的 `resolve()` 已经查过一遍，所以后面的 `get_mut` 恒为 `Some`。
        // 现在**没有第二次查表**，这一段是「未知 resourceId 的关闭」唯一真正的入口，
        // 并且有用例覆盖（`closing_an_unknown_resource_id_is_rejected_before_any_journal_write`）。
        let (attempt, owner, pool_key, budget_class, db_session_id) = {
            let mut resources = self.lock();
            let slot = resources.get_mut(&key).ok_or_else(|| {
                ProviderError::SessionLost(format!(
                    "资源 {key} 不存在，closeResource 无从判定归池前置（§9.4）"
                ))
            })?;
            // 幂等关闭：资源可能已经是 `Closed`（fixtures §3 契约表「幂等：首次 Closed，
            // 重复调用仍 Closed」/ 连接 §3 INV-10「关闭幂等，预算只释放一次」），
            // 所以这里不设「已关闭即拒」
            // 的门闸 —— 重复关闭仍然成功，只是 permit 不会二次归还。
            let attempt = slot.prepare_close(
                &request.handle,
                request.protocol_drained,
                request.registered_handles,
                unconfirmed,
                rolled_back,
            )?;
            (
                attempt,
                slot.owner.clone(),
                slot.pool_key.clone(),
                slot.budget_class(),
                slot.db_session_id.clone(),
            )
        };
        let precondition = attempt.precondition;

        if precondition.unconfirmed {
            // §4.2 F11 / §5.3 规则 3：资源留在预算占用里，只记 `CloseUnconfirmed`。
            // **不**记 `Closed`、**不**写 `ReturnedToPool`、**不**归还 permit、
            // **不**注销句柄、**不**收 lease/session 登记 —— 未确认的关闭没资格做这些。
            // 无 owner 的资源（夹具里 `FakeResource.owner = None` 的异常形状）写不出
            // `ResourceEvent`，此时宁可让台账缺这一笔也不伪造一个 owner。
            if let Some(owner) = owner.as_ref() {
                self.journal.record_resource_event(
                    &request.handle.resource_id,
                    ResourceEvent::CloseUnconfirmed,
                    owner,
                    pool_key,
                    budget_class,
                );
            }
            return Ok(close_receipt(
                db_session_id,
                attempt.close_outcome,
                EffectOutcome::Unknown,
            ));
        }

        // CM-74 顺序：`handle closed` → `ReturnedToPool?` / `Closed` → `permit -1`。
        for open in &attempt.deregistered {
            self.journal.record_handle(
                open,
                HandleAction::Closed,
                "物理关闭前注销句柄（CM-74 / §9.3）",
            );
        }
        // F14（连接 §3 INV-10「关闭幂等，预算只释放一次」/ §7.5(6)、§7.5(7) 的 tombstone /
        // §9.3「归还 idle pool 不释放物理连接预算」）：重复关闭**不产生新的记账事实**。
        // `freshly_closed` 是「本次调用真的把一张未关闭的资源关掉了」的实测值，在
        // `prepare_close` 的那把锁里、状态迁移之前测得。资源早就是 `Closed` 时它为假，
        // 于是这一段整体跳过：既不再写 `ReturnedToPool`，也不再写第二条 `Closed`。
        //
        // 回执仍然照发（`close_receipt` 在这两道门闸之外）：fixtures §3 契约表「幂等：首次 Closed，
        // 重复调用仍 Closed」与 `Confirmed` 管的是**响应**，`Closed` 事件管的是**台账**，
        // 两者不是同一件事。重复调用记第二条 `Closed` 会让台账凭空多出一次状态迁移，
        // 而台账是 §5.3 的判负依据 —— 幂等的要求正在于「台账只记一次」。
        if let Some(owner) = owner.as_ref() {
            // §5.3 规则 2 前置：归池必须有「协议已排空 + 无登记句柄 + 本次真的关掉一张新资源」
            // 的证据。判据读 `measured_handles`（注销**前**的实测值）—— 不是注销之后的余量
            // （恒为 0，读它等于没判），也不是宿主声明（可与实测分叉，见上面的 §9.4(b)）。
            // `freshly_closed` 是这道判据**自己的**合取项（F14），所以这一层不再另包门闸：
            // 包两层会让其中一层变成没人盯着的冗余项，删掉它也照样绿。
            if precondition.returns_to_pool() {
                self.journal.record_resource_event(
                    &request.handle.resource_id,
                    ResourceEvent::ReturnedToPool {
                        protocol_drained: precondition.protocol_drained,
                        registered_handles: precondition.measured_handles,
                    },
                    owner,
                    pool_key.clone(),
                    budget_class.clone(),
                );
            }
            // `Closed` 不受归池判据约束（§9.4：driver 不 Clean 也必须关闭），所以它自带门闸。
            if precondition.freshly_closed {
                self.journal.record_resource_event(
                    &request.handle.resource_id,
                    ResourceEvent::Closed,
                    owner,
                    pool_key,
                    budget_class.clone(),
                );
            }
        }

        // 归还 permit —— 只在 `Closed` 时，且 `Accounting::release()` 保证至多一次。
        // 计划连同 permitId/budgetClass 都来自刚才那一次 slot 求值，没有第二个来源。
        if let Some(release) = attempt.permit {
            self.journal.record_permit(
                &release.permit_id,
                release.plan.delta,
                release.plan.reason,
                release.budget_class,
            );
        }

        // 关闭路径必须收回孤立句柄（I7）、会话登记（I4）与 lease（I3），台账才收得口。
        self.journal
            .recover_orphans_on_close(&request.handle.resource_id, "closeResource");
        // 还挂着句柄就被关掉的资源（§9.3 真实线程竞态）：句柄随会话一起死，必须一起收回（I5）。
        // slot 里的句柄上面已经在关闭前注销过了，这里是 journal 侧登记册的兜底：
        // 覆盖 runtime 登记了句柄、但本进程没有对应 slot 条目的那部分。
        self.journal
            .reclaim_registered_handles_on_close(&request.handle.resource_id, "closeResource");
        self.journal.close_active_session(&db_session_id);
        for lease in self.leases_of(&request.handle.resource_id) {
            self.journal.release_lease(&lease);
        }

        Ok(close_receipt(
            db_session_id,
            attempt.close_outcome,
            // 诚实：真的回滚过才报 `RolledBack`。`had_transaction` 同样是那一次锁里测的，
            // 不是从 `resolve()` 的克隆上读的第二份。
            if rolled_back || attempt.had_transaction {
                EffectOutcome::RolledBack
            } else {
                EffectOutcome::Completed
            },
        ))
    }
}

/// 把状态机派生的 [`CloseOutcome`] 映射成对外回执的两个字段。
///
/// 这是 `state` / `resource_release` 的**唯一**产出口径：两条关闭路径（F11 的未确认提前
/// 返回、CM-74 的确认关闭）都走这里，不再各写一份 `SessionState::Closing` +
/// `ResourceRelease::Pending` 的字面量 —— 同一个结论的第二份弱判据正是 §9.4(b) 登记的
/// 缺陷形状（「记录一个量、消费另一个量」）。
///
/// 映射在本夹具里是无损的：`prepare_close` 的 unconfirmed 分支显式把状态钉成
/// `FakeResourceState::Closing`，所以 `CloseOutcome::CloseUnconfirmed` 从这里出来时
/// 资源必然停在 `Closing`（不是 `Quarantined` / `Lost` —— 那两种形状由
/// `transaction_operation` 的 F8 与 §4.2 F3 负责，不走 closeResource）。
/// §5.3 规则 3 要求「未证实关闭不能伪称预算已回收」，因此 `Pending` 而不是 `Confirmed`。
fn close_receipt(
    db_session_id: DbSessionId,
    outcome: CloseOutcome,
    effect_outcome: EffectOutcome,
) -> CloseReceipt {
    let (state, resource_release) = match outcome {
        CloseOutcome::Closed => (SessionState::Closed, ResourceRelease::Confirmed),
        CloseOutcome::CloseUnconfirmed => (SessionState::Closing, ResourceRelease::Pending),
    };
    CloseReceipt {
        db_session_id,
        state,
        effect_outcome,
        resource_release,
    }
}
