//! Dashboard 应用层：组件创建、运行历史、告警评估、归档与执行编排。
//! （Monitor 引擎与通知规则归 `crate::monitor`，不经本层转发。）

pub mod alert;
pub mod archive;
pub mod create;
pub mod execute;
pub mod runs;
