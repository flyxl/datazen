//! Workflow 错误词汇迁移垫片（D6 第一切片 a，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::workflow::domain::error`。
//! 本模块只再导出，旧的 feature-core workflow error 路径保持不变；
//! workflow 引擎归位后随 feature-core 退役一并删除。

pub use datazen_modules::workflow::domain::error::*;
