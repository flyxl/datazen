//! Application bootstrap: logging, AppState assembly, GUI/MCP entry points.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

use crate::ai::SchemaContextBuilder;
use crate::cache::SchemaCache;
use crate::commands::AppState;
use crate::db::init_drivers;
use crate::monitor::MonitorEngine;
use crate::services::ConnectionManager;
use crate::store::Store;
use crate::transfer::adapter_registry::SyncAdapterRegistry;
use tauri::Emitter;
use tauri::Manager;

#[cfg(target_os = "macos")]
use crate::app_menu::setup_menu;
use crate::driver_init;
use crate::mcp;
use crate::redis_flush_gate;
use crate::theme;
use crate::tray;
use crate::wapps;
use crate::workflow;

/// Collect unique driver type ids from saved connections (stable insertion order).
pub(crate) fn unique_driver_types(connections: &[crate::db::ConnectionConfig]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    connections
        .iter()
        .filter_map(|c| {
            seen.insert(c.database_type.clone())
                .then_some(c.database_type.clone())
        })
        .collect()
}
