//! Tauri IPC command surface.

mod adb;
pub mod ai;
mod backup;
mod config;
mod connection;
mod connection_import;
mod context;
mod dashboard;
mod data;
pub(crate) mod driver_command;
mod error;
mod file;
mod history;
mod kv;
pub mod mcp;
mod query;
mod schema;
mod schema_diff;
mod structure;
mod sync;
mod theme;
mod window;

pub use adb::*;
pub use ai::*;
pub use backup::*;
pub use config::*;
pub use connection::*;
pub use context::*;
pub use dashboard::*;
pub use data::*;
pub use driver_command::*;
pub use file::*;
pub use history::*;
pub use kv::*;
pub use mcp::*;
pub use query::*;
pub use schema::*;
pub use schema_diff::*;
pub use structure::*;
pub use sync::*;
pub use theme::*;
pub use window::*;

use crate::ai::{AiProviderRegistry, PromptResolver, SchemaContextBuilder};
use crate::cache::SchemaCache;
use crate::db::registry::DriverRegistry;
use crate::db::TransactionHandle;
use crate::mcp::McpClientManager;
use crate::monitor::{MonitorConnectionRegistry, MonitorEngine};
use crate::services::ConnectionManager;
use crate::store::Store;
use crate::sync::adapter_registry::SyncAdapterRegistry;
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
