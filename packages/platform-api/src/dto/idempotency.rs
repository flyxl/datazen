//! 幂等契约迁移垫片（D3，2026-10-26）。
//!
//! `IdempotentOperation` / `SubmissionToken` 的唯一定义在
//! `datazen-modules::execution::application::idempotency`。本模块只再导出，
//! 旧 import 路径不变；D8 退役本 crate 时一并删除。
//!
//! §13.1：幂等键不是客户端随意 UUID；验证与去重规则属于
//! `SubmissionTokenIssuer` 端口实现（仍在 ports/，随机制迁移）。

pub use datazen_modules::execution::application::{IdempotentOperation, SubmissionToken};
