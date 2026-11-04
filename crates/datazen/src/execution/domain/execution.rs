//! 执行域词汇：状态机、取消结局、错误码与执行来源（原 platform-api
//! `dto/execution.rs` 的域侧）。
//!
//! 状态机规则（终态不可被取消改写、未知结局不给终态）随类型驻留在本层；
//! 实时视图与回执（`ExecutionView`、`ExecutionReceipt`、`ResultChunk`）在
//! [`crate::execution::application`](super::super::application)。

use serde::{Deserialize, Serialize};

/// 数据库实际生效范围。与 [`ExecutionErrorCode`] **正交**：
/// `sqlError` 可以对应 `completed`/`rolledBack`/`partiallyApplied`/`unknown` 任一（§13.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EffectOutcome {
    NotStarted,
    Completed,
    RolledBack,
    PartiallyApplied,
    /// 超时、协议错误、取消或连接丢失导致生效范围无法判定时**必须**是 `Unknown`。
    Unknown,
}

/// 执行状态机。`CancelRequested` 只表示取消请求已送达驱动，
/// **不构成「写入已回滚」的证据**。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionState {
    Queued,
    Running,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
}

impl ExecutionState {
    /// 是否已进入终态。只有终态执行才允许回收会话的执行占用。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }

    /// 用取消结局解析终态。
    ///
    /// **P3 目标契约**（CM-22）。要点：**取消请求不得覆盖实际执行终态**。取消本身不是终态，
    /// 只有驱动**确认**回滚后才是。
    pub fn resolve_cancel(self, outcome: CancelOutcome) -> Self {
        // 已经终态的执行不被取消改写：先到的证据有效，但它必须是真的。
        if self.is_terminal() {
            return self;
        }
        match outcome {
            CancelOutcome::ConfirmedRolledBack => Self::Cancelled,
            // 不支持取消：状态原样继续跑，不得顺手标成 cancelled。
            CancelOutcome::Unsupported => self,
            // 取消生效前已完成。
            CancelOutcome::CompletedFirst => Self::Succeeded,
            // 结局未知**不是**「已取消」。它停在非终态，侧效应用
            // `EffectOutcome::Unknown` 表达——不能伪称已回滚。
            CancelOutcome::OutcomeUnknown => Self::CancelRequested,
        }
    }
}

/// 取消请求的四种结局。CM-22 的四种竞态编排各对应一个取值。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CancelOutcome {
    /// 驱动确认写入已回滚 → 终态 `cancelled`。
    ConfirmedRolledBack,
    /// 取消不被该驱动/协议支持 → **不改变状态**。
    Unsupported,
    /// 执行在取消生效前已正常完成 → 终态 `succeeded`。
    CompletedFirst,
    /// 生效范围无法判定 → 不给终态；侧效应必须记 `EffectOutcome::Unknown`。
    OutcomeUnknown,
}

/// 版本化的执行终态错误码，与 `ApiError.code` 是**两个独立命名空间**：
/// 派发后的终态失败只由 `ExecutionState='failed'` + 本枚举 + 事件表达，不产生 `ApiError`。
/// 新增取值需要提升枚举版本并同时更新全部消费者。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionErrorCode {
    SqlError,
    ProtocolError,
    Cancelled,
    Timeout,
    ResourceLost,
    /// 至少为 `partiallyApplied` 或 `unknown`（§13.1）。
    PipelineAborted,
    /// 已建立执行记录后由宿主二次校验拒绝。
    HostRejected,
}

/// 结果完整性。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResultCompleteness {
    Pending,
    Complete,
    Truncated,
}

/// 驱动能力快照：本次调用**实际确认**的非敏感能力。
/// 保存驱动/协议/能力版本作为计划核验与审计证据，**不保存凭据**（§4.4）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilitySnapshot {
    pub driver_id: String,
    pub driver_version: String,
    pub protocol_version: u32,
    pub capability_revision: crate::shared::id::CapabilityRevision,
    pub confirmed: std::collections::BTreeMap<String, serde_json::Value>,
}

impl CapabilitySnapshot {
    pub fn new(
        driver_id: impl Into<String>,
        driver_version: impl Into<String>,
        protocol_version: u32,
    ) -> Self {
        Self {
            driver_id: driver_id.into(),
            driver_version: driver_version.into(),
            protocol_version,
            capability_revision: crate::shared::id::CapabilityRevision::new(0),
            confirmed: std::collections::BTreeMap::new(),
        }
    }

    pub fn with_confirmed(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.confirmed.insert(key.into(), value);
        self
    }
}

