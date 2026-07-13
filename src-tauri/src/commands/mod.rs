//! Tauri IPC command surface.

mod backup;
mod config;
mod connection;
mod data;
mod file;
mod kiwi;
mod kv;
mod query;
mod schema;
mod sync;

pub use backup::*;
pub use config::*;
pub use connection::*;
pub use data::*;
pub use file::*;
pub use kiwi::*;
pub use kv::*;
pub use query::*;
pub use schema::*;
pub use sync::*;

use crate::cache::SchemaCache;
use crate::db::registry::DriverRegistry;
use crate::services::ConnectionManager;
use crate::store::Store;
use std::sync::Arc;

/// Shared application state injected into every command handler.
pub struct AppState {
    #[allow(dead_code)]
    pub driver_registry: Arc<DriverRegistry>,
    pub connection_manager: Arc<ConnectionManager>,
    pub store: Arc<Store>,
    pub schema_cache: Arc<SchemaCache>,
}

pub(crate) fn log_err(cmd: &str, e: &dyn std::fmt::Display) -> String {
    let msg = e.to_string();
    tracing::error!(cmd, error = %msg);
    msg
}
