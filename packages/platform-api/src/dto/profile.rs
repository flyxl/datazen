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

pub use datazen_modules::execution::application::{ConnectionEvent, ResultChunkEvent};
use crate::id::{DbSessionId, ExecutionId, StageId, StreamId, Timestamp};
use crate::target::ExecutionTarget;

// Profile 词汇：唯一定义在 datazen-modules::connection（D2）。
pub use datazen_modules::connection::application::ProfileView;
pub use datazen_modules::connection::domain::{
    ProfileDraft, ProfileDraftPatch, ProfilePatch,
};

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
    fn re_exported_connection_event_is_the_execution_type() {
        const _: fn(ConnectionEvent) -> datazen_modules::execution::ConnectionEvent = |e| e;
        let event = ConnectionEvent::StreamResetRequired {
            reason: "runtime restarted".into(),
        };
        let value = serde_json::to_value(&event).expect("serialize");
        assert_eq!(value["kind"], json!("streamResetRequired"));
        assert!(event.requires_resubscribe());
    }
}
