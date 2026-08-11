//! Backwards-compatible workflow facade.
//!
//! The implementation is split into focused modules. This file intentionally
//! contains no workflow runtime logic; it keeps the historical `workflow::workflows::*`
//! import path stable for existing callers while the engine moves to the new
//! Command-based execution model.

pub use super::command::WorkflowCommandStep;
pub use super::executor::{
    enforce_workflow_query_guards, WorkflowExecuteOptions, WorkflowExecutor,
    WORKFLOW_QUERY_ROW_LIMIT,
};
pub use super::model::{
    ErrorHandlingConfig, ErrorStrategy, ErrorStrategyKind, StepExecutionResult, StepStatus,
    WorkflowDefinition, WorkflowExecutionResult, WorkflowListItem, WorkflowOutput, WorkflowStep,
    WorkflowVariable,
};
pub use super::registry::WorkflowRegistry;
