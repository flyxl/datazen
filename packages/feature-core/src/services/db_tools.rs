//! 数据库工具共享实现迁移垫片（D6 item 4 收尾，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::ai::application::db_tools`
//! （SQL 分级沿用 mcp::domain::permission 判据，只读/Safe Mode 校验沿用
//! execution::domain::sql_guard）。本模块只再导出，旧路径
//! `datazen_feature_core::services::db_tools::*` 不变（ai_chat 工具执行
//! 与 MCP Server tools 的消费者零改动）；feature-core 退役时（D8）随垫片删除。

pub use datazen_modules::ai::application::db_tools::*;
