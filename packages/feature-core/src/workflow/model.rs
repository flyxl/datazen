//! Workflow 模型词汇迁移垫片（D6 第一切片 a，2026-10-26）。
//!
//! 唯一定义自本切片起在 `datazen-modules::workflow::domain::model`。
//! 本模块只再导出，旧路径 `datazen_feature_core::workflow::model::*` 不变
//! （feature-core 内部 executor/registry/dashboard 与 src-tauri 消费者零改动）；
//! workflow 引擎归位 modules 后（D6 后续切片）随 feature-core 退役一并删除。

pub use datazen_modules::workflow::domain::model::*;
