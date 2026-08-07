//! User-defined AI workflow system (YAML automations: SQL + AI steps).
//!
//! Exposed via Tauri IPC and optionally via MCP tools/resources; the engine
//! itself does not depend on the MCP protocol.

pub mod history;
pub mod workflows;

pub use history::{HistoryEntry, HistoryListItem, WorkflowHistoryManager};
pub use workflows::{
    StepExecutionResult, StepStatus, WorkflowDefinition, WorkflowExecutionResult, WorkflowExecutor,
    WorkflowListItem, WorkflowRegistry, WorkflowStep,
};
