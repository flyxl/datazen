//! User-defined AI workflow system (YAML automations: SQL + AI steps).
//!
//! Exposed via Tauri IPC and optionally via MCP tools/resources; the engine
//! itself does not depend on the MCP protocol.

pub mod command;
pub mod command_runtime;
pub mod history;
pub mod workflows;

// These modules own the canonical workflow data types and registry. They are
// public so the compatibility facade in `workflows` can re-export the same
// types without introducing duplicate definitions.
pub mod model;
pub mod registry;
pub mod scheduler;

pub use crate::store::{HistoryEntry, HistoryListItem};
pub use command::WorkflowCommandStep;
pub use command_runtime::{execute_command, resolve_connection_id};
pub use history::WorkflowHistoryManager;
pub use workflows::{
    enforce_workflow_query_guards, StepExecutionResult, StepStatus, WorkflowDefinition,
    WorkflowExecuteOptions, WorkflowExecutionResult, WorkflowExecutor, WorkflowListItem,
    WorkflowRegistry, WorkflowStep, WorkflowVisibility, WORKFLOW_QUERY_ROW_LIMIT,
};
