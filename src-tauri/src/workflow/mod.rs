//! User-defined AI workflow system (YAML automations: SQL + AI steps).
//!
//! Exposed via Tauri IPC and optionally via MCP tools/resources; the engine
//! itself does not depend on the MCP protocol.

pub mod command;
pub mod command_runtime;
pub mod history;
pub mod workflows;

// Extracted workflow modules are introduced before the final workflows.rs
// reduction so the refactor can be reviewed as a sequence of safe commits.
pub(crate) mod model;
pub(crate) mod registry;

pub use command::WorkflowCommandStep;
pub use command_runtime::{execute_command, resolve_connection_id};
pub use history::{HistoryEntry, HistoryListItem, WorkflowHistoryManager};
pub use workflows::{
    enforce_workflow_query_guards, StepExecutionResult, StepStatus, WorkflowDefinition,
    WorkflowExecuteOptions, WorkflowExecutionResult, WorkflowExecutor, WorkflowListItem,
    WorkflowRegistry, WorkflowStep, WORKFLOW_QUERY_ROW_LIMIT,
};
