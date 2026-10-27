mod app_data_archive;
mod app_menu;
#[path = "bootstrap/mod.rs"]
mod bootstrap;
mod commands;
mod dashboard;
pub use datazen_modules::migration::data_sync;
pub(crate) use datazen_modules::migration::data_transfer;
mod driver_init;
#[cfg(test)]
mod driver_registry_tests;
mod e2e_quiet;
mod i18n_locale;
pub mod mcp;
mod monitor;
pub mod platform;
mod product_features;
mod redis_flush_gate;
pub use datazen_modules::migration::schema_diff;
mod services;
mod store;
mod theme;
mod tls;
pub use datazen_modules::migration::data_transfer::transfer;
mod tray;
pub mod tunnel;
mod wapps;
pub mod workflow;

pub use store::{AppDb, HistoryDb};

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
    build_desktop_job_host, build_tracing_env_filter, finish_app_state, is_fullscreen_for_monitor,
    parse_log_settings_fields, recover_desktop_jobs_at_startup, resolve_context_dir,
    resolve_log_dir, resolve_prompts_dir, should_auto_start_embedded_mcp, unique_driver_types,
};

pub use bootstrap::{is_mcp_stdio_mode, run, run_mcp_stdio};
pub use tls::install_default_crypto_provider;
