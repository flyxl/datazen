//! MCP 领域层：权限模式词汇、SQL 分级与连接白名单规则。
//!
//! 规则全部是纯函数（判据只依赖入参与 `execution::domain::sql_guard`
//! 的机制内核）——宿主（桌面 MCP server）与未来其他形态的 MCP 宿主
//! 用同一份判据，不会出现「桌面拦了、别的形态放行」的分叉。

pub mod allowlist;
pub mod permission;

pub use allowlist::{ensure_connection_allowed, is_connection_allowed};
pub use permission::{
    check_sql_allowed, check_tool_call, is_resource_listed, is_tool_listed,
    query_history_content_allowed, McpPermissionMode,
};
