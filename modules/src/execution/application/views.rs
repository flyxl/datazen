//! 执行应用层视图与回执：`SessionView`、`ExecutionView`、回执家族与 `ResultChunk`。
//!
//! 原属 platform-api `dto/session.rs` / `dto/execution.rs` 的视图与回执部分，
//! D3（2026-10-26）起唯一定义在 application 层（§5.1：回执视图等服务边界 DTO
//! → execution::application）。状态机与来源词汇在
//! [`crate::execution::domain`](super::super::domain)。
//!
//! §4.4 的硬规则：**`ExecutionView` 是实时响应**，仓储不得直接序列化它；
//! 落盘一律走 `application::dto::DurableExecutionRecord`（runtime 侧，随机制迁移）。
//! `RuntimeResultBinding` 只由执行 registry 在内存附加，资源丢失/关闭/替换立即失效。

use serde::{Deserialize, Serialize};

use crate::execution::domain::target::ExecutionTarget;
use crate::execution::domain::{
    CancelDisposition, EffectOutcome, ExecutionErrorCode, ExecutionState, ResourceRelease,
    ResultCompleteness, ResultProvenance, SessionContext, SessionHandle, SessionState,
    StatementResultSource, AttachmentState,
};
use crate::shared::context::OwnerRef;
use crate::shared::id::{
    ArtifactId, ConnectionId, Counter, DbSessionId, ExecutionId, ResourceId, StreamId, Timestamp,
    AttachmentToken,
};

/// 会话视图：客户端可见的会话全貌。`activeExecutionId` 在执行中指向唯一执行
/// （INV-03：一个 session 一次一个常规执行）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub handle: SessionHandle,
    pub connection_id: ConnectionId,
    /// 配置版本。变更时该连接上所有会话都被判失效（租约保持固定，见 INV-02）。
    pub config_revision: Counter,
    pub owner: OwnerRef,
    pub initial_target: ExecutionTarget,
    pub observed_context: SessionContext,
    /// 上下文版本。`executeInSession` 用它做 CAS 冲突判定。
    pub context_revision: Counter,
    pub state: SessionState,
    pub attachment_state: AttachmentState,
    pub active_execution_id: Option<ExecutionId>,
    /// 单调 deadline；过期由后台清理回收，**不会**让长事务在建连期间被清理。
    pub expires_at: Option<Timestamp>,
}

impl SessionView {
    /// 是否满足「可以使用该 session 已确认的默认值」的前置条件：
    /// 会话可用、上下文已确认、当前没有正在执行的常规执行。
    pub fn session_defaults_usable(&self) -> bool {
        matches!(self.state, SessionState::Ready)
            && self.observed_context.confidence
                == crate::execution::domain::ContextConfidence::Confirmed
            && self.active_execution_id.is_none()
    }
}

/// 关闭会话回执。`state='lost'` 表示资源在关闭过程中丢失，
/// 此时 `effectOutcome` 可能是 `Unknown`——不得写成 `rolledBack`（§13.1 正交性规则）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseReceipt {
    pub db_session_id: DbSessionId,
    pub state: SessionState,
    pub effect_outcome: EffectOutcome,
    pub resource_release: ResourceRelease,
}

/// 上下文变更回执。`replacedSessionId` 非空表示旧会话被替换：
/// 物理会话连续性由固定租约保证（INV-02），但**旧 owner 必须收到 SessionLost**。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextChangeReceipt {
    pub session: SessionView,
    pub replaced_session_id: Option<DbSessionId>,
    /// 仅在**原 owner** 重新附着时返回；存储侧只留哈希（§4.2 `attachmentTokenHash`）。
    pub attachment_token: Option<AttachmentToken>,
}

/// 打开会话回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionReceipt {
    pub session: SessionView,
    pub attachment_token: AttachmentToken,
}

/// 取消请求的结果。`disposition='unsupported'` 是**正常取值**，
/// 不以异常 `CancellationUnsupported` 表达（§13.1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelReceipt {
    pub execution_id: ExecutionId,
    pub disposition: CancelDisposition,
    pub state: ExecutionState,
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

