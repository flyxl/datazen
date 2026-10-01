//! 执行契约：状态、结果来源、能力快照与实时 `ExecutionView`。
//!
//! §4.4 的硬规则：**`ExecutionView` 是实时响应**，仓储不得直接序列化它；
//! 落盘一律走 `application::dto::DurableExecutionRecord`。`RuntimeResultBinding`
//! 只由执行 registry 在内存附加，资源丢失/关闭/替换立即失效。

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::dto::session::{SessionContext, SessionHandle};
use crate::id::{
    ArtifactId, CapabilityRevision, ConfigRevision, ConnectionId, ExecutionId, OrganizationId,
    PrincipalId, ResourceId, StreamId, Timestamp,
};
use crate::target::ExecutionTarget;

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
    pub capability_revision: CapabilityRevision,
    pub confirmed: BTreeMap<String, serde_json::Value>,
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
            capability_revision: CapabilityRevision::new(0),
            confirmed: BTreeMap::new(),
        }
    }

    pub fn with_confirmed(mut self, key: impl Into<String>, value: serde_json::Value) -> Self {
        self.confirmed.insert(key.into(), value);
        self
    }
}

/// 执行来源。`context_before`/`context_after` 覆盖整次调用；
/// 批次内每个结果另由 `StatementResultSource` 记录，**不能都贴最终上下文**（§4.1 末段）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultProvenance {
    pub organization_id: OrganizationId,
    pub principal_id: PrincipalId,
    pub connection_id: ConnectionId,
    pub config_revision: ConfigRevision,
    pub context_before: SessionContext,
    pub context_after: SessionContext,
    pub requested_target: ExecutionTarget,
    pub capability_snapshot: CapabilitySnapshot,
    pub executed_at: Timestamp,
}

/// 单条语句结果的来源。驱动无法观察批次内准确上下文时 `relation=null` 且 readOnly，
/// **不猜测** relation，也不提供可写映射。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StatementResultSource {
    pub execution_id: ExecutionId,
    pub statement_index: u32,
    pub context: SessionContext,
    pub relation: Option<ExecutionTarget>,
    pub writable_mapping: WritableMapping,
}

/// 可写映射的确认强度。`ReadOnly` 是**诚实降级**，不是失败。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WritableMapping {
    Verified,
    ReadOnly,
}

/// 已接受执行的回执。**已接受 ≠ SQL 成功**：本回执只表示执行记录已建立（§13.1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionReceipt {
    pub execution_id: ExecutionId,
    pub stream_id: StreamId,
    pub state: ExecutionState,
}

/// 实时响应专用的运行时绑定。**永不落盘**（§4.4）：
/// 资源丢失、关闭或替换后立即失效，调用方必须拿到 `SessionLost`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeResultBinding {
    pub handle: SessionHandle,
    pub resource_binding_id: ResourceId,
    pub execution_id: ExecutionId,
}

/// 实时执行视图。**不得直接序列化进持久化存储**——仓储用
/// `application::dto::DurableExecutionRecord`，事件归档用显式白名单投影。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionView {
    pub execution_id: ExecutionId,
    pub state: ExecutionState,
    pub effect_outcome: EffectOutcome,
    pub provenance: Option<ResultProvenance>,
    pub artifact_ids: Vec<ArtifactId>,
    pub result_completeness: ResultCompleteness,
    pub truncation_reason: Option<String>,
    pub error_code: Option<ExecutionErrorCode>,
    /// 仅内存附加；资源丢失/关闭/替换立即失效。
    pub runtime_binding: Option<RuntimeResultBinding>,
}

impl ExecutionView {
    /// 是否仍持有运行时绑定。为 false 时任何「继续读流 / 继续用句柄」的请求都不成立。
    pub fn has_live_runtime(&self) -> bool {
        self.runtime_binding.is_some()
    }
}

