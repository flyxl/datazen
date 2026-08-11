//! Backwards-compatible workflow facade.
//!
//! The implementation is split into focused modules. This file contains no
//! workflow runtime logic; it preserves the historical
//! `workflow::workflows::*` import path while the executor is migrated to the
//! Driver Command runtime.

// `mod.rs` predates the split and cannot be changed independently in older
// plugin layouts, so the facade provides the implementation modules locally.
// The model module is only a type namespace forwarding to the canonical model.
pub(crate) mod model {
    pub(crate) use crate::workflow::model::*;
}

pub(crate) use crate::workflow::command;
pub(crate) use crate::workflow::command_runtime;

#[path = "context.rs"]
pub(crate) mod context;
#[path = "conditions.rs"]
pub(crate) mod conditions;
#[path = "executor.rs"]
pub(crate) mod executor;

pub use command::WorkflowCommandStep;
pub use executor::{
    enforce_workflow_query_guards, WorkflowExecuteOptions, WorkflowExecutor,
    WORKFLOW_QUERY_ROW_LIMIT,
};
pub use model::{
    ErrorHandlingConfig, ErrorStrategy, ErrorStrategyKind, StepExecutionResult, StepStatus,
    WorkflowDefinition, WorkflowExecutionResult, WorkflowListItem, WorkflowOutput, WorkflowStep,
    WorkflowVariable,
};
pub use super::registry::WorkflowRegistry;
