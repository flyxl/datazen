//! 会话与句柄投影类型（connection-management.md §4 的会话半部、§6.5 句柄登记）。
//!
//! 拆分理由：`types.rs` 只放「标识、目标与归属」，本文件放「会话状态与句柄」，
//! `execution.rs` 放「执行与结果」。三者按依赖方向单向向下，禁止互相回指。

use serde::{Deserialize, Serialize};

use super::types::{
    ConfigRevision, ConnectionId, Counter, DbSessionId, ExecutionId, ExecutionTarget, HandleId,
    NamespaceTarget, OwnerRef, ResourceId, Timestamp,
};

/// 会话句柄。`runtimeEpoch` 每次换 owner 必增；陈旧 epoch 的句柄必须被拒绝（CM-71）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHandle {
    pub db_session_id: DbSessionId,
    pub runtime_epoch: Counter,
}

/// 句柄类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HandleKind {
    Transaction,
    Cursor,
    ServerPrepared,
}

impl HandleKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            HandleKind::Transaction => "transaction",
            HandleKind::Cursor => "cursor",
            HandleKind::ServerPrepared => "serverPrepared",
        }
    }
}

/// 会话级句柄引用（connection-management.md §6.5）。
///
/// 必须在 execution 返回终态**之前**登记到 session actor；未登记 = 非法，
/// runtime 拒绝把它交给宿主。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHandleRef {
    pub handle_id: HandleId,
    pub kind: HandleKind,
    pub resource_id: ResourceId,
    pub runtime_epoch: Counter,
    pub closed: bool,
}

impl SessionHandleRef {
    pub fn new(
        handle_id: HandleId,
        kind: HandleKind,
        resource_id: ResourceId,
        runtime_epoch: Counter,
    ) -> Self {
        Self {
            handle_id,
            kind,
            resource_id,
            runtime_epoch,
            closed: false,
        }
    }
}

/// 事务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum TransactionState {
    #[default]
    None,
    Active,
    Aborted,
    Unknown,
    Unsupported,
}

/// 上下文观测置信度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextConfidence {
    Confirmed,
    Partial,
    Unknown,
}

/// 会话上下文投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    pub namespace: NamespaceTarget,
    pub search_path: Vec<String>,
    pub effective_identity: String,
    pub transaction_state: TransactionState,
    pub autocommit: bool,
    pub confidence: ContextConfidence,
}

impl SessionContext {
    pub fn new(namespace: NamespaceTarget, effective_identity: impl Into<String>) -> Self {
        Self {
            namespace,
            search_path: Vec::new(),
            effective_identity: effective_identity.into(),
            transaction_state: TransactionState::None,
            autocommit: true,
            confidence: ContextConfidence::Confirmed,
        }
    }

    /// 「观测为 unknown」形态：**禁止**回填 `initialTarget` 假充确认（connection-management.md §5.1 L424）。
    pub fn unknown(initial: &NamespaceTarget, effective_identity: impl Into<String>) -> Self {
        let mut ctx = Self::new(initial.clone(), effective_identity);
        ctx.confidence = ContextConfidence::Unknown;
        ctx.transaction_state = TransactionState::Unknown;
        ctx
    }
}

/// 附着状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentState {
    Attached,
    Detached,
    Expired,
}

/// 会话状态机（connection-management.md §6.1）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SessionState {
    New,
    Opening,
    Ready,
    Executing,
    Reconfiguring,
    Closing,
    Closed,
    Lost,
}

/// 会话视图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub handle: SessionHandle,
    pub connection_id: ConnectionId,
    pub config_revision: ConfigRevision,
    pub owner: OwnerRef,
    pub initial_target: ExecutionTarget,
    pub observed_context: SessionContext,
    pub context_revision: Counter,
    pub state: SessionState,
    pub attachment_state: AttachmentState,
    pub active_execution_id: Option<ExecutionId>,
    pub expires_at: Timestamp,
}

/// `CommandCall` —— 统一命令调用。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandCall {
    pub command: String,
    pub input: serde_json::Value,
}

/// 在会话内执行。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteInSessionRequest {
    pub handle: SessionHandle,
    pub expected_context_revision: Counter,
    pub call: CommandCall,
    pub idempotency_key: String,
}

/// 在目标上执行（**没有**会话句柄的路径）。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecuteAtTargetRequest {
    pub target: ExecutionTarget,
    pub call: CommandCall,
    pub idempotency_key: String,
}

