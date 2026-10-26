//! 执行请求迁移垫片（D3，2026-10-26）。
//!
//! `CommandCall` / `OpenSessionRequest` / `ExecuteInSessionRequest` /
//! `ExecuteAtTargetRequest` / `SetSessionContextRequest` / `CloseSessionRequest` /
//! `AttachmentRequest` 与 `CloseMode` 的唯一定义在
//! `datazen-modules::execution::application::requests`。本模块只再导出，
//! 旧 import 路径 `dto::requests::*`（含 glob）保持不变；D8 退役本 crate 时
//! 一并删除。

pub use datazen_modules::execution::application::requests::*;
