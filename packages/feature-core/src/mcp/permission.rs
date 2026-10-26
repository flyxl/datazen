//! MCP 权限模式迁移垫片（D6 item 4 切片一，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::mcp::domain::permission`
//! （McpPermissionMode + SQL 分级 + 工具/资源可见性规则）。本模块只再导出，
//! 旧路径 `datazen_feature_core::mcp::permission::*` 不变；feature-core 退役
//! 时（D8）随垫片一并删除。

pub use datazen_modules::mcp::domain::permission::*;
