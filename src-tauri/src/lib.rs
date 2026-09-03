pub mod ai;
mod app_data_archive;
mod app_menu;
mod bootstrap;
mod cache;
mod commands;
mod dashboard;
pub mod data_sync;
mod data_transfer;
pub mod db;
mod driver_init;
mod extensions;
mod i18n_locale;
mod log_redact;
pub mod mcp;
mod monitor;
mod product_features;
mod redis_flush_gate;
pub mod schema_diff;
mod schema_objects;
mod services;
mod sql_guard;
mod ssh_known_hosts;
pub mod ssh_tunnel;
mod store;
mod theme;
pub mod transfer;
mod tray;
mod util;
pub mod workflow;

pub use store::{AppDb, HistoryDb};
pub(crate) use transfer::adapter_registry::SyncAdapterRegistry;

#[cfg(any(test, feature = "test-harness"))]
pub(crate) mod testing;

#[cfg(feature = "test-harness")]
pub use commands::test_harness;

use commands::AppState;

// Re-exports for crate-internal callers (tray, commands, testing).
#[allow(unused_imports)]
pub(crate) use app_menu::{
    menu_action_for_id, menu_emit_needs_main_focus, menu_label, menu_labels, take_once_slot,
    theme_menu_item_checked, MenuAction,
};
#[allow(unused_imports)]
pub(crate) use bootstrap::{
    build_tracing_env_filter, finish_app_state, is_fullscreen_for_monitor, is_mcp_stdio_mode,
    parse_log_settings_fields, resolve_context_dir, resolve_log_dir, resolve_prompts_dir,
    should_auto_start_embedded_mcp, unique_driver_types,
};

pub use bootstrap::{run, run_mcp_stdio};
