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
use datazen_driver_api::QueryExecutionId;
use std::collections::{hash_map::Entry, HashMap};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Host-side ownership registry for live query execution handles.
///
/// The driver owns the backend target registry; this map only binds an opaque
/// execution token to its `dbSessionId` so IPC cannot cancel another session's
/// query. Entries are removed on every stream terminal path.
pub struct QueryExecutionRegistry {
    owners: RwLock<HashMap<QueryExecutionId, String>>,
}

impl QueryExecutionRegistry {
    pub fn new() -> Self {
        Self {
            owners: RwLock::new(HashMap::new()),
        }
    }

    pub async fn register(
        &self,
        execution_id: QueryExecutionId,
        db_session_id: impl Into<String>,
    ) -> Result<(), String> {
        let mut owners = self.owners.write().await;
        match owners.entry(execution_id.clone()) {
            Entry::Vacant(entry) => {
                entry.insert(db_session_id.into());
                Ok(())
            }
            Entry::Occupied(_) => Err(format!(
                "query execution id '{}' is already registered",
                execution_id.as_str()
            )),
        }
    }

    pub async fn validate_owner(
        &self,
        execution_id: &QueryExecutionId,
        db_session_id: &str,
    ) -> Result<(), String> {
        let owners = self.owners.read().await;
        match owners.get(execution_id) {
            Some(owner) if owner == db_session_id => Ok(()),
            Some(_) => Err(format!(
                "query execution '{}' belongs to a different db session",
                execution_id.as_str()
            )),
            None => Err(format!(
                "query execution '{}' is unknown or stale",
                execution_id.as_str()
            )),
        }
    }

    pub async fn remove(&self, execution_id: &QueryExecutionId) {
        self.owners.write().await.remove(execution_id);
    }
}

impl Default for QueryExecutionRegistry {
    fn default() -> Self {
        Self::new()
    }
}

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
    pub query_executions: Arc<QueryExecutionRegistry>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn query_execution_registry_rejects_duplicate_wrong_and_stale_handles() {
        let registry = QueryExecutionRegistry::new();
        let first = QueryExecutionId::new("exec-a");
        let second = QueryExecutionId::new("exec-b");

        registry.register(first.clone(), "session-a").await.unwrap();
        registry
            .register(second.clone(), "session-b")
            .await
            .unwrap();

        let duplicate = registry.register(first.clone(), "session-b").await;
        assert!(duplicate.unwrap_err().contains("already registered"));
        assert!(registry.validate_owner(&first, "session-a").await.is_ok());
        assert!(registry
            .validate_owner(&first, "session-b")
            .await
            .unwrap_err()
            .contains("different db session"));
        assert!(registry
            .validate_owner(&QueryExecutionId::new("unknown"), "session-a")
            .await
            .unwrap_err()
            .contains("unknown or stale"));

        registry.remove(&first).await;
        assert!(registry
            .validate_owner(&first, "session-a")
            .await
            .unwrap_err()
            .contains("unknown or stale"));
    }

    #[tokio::test]
    async fn concurrent_execution_ids_are_isolated_by_session() {
        let registry = QueryExecutionRegistry::new();
        let first = QueryExecutionId::new("exec-a");
        let second = QueryExecutionId::new("exec-b");
        registry.register(first.clone(), "session-a").await.unwrap();
        registry
            .register(second.clone(), "session-b")
            .await
            .unwrap();

        let (first_ok, second_ok, cross_cancel) = tokio::join!(
            registry.validate_owner(&first, "session-a"),
            registry.validate_owner(&second, "session-b"),
            registry.validate_owner(&first, "session-b"),
        );
        assert!(first_ok.is_ok());
        assert!(second_ok.is_ok());
        assert!(cross_cancel.is_err());
    }
}
