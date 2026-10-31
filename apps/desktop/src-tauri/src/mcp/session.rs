//! MCP 会话映射的宿主垫片（M2 归位后）。
//!
//! 语义（registry、TTL、归属校验、过期/清理登记）唯一定义在
//! `datazen::mcp::session`（modules）；此处保持旧路径再导出，宿主的
//! open/use/close 工具、cleanup worker 与传输生命周期驱动的 import
//! 零改动。

pub use datazen::mcp::session::*;
