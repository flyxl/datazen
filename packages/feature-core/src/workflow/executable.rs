//! Workflow 定义分派迁移垫片（D6 第一切片 a，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::workflow::domain::executable`
//! （纯函数：版本分派 + 静态校验 + 可执行形态；`impl WorkflowDefinition`
//! 随类型迁入定义 crate）。本模块只再导出，旧路径
//! `datazen_feature_core::workflow::executable::*` 不变。

pub use datazen_modules::workflow::domain::executable::*;
