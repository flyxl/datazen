//! 会话域词汇：句柄、状态机与上下文（原 platform-api `dto/session.rs` 的域侧）。
//!
//! `SessionHandle` 是**纯运行时**值：`dbSessionId` 与 `runtimeEpoch` **永不落盘**，
//! 也不得进入持久化日志、审计或 Job checkpoint（§4.4）。它能出现在实时响应和事件里，
//! 因为事件走的是内存/流通道而不是持久化归档。
//!
//! 视图与回执（`SessionView`、三份回执）在
//! [`crate::execution::application`](super::super::application)。

use serde::{Deserialize, Serialize};

use crate::connection::domain::NamespaceTarget;
use crate::shared::id::{DbSessionId, RuntimeEpoch};

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

/// 物理资源的释放结论。`Quarantined` 表示资源尚未确认释放，仍持续占用计数，
/// 逻辑额度已回收但物理占用不假装已归还。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ResourceRelease {
    Pending,
    Confirmed,
    Quarantined,
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
    use serde_json::json;

    #[test]
    fn session_handle_round_trips_with_camel_case_keys() {
        let handle = SessionHandle::new(
            DbSessionId::new("sess-1"),
            RuntimeEpoch::new("epoch-1"),
        );
        let value = serde_json::to_value(&handle).expect("serialize");
        assert_eq!(value["dbSessionId"], json!("sess-1"));
        assert_eq!(value["runtimeEpoch"], json!("epoch-1"));
        assert_eq!(
            serde_json::from_value::<SessionHandle>(value).expect("deserialize"),
            handle
        );
    }

    #[test]
    fn session_state_uses_the_spec_literals() {
        let states = [
            (SessionState::New, "new"),
            (SessionState::Opening, "opening"),
            (SessionState::Ready, "ready"),
            (SessionState::Executing, "executing"),
            (SessionState::Reconfiguring, "reconfiguring"),
            (SessionState::Closing, "closing"),
            (SessionState::Closed, "closed"),
            (SessionState::Lost, "lost"),
        ];
        for (state, literal) in states {
            assert_eq!(serde_json::to_value(state).expect("s"), json!(literal));
        }
    }

    #[test]
    fn transaction_state_distinguishes_unknown_from_unsupported() {
        assert_eq!(
            serde_json::to_value(TransactionState::Unknown).expect("s"),
            json!("unknown")
        );
        assert_eq!(
            serde_json::to_value(TransactionState::Unsupported).expect("s"),
            json!("unsupported")
        );
    }

    #[test]
    fn search_path_is_a_list_and_stays_independent_of_object_identity() {
        let context = SessionContext {
            namespace: NamespaceTarget::database("app"),
            search_path: Some(vec!["public".into(), "tenant".into()]),
            effective_identity: Some("app_user".into()),
            transaction_state: TransactionState::Active,
            autocommit: Some(false),
            confidence: ContextConfidence::Confirmed,
        };
        let value = serde_json::to_value(&context).expect("serialize");
        assert_eq!(value["searchPath"], json!(["public", "tenant"]));
        assert_eq!(value["namespace"]["schema"], json!(null));
    }

    #[test]
    fn attachment_states_cover_the_renewal_rule() {
        assert_eq!(
            serde_json::to_value(AttachmentState::Expired).expect("s"),
            json!("expired")
        );
    }
}
