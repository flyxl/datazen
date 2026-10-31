//! MCP 应用层：面向工具输出的格式化规则。
//!
//! 命令准备策略（command_policy）已随权限词汇归位
//! `execution::application::command_policy`（M1 前置刀）；此处保持旧路径
//! 再导出，宿主与垫片既有 import 零改动。

pub mod describe;

pub use crate::execution::application::command_policy;
pub use describe::format_table_description;
