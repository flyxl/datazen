//! Bootstrap pure helpers: log/settings resolution, driver-type scan, MCP flags.

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

/// Parse log level and custom log path from a settings JSON value.
pub(crate) fn parse_log_settings_fields(v: &serde_json::Value) -> (String, String) {
    let level = v
        .get("logLevel")
        .and_then(|v| v.as_str())
        .unwrap_or("info")
        .to_string();
    let path = v
        .get("logPath")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    (level, path)
}

/// Resolve the directory used for application logs.
pub(crate) fn resolve_log_dir(data_dir: &std::path::Path, custom_log_path: &str) -> PathBuf {
    if custom_log_path.is_empty() {
        data_dir.join("logs")
    } else {
        PathBuf::from(custom_log_path)
    }
}

/// Resolve the AI context files directory.
pub(crate) fn resolve_context_dir(
    data_dir: &std::path::Path,
    context_dir_setting: &str,
) -> PathBuf {
    if context_dir_setting.is_empty() {
        data_dir.join("contexts")
    } else {
        PathBuf::from(context_dir_setting)
    }
}

/// Whether CLI args request headless MCP stdio mode (`--mcp` / `--mcp-stdio`).
pub fn is_mcp_stdio_mode(args: &[String]) -> bool {
    args.iter().any(|a| a == "--mcp" || a == "--mcp-stdio")
}

/// Build the tracing env filter: honor `RUST_LOG` when set, else use settings log level.
pub(crate) fn build_tracing_env_filter(log_level: &str) -> tracing_subscriber::EnvFilter {
    tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(log_level))
}

/// True when a window's logical size covers the full monitor (used for fullscreen-changed emit).
pub(crate) fn is_fullscreen_for_monitor(
    window_width: u32,
    window_height: u32,
    monitor_width: u32,
    monitor_height: u32,
) -> bool {
    window_width >= monitor_width && window_height >= monitor_height
}

/// Resolve bundled prompt templates directory from Tauri resource dir when available.
pub(crate) fn resolve_prompts_dir(resource_dir: Option<PathBuf>) -> Option<PathBuf> {
    resource_dir.map(|d| d.join("prompts"))
}

/// Whether embedded MCP server should auto-start during GUI setup.
pub(crate) fn should_auto_start_embedded_mcp(mcp_server_enabled: bool) -> bool {
    mcp_server_enabled
}

pub(crate) fn resolve_log_settings() -> (String, PathBuf) {
    let data_dir =
        crate::store::Store::default_app_data_dir().unwrap_or_else(|_| PathBuf::from("."));
    resolve_log_settings_in(&data_dir)
}

/// Core logic extracted so tests can inject a temporary `data_dir`.
pub(crate) fn resolve_log_settings_in(data_dir: &std::path::Path) -> (String, PathBuf) {
    let settings_path = data_dir.join("settings.json");

    let (level, custom_path) = std::fs::read_to_string(&settings_path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .map(|v| parse_log_settings_fields(&v))
        .unwrap_or_else(|| ("info".to_string(), String::new()));

    let log_dir = resolve_log_dir(data_dir, &custom_path);

    (level, log_dir)
}
