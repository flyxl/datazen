//! Workflow 引擎迁移垫片（D6 item 3 切片 b，2026-10-26）。
//!
//! 全家族唯一定义自本切片起在 `datazen-modules::workflow`（domain/application/
//! engine 三分）。各子模块只再导出，旧路径 `datazen_feature_core::workflow::*`
//! 不变（src-tauri 消费者零改动）；家族内部的私有 results 模块随引擎走，
//! 不再在此声明。feature-core 退役时（D8）随垫片一并删除。

pub mod command;
pub mod command_runtime;
pub mod conditions;
pub mod context;
pub mod data_step;
pub mod error;
pub mod executable;
pub mod executor;
pub mod history;
pub mod host;
pub mod model;
pub use error::WorkflowError;
pub mod registry;
pub mod resource_execution;
pub mod scheduler;
pub use command::WorkflowCommandStep;
pub mod plan_guard;
pub mod target_plan;
