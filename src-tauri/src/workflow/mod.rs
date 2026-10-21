//! User-defined AI workflow system (YAML automations: SQL + AI steps).
//!
//! Exposed via Tauri IPC and optionally via MCP tools/resources; the engine
//! itself does not depend on the MCP protocol.
//!
//! The engine, the model and the rules live in
//! `datazen_feature_core::workflow`. What stays here is the desktop half: the
//! store bindings, the runtime session handling and the Tauri IPC.

pub mod command_runtime;
pub mod history;
mod independent_sql;
pub(crate) mod migration;

// These modules own the workflow registry and the desktop scheduling adapter.
pub mod registry;
pub mod scheduler;

pub use datazen_feature_core::workflow::command::WorkflowCommandStep;
pub use datazen_feature_core::workflow::error::WorkflowError;
pub use history::WorkflowHistoryManager;
pub use registry::WorkflowRegistry;

mod host;

#[cfg(test)]
mod live_acceptance;

// 长期文档 `docs/architecture/platform/workflow-resource-model.md` 的引用新鲜度守卫。
#[cfg(test)]
#[path = "doc_reference_tests.rs"]
mod doc_reference_tests;
