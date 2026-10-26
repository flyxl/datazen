//! `SessionDirectory` 迁移垫片（D3，2026-10-26）。
//!
//! 会话目录端口（归属、替换、失效、关闭）与 `SessionOwner`/`ReplacementOperation`
//! 等词汇的唯一定义自 D3 起在
//! `datazen-modules::execution::domain::session_directory`。本模块只再导出，
//! 旧 import 路径不变；D8 退役本 crate 时一并删除。

pub use datazen_modules::execution::domain::{
    has_active_execution, owner_matches_client, owner_matches_job, CloseDisposition,
    InvalidationReason, ReplacementCommit, ReplacementOperation, ReplacementOutcome,
    SessionDirectory, SessionOwner,
};
