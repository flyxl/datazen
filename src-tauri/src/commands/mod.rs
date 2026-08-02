//! Tauri IPC command surface.

mod adb;
pub mod ai;
mod backup;
mod config;
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
use crate::mcp::{McpClientManager, SkillRegistry};
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
    pub skill_registry: Arc<SkillRegistry>,
    pub mcp_client_manager: Arc<McpClientManager>,
}

