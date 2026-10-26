//! 连接模块的回执与事件集合 + Profile DTO 再导出（D2，2026-10-26）。
//!
//! Profile 词汇（`ProfileView` / `ProfileDraft` / `ProfilePatch` /
//! `ProfileDraftPatch`）的唯一定义自 D2 起在 `datazen-modules::connection`：
//! 视图与投影在 application，草稿/补丁在 domain。本文件只再导出，旧
//! import 路径 `dto::profile::*` 保持不变。
//!
//! §4.1 的字段校验随定义迁移：列表**不返回** password、token、TLS 私钥或
//! 任何可直接解密材料；`publicOptions` 只含可见且非敏感字段。凭据内容只存
//! `SecretProvider`（§4.2）。
//!
//! 留在本文件的是会话/执行事件信封（`ConnectionEvent` / `ResultChunkEvent`）
//! 与回执别名：它们是 execution 侧词汇，随 D3 迁移。

use serde::{Deserialize, Serialize};

use crate::id::{ArtifactId, Counter, DbSessionId, ExecutionId, StageId, StreamId, Timestamp};
use crate::target::ExecutionTarget;

// Profile 词汇：唯一定义在 datazen-modules::connection（D2）。
pub use datazen_modules::connection::application::ProfileView;
pub use datazen_modules::connection::domain::{
    ProfileDraft, ProfileDraftPatch, ProfilePatch,
};

/// 结果块到达事件携带的来源标签。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResultChunkEvent {
    pub artifact_id: ArtifactId,
    pub chunk_index: Counter,
    pub source: crate::dto::execution::StatementResultSource,
}

/// 事件载荷。
// `ExecutionView` 比 `SessionView` 大（864 vs 544 字节），装箱可以压掉差值，但
// `ConnectionEvent` 是响应/事件信封而不是热路径栈值：每条事件都会跨进程序列化，
// 装箱只会把这一次分配加到每条事件上，而枚举大小的收益在信封生命周期里兑现不了。
// 这里的形状是刻意设计，改成参数结构体或加间接层都会动到 Wave 1/Wave 2 已依赖的契约。
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum ConnectionEvent {
    /// 会话变化（状态、上下文、attachment）。
    SessionChanged {
        session: crate::dto::session::SessionView,
    },
    /// 执行状态变化。
    ExecutionChanged {
        execution: crate::dto::execution::ExecutionView,
    },
    /// 结果块到达。
    ResultChunk(ResultChunkEvent),
    /// 流已失效，客户端必须重新订阅；**不得**据此推断执行未发生。
    StreamResetRequired { reason: String },
}

impl ConnectionEvent {
    /// 该事件是否需要客户端重新订阅。
    pub fn requires_resubscribe(&self) -> bool {
        matches!(self, Self::StreamResetRequired { .. })
    }
}

/// 创建会话的回执（附件令牌仅发给原 owner）。
pub type ProfileOpenSessionReceipt = crate::dto::session::OpenSessionReceipt;
/// 上下文变更回执。
pub type ProfileContextChangeReceipt = crate::dto::session::ContextChangeReceipt;

/// 数据库会话 ID 的等价物：在回执里它以裸 ID 出现而不是 `SessionHandle`，
/// 因为回执要跨进程/跨 owner 传递，不能携带运行时纪元。
pub type ReceiptDbSessionId = DbSessionId;
/// 回执里的执行 ID。
pub type ReceiptExecutionId = ExecutionId;
/// 回执里的阶段 ID。
pub type ReceiptStageId = StageId;
/// 回执里的流 ID。
pub type ReceiptStreamId = StreamId;
/// 回执里的时刻标记。
pub type ReceiptTimestamp = Timestamp;
/// 初始目标在配置视图里的别名。
pub type ProfileInitialTarget = ExecutionTarget;
/// 取消回执在连接模块接口上的别名。
pub type ProfileCancelReceipt = crate::dto::session::CancelReceipt;
/// 关闭回执在连接模块接口上的别名。
pub type ProfileCloseReceipt = crate::dto::session::CloseReceipt;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::NamespaceTarget;
    use serde_json::json;

    #[test]
    fn re_exported_profile_view_is_the_modules_type() {
        // 编译期保证：再导出与 modules 定义是同一个类型，不是两份形状。
        const _: fn(ProfileView) -> datazen_modules::connection::ProfileView = |view| view;
        let view = ProfileView::new(
            crate::id::ConnectionId::new("conn-1"),
            "prod",
            "postgres",
        );
        let json = serde_json::to_string(&view).expect("serialize");
        assert!(json.contains("conn-1"));
    }

    #[test]
    fn connection_event_uses_the_spec_tag_names() {
        let events = [
            ConnectionEvent::StreamResetRequired {
                reason: "runtime restarted".into(),
            },
            ConnectionEvent::ExecutionChanged {
                execution: crate::dto::execution::ExecutionView {
                    execution_id: ExecutionId::new("exec-1"),
                    state: crate::dto::execution::ExecutionState::Running,
                    effect_outcome: crate::dto::execution::EffectOutcome::NotStarted,
                    provenance: None,
                    artifact_ids: vec![],
                    result_completeness: crate::dto::execution::ResultCompleteness::Pending,
                    truncation_reason: None,
                    error_code: None,
                    runtime_binding: None,
                },
            },
        ];
        let kinds: Vec<String> = events
            .iter()
            .map(|e| {
                serde_json::to_value(e).expect("serialize")["kind"]
                    .as_str()
                    .unwrap_or_default()
                    .to_owned()
            })
            .collect();
        assert_eq!(kinds, ["streamResetRequired", "executionChanged"]);

        for event in &events {
            let value = serde_json::to_value(event).expect("serialize");
            assert_eq!(
                serde_json::from_value::<ConnectionEvent>(value).expect("deserialize"),
                *event
            );
        }

        assert!(events[0].requires_resubscribe());
        assert!(!events[1].requires_resubscribe());
    }

    #[test]
    fn result_chunk_event_round_trips() {
        let event = ConnectionEvent::ResultChunk(ResultChunkEvent {
            artifact_id: ArtifactId::new("art-1"),
            chunk_index: Counter::new(2),
            source: crate::dto::execution::StatementResultSource {
                execution_id: ExecutionId::new("exec-1"),
                statement_index: 0,
                context: crate::dto::session::SessionContext {
                    namespace: NamespaceTarget::database("app"),
                    search_path: None,
                    effective_identity: None,
                    transaction_state: crate::dto::session::TransactionState::None,
                    autocommit: Some(true),
                    confidence: crate::dto::session::ContextConfidence::Confirmed,
                },
                relation: None,
                writable_mapping: crate::dto::execution::WritableMapping::ReadOnly,
            },
        });
        let value = serde_json::to_value(&event).expect("serialize");
        assert_eq!(value["kind"], json!("resultChunk"));
        assert_eq!(value["chunkIndex"], json!("2"));
        assert_eq!(value["source"]["writableMapping"], json!("readOnly"));
        assert_eq!(
            serde_json::from_value::<ConnectionEvent>(value).expect("deserialize"),
            event
        );
    }
}
