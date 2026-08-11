//! User-defined AI workflow system (YAML automations: SQL + AI steps).
//!
//! Exposed via Tauri IPC and optionally via MCP tools/resources; the engine
//! itself does not depend on the MCP protocol.

pub mod command;
pub mod history;
pub mod workflows;

pub use command::WorkflowCommandStep;
pub use history::{HistoryEntry, HistoryListItem, WorkflowHistoryManager};
pub use workflows::{
    enforce_workflow_query_guards, StepExecutionResult, StepStatus, WorkflowDefinition,
    WorkflowExecuteOptions, WorkflowExecutionResult, WorkflowExecutor, WorkflowListItem,
    WorkflowRegistry, WorkflowStep, WORKFLOW_QUERY_ROW_LIMIT,
};
