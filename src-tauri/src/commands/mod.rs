//! Tauri IPC command surface.

pub mod ai;
mod backup;
mod config;
mod connection;
mod connection_import;
mod context;
mod dashboard;
mod data;
mod data_transfer;
mod dialog;
pub(crate) mod driver_command;
mod error;
mod export;
mod extensions;
mod file;
mod history;
pub(crate) mod job_registry;
pub mod mcp;
mod query;
mod schema;
mod schema_diff;
mod structure;
mod sync;
mod theme;
pub(crate) mod window;
mod workflow;

pub use ai::*;
pub use backup::*;
pub use config::*;
pub use connection::*;
pub use context::*;
pub use dashboard::*;
pub use data::*;
pub use data_transfer::*;
// Webdriver builds re-export the injection IPCs for lib.rs registration;
// production builds expose nothing from the dialog gateway beyond its
// internal call sites (super::dialog::* within this module).
#[cfg(feature = "webdriver")]
pub use dialog::*;
pub use driver_command::*;
pub use export::*;
pub use extensions::*;
pub use file::*;
pub use history::*;
pub use mcp::*;
pub use query::*;
pub use schema::*;
pub use schema_diff::*;
pub use structure::*;
pub use sync::*;
pub use theme::*;
pub use window::*;
pub use workflow::*;

use crate::ai::{AiProviderRegistry, PromptResolver, SchemaContextBuilder};
use crate::cache::SchemaCache;
use crate::db::registry::DriverRegistry;
use crate::db::TransactionHandle;
use crate::extensions::ExtensionManager;
use crate::mcp::McpClientManager;
use crate::monitor::{MonitorConnectionRegistry, MonitorEngine};
use crate::services::ConnectionManager;
use crate::store::Store;
use crate::transfer::adapter_registry::SyncAdapterRegistry;
use crate::workflow::scheduler::WorkflowScheduler;
use crate::workflow::{WorkflowHistoryManager, WorkflowRegistry};
use std::collections::HashMap;
use std::sync::Arc;

/// Shared application state injected into every command handler.
#[derive(Clone)]
pub struct AppState {
    pub driver_registry: Arc<DriverRegistry>,
    pub connection_manager: Arc<ConnectionManager>,
    pub monitor_connections: Arc<MonitorConnectionRegistry>,
    pub monitor_engine: Arc<MonitorEngine>,
    pub store: Arc<Store>,
    pub schema_cache: Arc<SchemaCache>,
    pub sync_adapters: Arc<SyncAdapterRegistry>,
    pub ai_registry: Arc<AiProviderRegistry>,
    pub schema_context_builder: Arc<SchemaContextBuilder>,
    pub prompt_resolver: Arc<PromptResolver>,
    pub workflow_registry: Arc<WorkflowRegistry>,
    pub workflow_history: Arc<WorkflowHistoryManager>,
    pub mcp_client_manager: Arc<McpClientManager>,
    pub session_transactions: Arc<tokio::sync::Mutex<HashMap<String, TransactionHandle>>>,
    pub workflow_scheduler: Arc<WorkflowScheduler>,
    pub extensions: Arc<ExtensionManager>,
}

impl AppState {
    /// Lazily register AI providers + load prompt templates.
    /// Does not run during GUI startup — only on first AI / prompt use.
    pub async fn ensure_ai_ready(&self) {
        self.ai_registry.ensure_registered(&self.store).await;
        let lang = self.store.get_settings().await.language;
        self.prompt_resolver.ensure_ready(&lang).await;
    }
}
