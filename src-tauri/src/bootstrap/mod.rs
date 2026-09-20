//! Application bootstrap: logging, AppState assembly, GUI/MCP entry points.
//!
//! Layout (each file has a single responsibility, no mid-function `include!`):
//! - `helpers`   — log/path/MCP flag pure functions
//! - `app_state` — AppState construction for GUI + headless MCP
//! - `run`       — `run_mcp_stdio` + full `run` builder chain (incl. generate_handler!)
//! - `tests`     — unit tests for helpers + IPC registration contracts

mod app_state;
mod helpers;
mod run;

#[cfg(test)]
mod tests;

pub use helpers::is_mcp_stdio_mode;
pub use run::{run, run_mcp_stdio};

// Crate-internal re-exports used by tray, commands, testing.
pub(crate) use app_state::finish_app_state;
pub(crate) use helpers::{
    build_tracing_env_filter, is_fullscreen_for_monitor, parse_log_settings_fields,
    resolve_context_dir, resolve_log_dir, resolve_prompts_dir, should_auto_start_embedded_mcp,
    unique_driver_types,
};
