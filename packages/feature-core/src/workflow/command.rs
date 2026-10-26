//! Workflow 命令步骤词汇（WorkflowCommandStep，归一化的 Driver Command 模型）迁移垫片（D6 item 3 切片 b，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::workflow::domain::command`。
//! 本模块只再导出，旧路径不变；feature-core 退役时（D8）随垫片删除。

pub use datazen_modules::workflow::domain::command::*;