/// 关闭模式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CloseMode {
    RequireNoTransaction,
    RollbackAndClose,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::types::{JobId, OrganizationId};

    #[test]
    fn handle_kind_literals_match_the_protocol_map() {
        assert_eq!(HandleKind::Transaction.as_str(), "transaction");
        assert_eq!(HandleKind::Cursor.as_str(), "cursor");
        assert_eq!(HandleKind::ServerPrepared.as_str(), "serverPrepared");
    }

    #[test]
    fn a_fresh_handle_ref_starts_unclosed_and_carries_its_epoch() {
        let id = HandleId::new("hnd-1");
        let resource = ResourceId::new("res_w1_0001");
        let epoch = Counter::new(1);
        let handle =
            SessionHandleRef::new(id.clone(), HandleKind::Transaction, resource.clone(), epoch);
        assert!(!handle.closed, "新建句柄未终结");
        assert_eq!(handle.handle_id, id);
        assert_eq!(handle.resource_id, resource);
        assert_eq!(handle.runtime_epoch, epoch);
    }

    #[test]
    fn unknown_observation_marks_both_context_and_transaction_unknown() {
        // §3.2 observeSession：可置任一字段为 unknown，且禁止回填 initialTarget 假充确认。
        let initial = NamespaceTarget {
            database: "dz_ns_a".into(),
            catalog: String::new(),
            schema: "public".into(),
            path: String::new(),
        };
        let ctx = SessionContext::unknown(&initial, "exec-identity-shared");
        assert_eq!(ctx.confidence, ContextConfidence::Unknown);
        assert_eq!(ctx.transaction_state, TransactionState::Unknown);
        assert_ne!(
            ctx.confidence,
            ContextConfidence::Confirmed,
            "观测为 unknown 时不得声称 confirmed"
        );
    }

    #[test]
    fn confirmed_context_is_the_default_shape() {
        let ctx = SessionContext::new(NamespaceTarget::default(), "exec-identity-shared");
        assert_eq!(ctx.confidence, ContextConfidence::Confirmed);
        assert_eq!(ctx.transaction_state, TransactionState::None);
        assert!(ctx.autocommit);
    }

    #[test]
    fn session_state_covers_the_documented_lifecycle() {
        // §6.1：new → opening → ready → executing → reconfiguring → closing → closed / lost。
        let all = [
            SessionState::New,
            SessionState::Opening,
            SessionState::Ready,
            SessionState::Executing,
            SessionState::Reconfiguring,
            SessionState::Closing,
            SessionState::Closed,
            SessionState::Lost,
        ];
        assert_eq!(all.len(), 8);
        assert_ne!(all[0], all[7]);
    }

    #[test]
    fn session_view_keeps_configuration_and_session_ids_apart() {
        // CM-04：SessionView 上并存的是 connectionId（持久化配置）与 dbSessionId（内存态会话）。
        let view = SessionView {
            handle: SessionHandle {
                db_session_id: DbSessionId::new("dbs_w1_0001"),
                runtime_epoch: Counter::new(1),
            },
            connection_id: ConnectionId::new("conn-fixture-p"),
            config_revision: ConfigRevision::new(7),
            owner: OwnerRef::Job {
                organization_id: OrganizationId::new("org-alpha"),
                job_id: JobId::new("job_org-alpha_0001"),
                stage_id: "job:job_org-alpha_0001/stage:1".into(),
            },
            // `initial_target` 是 `ExecutionTarget`（§4：绑定的执行目标），
            // 与 `observed_context.namespace`（命名空间）不是同一个类型。
            initial_target: ExecutionTarget {
                connection_id: ConnectionId::new("conn-fixture-p"),
                namespace: NamespaceTarget::default(),
                object: None,
            },
            observed_context: SessionContext::new(
                NamespaceTarget::default(),
                "exec-identity-shared",
            ),
            context_revision: Counter::new(0),
            state: SessionState::Ready,
            attachment_state: AttachmentState::Attached,
            active_execution_id: None,
            expires_at: Timestamp::new("2026-01-01T00:30:00Z"),
        };
        assert_eq!(view.connection_id.as_str(), "conn-fixture-p");
        assert_eq!(view.handle.db_session_id.as_str(), "dbs_w1_0001");
        assert_ne!(
            view.connection_id.as_str(),
            view.handle.db_session_id.as_str()
        );
    }

    #[test]
    fn close_mode_literals_are_the_documented_pair() {
        let json = serde_json::to_string(&CloseMode::RequireNoTransaction).expect("serialize");
        assert_eq!(json, "\"requireNoTransaction\"");
        let json = serde_json::to_string(&CloseMode::RollbackAndClose).expect("serialize");
        assert_eq!(json, "\"rollbackAndClose\"");
    }

    #[test]
    fn execute_requests_expose_exactly_the_id_terminology_they_need() {
        let in_session = ExecuteInSessionRequest {
            handle: SessionHandle {
                db_session_id: DbSessionId::new("dbs_w1_0001"),
                runtime_epoch: Counter::new(1),
            },
            expected_context_revision: Counter::new(2),
            call: CommandCall {
                command: "select_rows".into(),
                input: serde_json::json!({}),
            },
            idempotency_key: "idem-1".into(),
        };
        // 已在会话上的操作只带 dbSessionId；配置语义由 connectionId 承担，两者不混用。
        assert_eq!(in_session.handle.db_session_id.as_str(), "dbs_w1_0001");

        let at_target = ExecuteAtTargetRequest {
            target: ExecutionTarget {
                connection_id: ConnectionId::new("conn-fixture-p"),
                namespace: NamespaceTarget::default(),
                object: None,
            },
            call: CommandCall {
                command: "select_rows".into(),
                input: serde_json::json!({}),
            },
            idempotency_key: "idem-2".into(),
        };
        assert_eq!(at_target.target.connection_id.as_str(), "conn-fixture-p");
    }
}
