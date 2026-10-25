//! 错误词汇迁移垫片（D2，2026-10-26）。
//!
//! `PortError` / `ApiError` / `ApiErrorCode` / `RetryDisposition` 的唯一定义
//! 自 D2 起在 `datazen-modules::shared::error`（DDD 迁移 §5.1：公共错误信封
//! 可保留在 shared，但不包含具体业务实体）。本模块只再导出，旧 import
//! 路径不变；D8 退役本 crate 时一并删除。
//!
//! 词汇语义不变：端口层只报基础设施与并发事实，业务拒绝由用例层判定成
//! `ApiError`；`PortError` 依旧不实现 serde。

pub use datazen_modules::shared::error::{ApiError, ApiErrorCode, PortError, RetryDisposition};
