//! op 9 `closeResource` 的实现（fake-runtime-fixtures.md §3.1、§5.3 规则 2/3/6、
//! connection-management.md CM-74 / §9.4）。
//!
//! 从 `ops.rs` **整段**搬出来，行为逐字不变。这一步只做两件事：
//!
//! 1. 让 op 9 的 ~160 行有一个自己的落点（`ops.rs` 此前同时承载九个操作，
//!    已经贴近 AGENTS.md 的单文件规模红线；`transaction.rs` 已有同构先例）；
//! 2. 后面几项修复（F11 覆盖、F12 死分支、F13 判据双源、F14 重复记账）都在
//!    这一个文件里发生，评审时不必在九个操作之间跳。
//!
//! 依赖方向不变：只向下依赖 `connection::{port, session, types, execution, error}`
//! 与同目录的 `handles` / `state` / `script`，仍然不写任何网络或凭据代码（§13）。

use crate::connection::error::ProviderError;
use crate::connection::execution::EffectOutcome;
use crate::connection::port::{CloseReceipt, CloseResourceRequest, ResourceRelease};
use crate::connection::session::{SessionState, TransactionState};
use crate::connection::testing::journal::{HandleAction, ResourceEvent};

use super::script::{FaultKind, ResourceOp};
use super::state::FakeResourceState;
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
        // 幂等关闭：资源可能已经是 `Closed`，所以这里 `allow_closed = true`。
        let resource = self.resolve(&request.handle.resource_id, &request.handle, true)?;
        let key = request.handle.resource_id.as_str().to_owned();
        let had_transaction = resource.has_open_transaction();

        // F11：关闭未确认 —— 资源留在预算占用里（§5.3 规则 3：余额不变）。
        if let Some((_, FaultKind::CloseUnconfirmed { .. })) = self.script.take(ResourceOp::Close) {
            {
                let mut resources = self.lock();
                if let Some(slot) = resources.get_mut(&key) {
                    slot.state = FakeResourceState::Closing;
                    slot.protocol_drained = false;
                }
            }
            if let Some(owner) = resource.owner.clone() {
                self.journal.record_resource_event(
                    &request.handle.resource_id,
                    ResourceEvent::CloseUnconfirmed,
                    &owner,
                    resource.pool_key.clone(),
                    resource.budget_class(),
                );
            }
            return Ok(CloseReceipt {
                db_session_id: resource.db_session_id.clone(),
                state: SessionState::Closing,
                effect_outcome: EffectOutcome::Unknown,
                resource_release: ResourceRelease::Pending,
            });
        }

        // CM-74：句柄注销与归池前置判定合并进**同一个临界区**，次序固定为
        // 「取注销前快照 → 注销句柄 → 判归池」。
        //
        // 判归池读的是**注销前**的快照，这是判据的字面要求而非取巧：CM-74 对同一条断言
        // 既要求句柄先于物理关闭注销，又要求「driver 报 `Clean` 而宿主仍有已登记句柄时
        // 宿主检查必须失败（§9.4）」。后者问的是「关闭开始前资源上挂没挂着句柄」，
        // 不是「注销之后还剩几个」。两个判据都是关闭**之前**的事实，
        // 所以快照与注销同处一把锁 —— 中间没有观察者，也就没有 TOCTOU。
        let rolled_back = self.script.rollback_before_release();
        let (drained, handles_before_close, closed) = {
            let mut resources = self.lock();
            match resources.get_mut(&key) {
                Some(slot) => {
                    slot.protocol_drained = request.protocol_drained && slot.protocol_drained;
                    let drained = slot.protocol_drained;
                    // 归池判定的输入：注销**之前**这张资源上还挂着几个句柄。
                    let handles_before_close = slot.registered_handles();
                    // CM-74：句柄随资源一起死，必须在物理关闭前注销（journal 先记 `closed`）。
                    // 这一步**无条件** —— 它是关闭语义，不是回滚语义。
                    let closed: Vec<_> = slot
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
                    (drained, handles_before_close, closed)
                }
                None => (
                    request.protocol_drained,
                    resource.registered_handles(),
                    Vec::new(),
                ),
            }
        };
        for open in &closed {
            self.journal.record_handle(
                open,
                HandleAction::Closed,
                "物理关闭前注销句柄（CM-74 / §9.3）",
            );
        }
        if let Some(owner) = resource.owner.clone() {
            // §5.3 规则 2 前置：归池必须有「协议已排空 + 无登记句柄」的证据。
            // 「无登记句柄」按 CM-74 §9.4 读**关闭前**的快照 `handles_before_close`，
            // 而不是刚注销完的 `registered_handles()` —— 后者恒为 0，读它等于没判。
            if drained && handles_before_close == 0 {
                self.journal.record_resource_event(
                    &request.handle.resource_id,
                    ResourceEvent::ReturnedToPool {
                        protocol_drained: true,
                        registered_handles: handles_before_close,
                    },
                    &owner,
                    resource.pool_key.clone(),
                    resource.budget_class(),
                );
            }
            self.journal.record_resource_event(
                &request.handle.resource_id,
                ResourceEvent::Closed,
                &owner,
                resource.pool_key.clone(),
                resource.budget_class(),
            );
        }

        // 归还 permit —— 只在 `Closed` 时，且 `Accounting` 保证至多一次。
        let release = {
            let mut resources = self.lock();
            resources.get_mut(&key).and_then(|slot| {
                slot.accounting.release().map(|plan| {
                    (
                        plan,
                        slot.accounting.permit_id.clone(),
                        slot.accounting.budget_class,
                    )
                })
            })
        };
        if let Some((plan, permit_id, budget_class)) = release {
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
        self.journal.close_active_session(&resource.db_session_id);
        for lease in self.leases_of(&request.handle.resource_id) {
            self.journal.release_lease(&lease);
        }

        Ok(CloseReceipt {
            db_session_id: resource.db_session_id.clone(),
            state: SessionState::Closed,
            // 诚实：真的回滚过才报 `RolledBack`。
            effect_outcome: if rolled_back || had_transaction {
                EffectOutcome::RolledBack
            } else {
                EffectOutcome::Completed
            },
            resource_release: ResourceRelease::Confirmed,
        })
    }
}
