//! Backwards-compatible workflow facade.
//!
//! The implementation is split into focused modules. This file contains no
//! workflow runtime logic; it preserves the historical
//! `workflow::workflows::*` import path while the executor is migrated to the
//! Driver Command runtime.

pub(crate) use crate::workflow::command;

#[path = "context.rs"]
pub(crate) mod context;
#[path = "executor.rs"]
pub(crate) mod executor;

pub use super::registry::WorkflowRegistry;
pub use crate::workflow::model::{
    ErrorHandlingConfig, ErrorStrategy, ErrorStrategyKind, StepExecutionResult, StepStatus,
    UnattendedDestructivePolicy, WorkflowDefinition, WorkflowExecutionResult, WorkflowListItem,
    WorkflowMigrationOperation, WorkflowOutput, WorkflowSchedule, WorkflowStep, WorkflowVariable,
    WorkflowVisibility,
};
pub use command::WorkflowCommandStep;
pub use executor::{
    enforce_workflow_query_guards, WorkflowExecuteOptions, WorkflowExecutor,
    WORKFLOW_QUERY_ROW_LIMIT,
};
