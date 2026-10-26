//! Workflow 引擎层：执行引擎与定时调度。
//!
//! executor 只依赖 application 的端口与规则（引擎 → 应用，单向）；scheduler
//! 经 `WorkflowScheduleHost` 端口回调宿主，tick 循环/在途守卫/取消令牌自持。

pub mod executor;
pub mod scheduler;

pub use executor::{WorkflowExecuteOptions, WorkflowExecutor, WORKFLOW_QUERY_ROW_LIMIT};
pub use scheduler::{
    clamp_interval_secs, due_decision, is_workflow_due, scheduled_interval_secs, DueDecision,
    WorkflowScheduleHost, WorkflowScheduler, MIN_INTERVAL_SECS,
};
