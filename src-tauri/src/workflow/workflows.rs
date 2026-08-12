//! Backwards-compatible workflow facade.
//!
//! The implementation is split into focused modules. This file contains no
//! workflow runtime logic; it preserves the historical
//! `workflow::workflows::*` import path while the executor is migrated to the
//! Driver Command runtime.

pub(crate) use crate::workflow::command;
pub(crate) use crate::workflow::command_runtime;
#[allow(unused_imports)]
pub(crate) use crate::workflow::model;

#[path = "conditions.rs"]
pub(crate) mod conditions;
#[path = "context.rs"]
pub(crate) mod context;
#[path = "executor.rs"]
pub(crate) mod executor;

pub use super::registry::WorkflowRegistry;
pub use crate::workflow::model::{
    ErrorHandlingConfig, ErrorStrategy, ErrorStrategyKind, StepExecutionResult, StepStatus,
    WorkflowDefinition, WorkflowExecutionResult, WorkflowListItem, WorkflowOutput,
    WorkflowSchedule, WorkflowStep, WorkflowVariable, WorkflowVisibility,
};
pub use command::WorkflowCommandStep;
pub use executor::{
    enforce_workflow_query_guards, WorkflowExecuteOptions, WorkflowExecutor,
    WORKFLOW_QUERY_ROW_LIMIT,
};
