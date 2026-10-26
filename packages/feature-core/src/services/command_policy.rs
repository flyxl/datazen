//! MCP 命令准备策略迁移垫片（D6 item 4，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::mcp::application::command_policy`。
//! 本模块只再导出，旧路径 `datazen_feature_core::services::command_policy::*`
//! 不变（src-tauri driver_command 适配器零改动）；feature-core 退役时（D8）
//! 随垫片一并删除。

pub use datazen_modules::mcp::application::command_policy::*;
