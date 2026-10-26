//! 会话契约迁移垫片（D3，2026-10-26）。
//!
//! `SessionHandle`、状态机与上下文的唯一定义在
//! `datazen-modules::execution::domain::session`；视图与三份回执在
//! `datazen-modules::execution::application::views`。本模块只再导出，
//! 旧 import 路径 `dto::session::*` 保持不变；D8 退役本 crate 时一并删除。

pub use datazen_modules::execution::application::{
    CancelReceipt, CloseReceipt, ContextChangeReceipt, OpenSessionReceipt, SessionView,
};
pub use datazen_modules::execution::domain::{
    AttachmentState, CancelDisposition, ContextConfidence, ResourceRelease, SessionContext,
    SessionHandle, SessionState, TransactionState,
};
