//! 会话契约：`SessionHandle`、`SessionView` 与三份回执。
//!
//! `SessionHandle` 是**纯运行时**值：`dbSessionId` 与 `runtimeEpoch` **永不落盘**，
//! 也不得进入持久化日志、审计或 Job checkpoint（§4.4）。它能出现在实时响应和事件里，
//! 因为事件走的是内存/流通道而不是持久化归档。

use serde::{Deserialize, Serialize};

use crate::id::{ConnectionId, Counter, DbSessionId, ExecutionId, RuntimeEpoch, Timestamp};
use crate::target::{ExecutionTarget, NamespaceTarget};
use crate::OwnerRef;

/// 运行时会话句柄。**不是持久化身份**：资源丢失或替换后 `runtimeEpoch` 立即变化，
/// 旧句柄必须让调用方拿到 `SessionLost` 而不是静默重建（§4.4 末段）。
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionHandle {
    pub db_session_id: DbSessionId,
    pub runtime_epoch: RuntimeEpoch,
}

impl SessionHandle {
    pub fn new(db_session_id: DbSessionId, runtime_epoch: RuntimeEpoch) -> Self {
        Self {
            db_session_id,
            runtime_epoch,
        }
    }
}

/// 事务状态。`Unsupported` 表示驱动或后端无法观察事务，与 `Unknown`（能观察但读不到）不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TransactionState {
    None,
    Active,
    Aborted,
    Unknown,
    Unsupported,
}

/// 上下文可信度。驱动无法观察批次内准确上下文时返回 `Partial`/`Unknown` 且 readOnly，
/// **不猜测** relation，也不提供可写映射（§4.1 末段）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContextConfidence {
    Confirmed,
    Partial,
    Unknown,
}

/// 后端观察到的会话上下文。`confidence=Confirmed` 是「可用该 session 已确认默认值」的**唯一**依据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionContext {
    pub namespace: NamespaceTarget,
    /// 观察到的 search path，与对象身份相互独立（§4.3 PostgreSQL 行）。
    pub search_path: Option<Vec<String>>,
    /// 后端实际使用的执行身份显示值；不含凭据。
    pub effective_identity: Option<String>,
    pub transaction_state: TransactionState,
    pub autocommit: Option<bool>,
    pub confidence: ContextConfidence,
}

/// 附着状态。`Expired` 之后必须重新走 attachment 流程，不得沿用旧 attachmentToken。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AttachmentState {
    Attached,
    Detached,
    Expired,
}

/// 会话生命周期状态机。
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
            && self.observed_context.confidence == ContextConfidence::Confirmed
            && self.active_execution_id.is_none()
    }
}

/// 物理资源的释放结论。`Quarantined` 表示资源尚未确认释放，仍持续占用计数，
/// 逻辑额度已回收但物理占用不假装已归还。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourceRelease {
    Pending,
    Confirmed,
    Quarantined,
}

/// 关闭会话回执。`state='lost'` 表示资源在关闭过程中丢失，
/// 此时 `effectOutcome` 可能是 `Unknown`——不得写成 `rolledBack`（§13.1 正交性规则）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloseReceipt {
    pub db_session_id: DbSessionId,
    pub state: SessionState,
    pub effect_outcome: crate::dto::execution::EffectOutcome,
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
    pub attachment_token: Option<crate::id::AttachmentToken>,
}

/// 打开会话回执。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSessionReceipt {
    pub session: SessionView,
    pub attachment_token: crate::id::AttachmentToken,
}

/// 取消请求的结果。`disposition='unsupported'` 是**正常取值**，
/// 不以异常 `CancellationUnsupported` 表达（§13.1）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CancelReceipt {
    pub execution_id: ExecutionId,
    pub disposition: CancelDisposition,
    pub state: crate::dto::execution::ExecutionState,
}

/// 取消控制请求的结果分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CancelDisposition {
    /// 请求已送达驱动。**不构成「写入已回滚」的证据**。
    Requested,
    /// 驱动不支持精确取消（能力快照里 `PreciseCancel=false`）。
    Unsupported,
    /// 执行已终态，取消未改变任何东西。
    AlreadyFinished,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::OwnerRef;
    use crate::id::ClientInstanceId;
    use serde_json::json;

    fn handle() -> SessionHandle {
        SessionHandle {
            db_session_id: DbSessionId::new("sess-1"),
            runtime_epoch: RuntimeEpoch::new("epoch-1"),
        }
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
                editor_session_id: crate::id::EditorSessionId::new("e-1"),
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
    fn search_path_is_a_list_and_stays_independent_of_object_identity() {
        let value = serde_json::to_value(context()).expect("serialize");
        assert_eq!(value["searchPath"], json!(["public", "tenant"]));
        assert_eq!(value["namespace"]["schema"], json!(null));
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
            effect_outcome: crate::dto::execution::EffectOutcome::Unknown,
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
            state: crate::dto::execution::ExecutionState::Running,
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
            attachment_token: Some(crate::id::AttachmentToken::new("tok-1")),
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
            attachment_token: crate::id::AttachmentToken::new("tok-1"),
        };
        let value = serde_json::to_value(&receipt).expect("serialize");
        assert_eq!(value["attachmentToken"], json!("tok-1"));
        assert_eq!(
            serde_json::from_value::<OpenSessionReceipt>(value).expect("deserialize"),
            receipt
        );
    }

    #[test]
    fn attachment_token_debug_is_redacted() {
        let token = crate::id::AttachmentToken::new("tok-secret");
        assert_eq!(format!("{token:?}"), "AttachmentToken(<redacted>)");
    }
}
