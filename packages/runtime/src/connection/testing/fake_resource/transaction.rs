//! `transactionOperation` 的实现。
//!
//! 九个操作里事务这一条最重：它同时承载三条硬规则
//! （不可判定 ⇒ `effectOutcome = Unknown`、永不 `Completed`；回滚失败 ⇒ 隔离），
//! 以及「隔离**保留预算占用**」形状，所以单独成文件，不和
//! `ops.rs` 里其余八个操作挤在一起。
//!
//! 判定仍走 `FakeResourceProvider::resolve`（`resourceId` + `runtimeEpoch` + owner 三校验），
//! 故障注入只改返回值、状态迁移与 journal 写入照常 —— 与 `ops.rs` 的其余操作同一条纪律。

use crate::connection::error::ProviderError;
use crate::connection::execution::EffectOutcome;
use crate::connection::port::{
    CompletionStatus, ExecutionCompletion, ResourceHandle, TransactionObservation,
    TransactionOperation,
};
use crate::connection::session::TransactionState;
use crate::connection::testing::journal::{HandleAction, ResourceEvent};

use super::script::{execution_error_code, FaultKind, ResourceOp};
use super::FakeResourceProvider;

impl FakeResourceProvider {
    // ---- 6. transactionOperation ----

    /// `begin` / `commit` / `rollback`。
    ///
    /// 提交不可判定时 `effectOutcome` 必须是 `Unknown`；
    /// 回滚注入 `RollbackFailed` 时资源进 `Quarantined` 且**保留预算占用**。
    pub fn transaction_operation(
        &self,
        handle: &ResourceHandle,
        operation: TransactionOperation,
    ) -> Result<ExecutionCompletion, ProviderError> {
        let resource = self.resolve(&handle.resource_id, handle, false)?;
        let key = handle.resource_id.as_str().to_owned();
        let handle_id = match &operation {
            TransactionOperation::Begin => None,
            TransactionOperation::Commit(handle_id) | TransactionOperation::Rollback(handle_id) => {
                Some(handle_id.clone())
            }
        };

        // 回滚失败 → 资源隔离，**不归还 permit**，余额保持不变。
        if let TransactionOperation::Rollback(_) = &operation {
            if let Some((_, FaultKind::RollbackFailed { reason })) =
                self.script.take(ResourceOp::Transaction)
            {
                {
                    let mut resources = self.lock();
                    if let Some(slot) = resources.get_mut(&key) {
                        slot.quarantine();
                    }
                }
                if let Some(owner) = resource.owner.clone() {
                    self.journal.record_resource_event(
                        &handle.resource_id,
                        ResourceEvent::Quarantined,
                        &owner,
                        resource.pool_key.clone(),
                        resource.budget_class(),
                    );
                }
                return Err(ProviderError::RollbackFailed(reason));
            }
        }

        let mut committed_unknown = None;
        {
            let mut resources = self.lock();
            let slot = resources
                .get_mut(&key)
                .ok_or_else(|| ProviderError::SessionLost(format!("资源 {key} 不存在")))?;
            match &operation {
                TransactionOperation::Begin => slot.transaction_state = TransactionState::Active,
                TransactionOperation::Commit(_) => {
                    // 提交结果不可判定。
                    if let Some((_, FaultKind::CommitUnknown { code })) =
                        self.script.take(ResourceOp::Transaction)
                    {
                        let code = execution_error_code(code);
                        // 硬规则：`Unknown` ⟹ `effectOutcome == unknown`，**永不** `completed`。
                        slot.transaction_state = TransactionState::Unknown;
                        slot.last_error_code = Some(code);
                        committed_unknown = Some(code);
                    } else {
                        slot.transaction_state = TransactionState::None;
                    }
                }
                TransactionOperation::Rollback(_) => {
                    slot.transaction_state = TransactionState::None
                }
            }
        }

        // 提交 / 回滚之后注销事务句柄（移出登记册并写 `closed` 事件）。
        // 提交不可判定时**保留**句柄登记 —— 句柄的最终归属未知，不能假装已经注销。
        if let (Some(handle_id), None) = (&handle_id, committed_unknown) {
            let deregistered = {
                let mut resources = self.lock();
                resources
                    .get_mut(&key)
                    .and_then(|slot| slot.deregister_handle(handle_id.as_str()))
            };
            if let Some(mut open) = deregistered {
                open.closed = true;
                self.journal
                    .record_handle(&open, HandleAction::Closed, "事务终结后注销句柄");
            }
        }

        if let Some(code) = committed_unknown {
            return Ok(ExecutionCompletion {
                completion_status: CompletionStatus::Error,
                error_code: Some(code),
                effect_outcome: EffectOutcome::Unknown,
                context_before: Some(resource.context.clone()),
                context_after: Some(resource.context.clone()),
                transaction_observation: TransactionObservation::Unknown,
                resource_health: resource.health,
                ..ExecutionCompletion::ok()
            });
        }
        Ok(ExecutionCompletion {
            context_before: Some(resource.context.clone()),
            context_after: Some(resource.context.clone()),
            resource_health: resource.health,
            ..ExecutionCompletion::ok()
        })
    }
}