/// 事件序号别名。见 [`crate::dto::event::EventSequence`]。

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::session::{ContextConfidence, TransactionState};
    use crate::id::RuntimeEpoch;
    use crate::target::NamespaceTarget;
    use serde_json::json;

    fn session_context() -> SessionContext {
        SessionContext {
            namespace: NamespaceTarget::database("app"),
            search_path: None,
            effective_identity: Some("app_user".into()),
            transaction_state: TransactionState::None,
            autocommit: Some(true),
            confidence: ContextConfidence::Confirmed,
        }
    }

    fn provenance() -> ResultProvenance {
        ResultProvenance {
            organization_id: OrganizationId::new("org-1"),
            principal_id: PrincipalId::new("user-1"),
            connection_id: ConnectionId::new("conn-1"),
            config_revision: ConfigRevision::new(2),
            context_before: session_context(),
            context_after: session_context(),
            requested_target: ExecutionTarget::new(
                ConnectionId::new("conn-1"),
                NamespaceTarget::database("app"),
            ),
            capability_snapshot: CapabilitySnapshot::new("postgres", "1.2.3", 4)
                .with_confirmed("preciseCancel", json!(true)),
            executed_at: Timestamp::new("2026-01-01T00:00:00Z"),
        }
    }

    fn execution_view() -> ExecutionView {
        ExecutionView {
            execution_id: ExecutionId::new("exec-1"),
            state: ExecutionState::Succeeded,
            effect_outcome: EffectOutcome::Completed,
            provenance: Some(provenance()),
            artifact_ids: vec![ArtifactId::new("art-1")],
            result_completeness: ResultCompleteness::Complete,
            truncation_reason: None,
            error_code: None,
            runtime_binding: Some(RuntimeResultBinding {
                handle: SessionHandle {
                    db_session_id: crate::id::DbSessionId::new("sess-1"),
                    runtime_epoch: RuntimeEpoch::new("epoch-1"),
                },
                resource_binding_id: ResourceId::new("res-1"),
                execution_id: ExecutionId::new("exec-1"),
            }),
        }
    }

    #[test]
    fn execution_view_round_trips_with_and_without_provenance() {
        let original = execution_view();
        let value = serde_json::to_value(&original).expect("serialize");
        assert_eq!(value["state"], json!("succeeded"));
        assert_eq!(value["effectOutcome"], json!("completed"));
        assert_eq!(value["resultCompleteness"], json!("complete"));
        assert_eq!(
            value["provenance"]["capabilitySnapshot"]["capabilityRevision"],
            json!("0")
        );
        assert_eq!(
            value["provenance"]["capabilitySnapshot"]["protocolVersion"],
            json!(4)
        );
        assert_eq!(
            serde_json::from_value::<ExecutionView>(value).expect("deserialize"),
            original
        );

        let mut bare = execution_view();
        bare.provenance = None;
        let value = serde_json::to_value(&bare).expect("serialize");
        assert_eq!(value["provenance"], json!(null));
        assert_eq!(
            serde_json::from_value::<ExecutionView>(value).expect("deserialize"),
            bare
        );
    }

    #[test]
    fn runtime_binding_is_the_only_runtime_only_field() {
        // §4.4：只有 runtimeBinding 与其中的句柄是运行时值。
        let value = serde_json::to_value(execution_view()).expect("serialize");
        assert_eq!(
            value["runtimeBinding"]["handle"]["dbSessionId"],
            json!("sess-1")
        );
        assert_eq!(value["runtimeBinding"]["resourceBindingId"], json!("res-1"));

        let mut detached = execution_view();
        detached.runtime_binding = None;
        assert!(!detached.has_live_runtime());
        let value = serde_json::to_value(&detached).expect("serialize");
        assert_eq!(value["runtimeBinding"], json!(null));
    }

    #[test]
    fn statement_result_source_keeps_per_statement_context() {
        let source = StatementResultSource {
            execution_id: ExecutionId::new("exec-1"),
            statement_index: 1,
            context: session_context(),
            relation: Some(ExecutionTarget::new(
                ConnectionId::new("conn-1"),
                NamespaceTarget::database("app"),
            )),
            writable_mapping: WritableMapping::ReadOnly,
        };
        let value = serde_json::to_value(&source).expect("serialize");
        assert_eq!(value["statementIndex"], json!(1));
        assert_eq!(value["writableMapping"], json!("readOnly"));
        assert_eq!(
            serde_json::from_value::<StatementResultSource>(value).expect("deserialize"),
            source
        );

        let unknown_relation = StatementResultSource {
            relation: None,
            ..source.clone()
        };
        let value = serde_json::to_value(&unknown_relation).expect("serialize");
        assert_eq!(value["relation"], json!(null));
        assert_eq!(
            serde_json::from_value::<StatementResultSource>(value).expect("deserialize"),
            unknown_relation
        );
    }

    #[test]
    fn execution_receipt_round_trips() {
        let receipt = ExecutionReceipt {
            execution_id: ExecutionId::new("exec-1"),
            stream_id: StreamId::new("stream-1"),
            state: ExecutionState::Queued,
        };
        let value = serde_json::to_value(&receipt).expect("serialize");
        assert_eq!(value["state"], json!("queued"));
        assert_eq!(
            serde_json::from_value::<ExecutionReceipt>(value).expect("deserialize"),
            receipt
        );
    }

    #[test]
    fn terminal_states_and_error_codes_are_independent_namespaces() {
        assert!(ExecutionState::Succeeded.is_terminal());
        assert!(ExecutionState::Cancelled.is_terminal());
        assert!(
            !ExecutionState::CancelRequested.is_terminal(),
            "取消请求已送达 ≠ 终态"
        );
        assert!(!ExecutionState::Running.is_terminal());
        // sqlError 与四个 effectOutcome 正交，由类型本身承载（两个独立字段）。
        let failed = ExecutionView {
            state: ExecutionState::Failed,
            effect_outcome: EffectOutcome::RolledBack,
            error_code: Some(ExecutionErrorCode::SqlError),
            ..execution_view()
        };
        let value = serde_json::to_value(&failed).expect("serialize");
        assert_eq!(value["errorCode"], json!("sqlError"));
        assert_eq!(value["effectOutcome"], json!("rolledBack"));
    }

    #[test]
    fn capability_snapshot_starts_empty_and_serializes_confirmed_values() {
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
}
