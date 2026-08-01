//! Tauri IPC command surface.

pub mod ai;
mod backup;
mod config;
mod connection;
mod data;
mod file;
mod kiwi;
mod kv;
pub mod mcp;
mod query;
mod schema;
mod sync;

pub use ai::*;
pub use backup::*;
pub use config::*;
pub use connection::*;
pub use data::*;
pub use file::*;
pub use kiwi::*;
pub use kv::*;
pub use mcp::*;
pub use query::*;
pub use schema::*;
pub use sync::*;

use crate::ai::{AiProviderRegistry, SchemaContextBuilder};
use crate::cache::SchemaCache;
use crate::db::registry::DriverRegistry;
use crate::mcp::SkillRegistry;
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
    pub skill_registry: Arc<SkillRegistry>,
}

pub(crate) fn log_err(cmd: &str, e: &dyn std::fmt::Display) -> String {
    let msg = e.to_string();
    tracing::error!(cmd, error = %msg);
    msg
}