/// 结果块（驱动侧 JSON 行块）。
///
/// 与 [`ArtifactChunk`](crate::execution::domain::ArtifactChunk) 的关系：那个是
/// **字节块**（产物仓储的字节寻址单位，服务端/浏览器形态还要做 base64），
/// 这个是**语义块**——驱动已经把结果解释成列与行，跨边界后仍是 JSON。
/// 两者不是同一个概念，也不该合成一个类型：一个按字节分页，
/// 一个按 `published_chunk_count` 分页。
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultChunk {
    pub artifact_id: ArtifactId,
    pub chunk_index: Counter,
    pub byte_offset: Counter,
    pub columns: serde_json::Value,
    pub rows: Vec<serde_json::Value>,
    pub output: Option<serde_json::Value>,
    pub source: StatementResultSource,
    pub published_chunk_count: Counter,
    pub published_byte_length: Counter,
    pub complete: bool,
}

/// 备份输出的**接收回调**：runtime 在每次推进备份时调用它，回调把字节写进
/// 调用方自己的暂存文件（原 platform-api `dto/artifact.rs`，D3 随视图家族迁入）。
pub type BackupOutput =
    std::sync::Arc<dyn Fn(serde_json::Value) -> Result<(), crate::shared::error::ApiError> + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::domain::NamespaceTarget;
    use crate::execution::domain::{
        CancelOutcome, CapabilitySnapshot, ContextConfidence, TransactionState,
    };
    use crate::shared::context::OwnerRef;
    use crate::shared::id::{
        ClientInstanceId, ConfigRevision, DbSessionId, EditorSessionId, OrganizationId,
        PrincipalId, ResourceId, RuntimeEpoch,
    };
    use serde_json::json;

    fn handle() -> SessionHandle {
        SessionHandle::new(DbSessionId::new("sess-1"), RuntimeEpoch::new("epoch-1"))
    }

    fn context() -> SessionContext {
        SessionContext {
            namespace: NamespaceTarget::database("app"),
            search_path: Some(vec!["public".into(), "tenant".into()]),
            effective_identity: Some("app_user".into()),
            transaction_state: TransactionState::Active,
            autocommit: Some(false),
            confidence: ContextConfidence::Confirmed,
        }
    }

    fn session(state: SessionState, active: Option<&str>) -> SessionView {
        SessionView {
            handle: handle(),
            connection_id: ConnectionId::new("conn-1"),
            config_revision: Counter::new(3),
            owner: OwnerRef::Editor {
                client_instance_id: ClientInstanceId::new("c-1"),
                editor_session_id: EditorSessionId::new("e-1"),
            },
            initial_target: ExecutionTarget::new(
                ConnectionId::new("conn-1"),
                NamespaceTarget::database("app"),
            ),
            observed_context: context(),
            context_revision: Counter::new(1),
            state,
            attachment_state: AttachmentState::Attached,
            active_execution_id: active.map(ExecutionId::new),
            expires_at: Some(Timestamp::new("2026-01-01T00:00:00Z")),
        }
    }

    fn provenance() -> ResultProvenance {
        ResultProvenance {
            organization_id: OrganizationId::new("org-1"),
            principal_id: PrincipalId::new("user-1"),
            connection_id: ConnectionId::new("conn-1"),
            config_revision: ConfigRevision::new(2),
            context_before: context(),
            context_after: context(),
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
                handle: handle(),
                resource_binding_id: ResourceId::new("res-1"),
                execution_id: ExecutionId::new("exec-1"),
            }),
        }
    }

    #[test]
    fn session_view_round_trips_with_decimal_string_counters() {
        let original = session(SessionState::Ready, None);
        let value = serde_json::to_value(&original).expect("serialize");
        assert_eq!(value["configRevision"], json!("3"));
        assert_eq!(value["contextRevision"], json!("1"));
        assert_eq!(value["activeExecutionId"], json!(null));
        assert_eq!(value["handle"]["dbSessionId"], json!("sess-1"));
        assert_eq!(value["handle"]["runtimeEpoch"], json!("epoch-1"));
        assert_eq!(
            value["observedContext"]["transactionState"],
            json!("active")
        );
        assert_eq!(value["observedContext"]["confidence"], json!("confirmed"));
        let back: SessionView = serde_json::from_value(value).expect("deserialize");
        assert_eq!(back, original);
    }

    #[test]
    fn session_defaults_need_ready_state_and_confirmed_context() {
        assert!(session(SessionState::Ready, None).session_defaults_usable());

        let busy = session(SessionState::Ready, Some("exec-1"));
        assert!(!busy.session_defaults_usable(), "执行中不能并发常规执行");

        let unconfirmed = SessionView {
            observed_context: SessionContext {
                confidence: ContextConfidence::Partial,
                ..context()
            },
            ..session(SessionState::Ready, None)
        };
        assert!(
            !unconfirmed.session_defaults_usable(),
            "未确认上下文不能补默认值"
        );

        let closing = session(SessionState::Closing, None);
        assert!(!closing.session_defaults_usable());
    }

    #[test]
    fn close_receipt_can_report_lost_with_unknown_effect() {
        let receipt = CloseReceipt {
            db_session_id: DbSessionId::new("sess-1"),
            state: SessionState::Lost,
            effect_outcome: EffectOutcome::Unknown,
            resource_release: ResourceRelease::Quarantined,
        };
        let value = serde_json::to_value(&receipt).expect("serialize");
        assert_eq!(value["state"], json!("lost"));
        assert_eq!(value["effectOutcome"], json!("unknown"));
        assert_eq!(value["resourceRelease"], json!("quarantined"));
        assert_eq!(
            serde_json::from_value::<CloseReceipt>(value).expect("deserialize"),
            receipt
        );
    }

    #[test]
    fn cancel_receipt_reports_unsupported_as_a_normal_value() {
        let receipt = CancelReceipt {
            execution_id: ExecutionId::new("exec-1"),
            disposition: CancelDisposition::Unsupported,
            state: ExecutionState::Running,
        };
        let value = serde_json::to_value(&receipt).expect("serialize");
        assert_eq!(value["disposition"], json!("unsupported"));
        assert_eq!(value["state"], json!("running"));
        assert_eq!(
            serde_json::from_value::<CancelReceipt>(value).expect("deserialize"),
            receipt
        );
    }

    #[test]
    fn context_change_receipt_round_trips_with_optional_attachment_token() {
        let receipt = ContextChangeReceipt {
            session: session(SessionState::Reconfiguring, None),
            replaced_session_id: Some(DbSessionId::new("sess-0")),
            attachment_token: Some(AttachmentToken::new("tok-1")),
        };
        let value = serde_json::to_value(&receipt).expect("serialize");
        assert_eq!(value["replacedSessionId"], json!("sess-0"));
        assert_eq!(value["attachmentToken"], json!("tok-1"));
        assert_eq!(
            serde_json::from_value::<ContextChangeReceipt>(value).expect("deserialize"),
            receipt
        );
    }

    #[test]
    fn open_session_receipt_round_trips() {
        let receipt = OpenSessionReceipt {
            session: session(SessionState::Ready, None),
            attachment_token: AttachmentToken::new("tok-1"),
        };
        let value = serde_json::to_value(&receipt).expect("serialize");
        assert_eq!(value["attachmentToken"], json!("tok-1"));
        assert_eq!(
            serde_json::from_value::<OpenSessionReceipt>(value).expect("deserialize"),
            receipt
        );
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
    fn sql_error_and_effect_outcome_stay_orthogonal() {
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
    fn an_unknown_cancel_outcome_surfaces_as_non_terminal_state() {
        let resolved = ExecutionState::CancelRequested.resolve_cancel(CancelOutcome::OutcomeUnknown);
        let view = ExecutionView {
            state: resolved,
            effect_outcome: EffectOutcome::Unknown,
            error_code: None,
            ..execution_view()
        };
        let value = serde_json::to_value(&view).expect("serialize");
        assert_eq!(value["state"], json!("cancelRequested"));
        assert_eq!(value["effectOutcome"], json!("unknown"));
    }

    #[test]
    fn attachment_token_debug_is_redacted() {
        let token = AttachmentToken::new("tok-secret");
        assert_eq!(format!("{token:?}"), "AttachmentToken(<redacted>)");
    }
}