/// 执行来源。`context_before`/`context_after` 覆盖整次调用；
/// 批次内每个结果另由 [`StatementResultSource`] 记录，**不能都贴最终上下文**（§4.1 末段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultProvenance {
    pub organization_id: crate::shared::id::OrganizationId,
    pub principal_id: crate::shared::id::PrincipalId,
    pub connection_id: crate::shared::id::ConnectionId,
    pub config_revision: crate::shared::id::ConfigRevision,
    pub context_before: super::session::SessionContext,
    pub context_after: super::session::SessionContext,
    pub requested_target: super::target::ExecutionTarget,
    pub capability_snapshot: CapabilitySnapshot,
    pub executed_at: crate::shared::id::Timestamp,
}

/// 单条语句结果的来源。驱动无法观察批次内准确上下文时 `relation=null` 且 readOnly，
/// **不猜测** relation，也不提供可写映射。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementResultSource {
    pub execution_id: crate::shared::id::ExecutionId,
    pub statement_index: u32,
    pub context: super::session::SessionContext,
    pub relation: Option<super::target::ExecutionTarget>,
    pub writable_mapping: WritableMapping,
}

/// 可写映射的确认强度。`ReadOnly` 是**诚实降级**，不是失败。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WritableMapping {
    Verified,
    ReadOnly,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_states_and_error_codes_are_independent_namespaces() {
        assert!(ExecutionState::Succeeded.is_terminal());
        assert!(ExecutionState::Cancelled.is_terminal());
        assert!(
            !ExecutionState::CancelRequested.is_terminal(),
            "取消请求已送达 ≠ 终态"
        );
        assert!(!ExecutionState::Running.is_terminal());
    }

    #[test]
    fn a_cancel_request_can_never_overwrite_a_terminal_state() {
        // 竞态里唯一不可协商的规则：终态一旦确定，取消不改写它。
        for terminal in [
            ExecutionState::Succeeded,
            ExecutionState::Failed,
            ExecutionState::Cancelled,
        ] {
            for outcome in [
                CancelOutcome::ConfirmedRolledBack,
                CancelOutcome::Unsupported,
                CancelOutcome::CompletedFirst,
                CancelOutcome::OutcomeUnknown,
            ] {
                assert_eq!(
                    terminal.resolve_cancel(outcome),
                    terminal,
                    "{terminal:?} must survive a cancel resolved as {outcome:?}"
                );
            }
        }
    }

    #[test]
    fn only_a_confirmed_rollback_produces_the_cancelled_terminal_state() {
        let running = ExecutionState::Running;
        assert_eq!(
            running.resolve_cancel(CancelOutcome::ConfirmedRolledBack),
            ExecutionState::Cancelled
        );
        // 「不支持取消」必须原样返回，不得顺手标成 cancelled。
        assert_eq!(
            running.resolve_cancel(CancelOutcome::Unsupported),
            ExecutionState::Running
        );
        assert_eq!(
            running.resolve_cancel(CancelOutcome::CompletedFirst),
            ExecutionState::Succeeded
        );
    }

    #[test]
    fn an_unknown_cancel_outcome_is_never_reported_as_cancelled() {
        let resolved =
            ExecutionState::CancelRequested.resolve_cancel(CancelOutcome::OutcomeUnknown);
        assert_eq!(resolved, ExecutionState::CancelRequested);
        assert!(
            !resolved.is_terminal(),
            "未知结局不得产生终态：否则等于宣称已回滚"
        );
        assert_ne!(resolved, ExecutionState::Cancelled);
    }

    #[test]
    fn capability_snapshot_starts_empty_and_serializes_confirmed_values() {
        use serde_json::json;
        let snapshot = CapabilitySnapshot::new("redis", "0.9.0", 1)
            .with_confirmed("preciseCancel", json!(false))
            .with_confirmed("dbiIndexMapping", json!({"encoding": "base10"}));
        let value = serde_json::to_value(&snapshot).expect("serialize");
        assert_eq!(value["confirmed"]["preciseCancel"], json!(false));
        assert_eq!(
            value["confirmed"]["dbiIndexMapping"]["encoding"],
            json!("base10")
        );
        assert_eq!(
            serde_json::from_value::<CapabilitySnapshot>(value).expect("deserialize"),
            snapshot
        );
    }

    #[test]
    fn state_literals_stay_pinned_to_the_wire_format() {
        use serde_json::json;
        assert_eq!(
            serde_json::to_value(ExecutionState::CancelRequested).expect("s"),
            json!("cancelRequested")
        );
        assert_eq!(
            serde_json::to_value(EffectOutcome::PartiallyApplied).expect("s"),
            json!("partiallyApplied")
        );
        assert_eq!(
            serde_json::to_value(ExecutionErrorCode::PipelineAborted).expect("s"),
            json!("pipelineAborted")
        );
        assert_eq!(
            serde_json::to_value(ResultCompleteness::Truncated).expect("s"),
            json!("truncated")
        );
    }
}
