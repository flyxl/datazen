//! Tauri IPC command surface.

mod adb;
pub mod ai;
mod backup;
mod config;
mod context;
mod connection;
mod data;
mod error;
mod file;
mod kv;
pub mod mcp;
mod query;
mod schema;
mod sync;
mod window;

pub use adb::*;
pub use ai::*;
pub use backup::*;
pub use config::*;
pub use context::*;
pub use connection::*;
pub use data::*;
pub use file::*;
pub use kv::*;
pub use mcp::*;
pub use query::*;
pub use schema::*;
pub use sync::*;
pub use window::*;

use crate::ai::{AiProviderRegistry, PromptResolver, SchemaContextBuilder};
use crate::cache::SchemaCache;
use crate::db::registry::DriverRegistry;
use crate::mcp::{McpClientManager, WorkflowHistoryManager, WorkflowRegistry};
use crate::services::ConnectionManager;
use crate::store::Store;
use crate::sync::adapter_registry::SyncAdapterRegistry;
use std::sync::Arc;

/// Shared application state injected into every command handler.
pub struct AppState {
    #[allow(dead_code)]
    pub driver_registry: Arc<DriverRegistry>,
    pub connection_manager: Arc<ConnectionManager>,
    pub store: Arc<Store>,
    pub schema_cache: Arc<SchemaCache>,
    pub sync_adapters: Arc<SyncAdapterRegistry>,
    pub ai_registry: Arc<AiProviderRegistry>,
    pub schema_context_builder: Arc<SchemaContextBuilder>,
    pub prompt_resolver: Arc<PromptResolver>,
    pub workflow_registry: Arc<WorkflowRegistry>,
    pub workflow_history: Arc<WorkflowHistoryManager>,
    pub mcp_client_manager: Arc<McpClientManager>,
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
