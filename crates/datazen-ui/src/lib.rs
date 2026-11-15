//! Shared DataZen UI entry point for native and WebAssembly hosts.
//!
//! The page shell is deliberately independent of DataZen's transport and storage
//! implementations. Platform adapters will be added behind typed UI ports.

pub mod data_grid;
pub mod splitter;
pub mod sql_editor;
mod constants;
mod icons;
mod types;
mod state_def;
mod state_session;
mod state_settings;
mod state_snippets;
mod state_connections_io;
mod state_search;
mod state_schema_workflows;
mod state_tools;
mod state_workspace;
mod views;
mod app;


pub use constants::TABLE_PAGE_SIZE;
pub(crate) use constants::{APP_FONTS, APP_FONT_FAMILIES, NAVIGATOR_WIDTH_DEFAULT, NAVIGATOR_WIDTH_KEY, NAVIGATOR_WIDTH_MAX, NAVIGATOR_WIDTH_MIN, RAIL_WIDTH_COLLAPSED, RAIL_WIDTH_EXPANDED, SIDEBAR_RAIL_WIDTH_KEY};
pub use state_def::AppState;
pub(crate) use types::{Section, TableView, Tool};
pub use app::{app_with_adapter, run, run_with_adapter};
#[cfg(target_arch = "wasm32")]
pub use app::start;
pub(crate) use app::{app_ime, app_key, app_poll, view};
pub(crate) use views::query_selected;
pub(crate) use rune::tok::type_;
pub(crate) use datazen_ui_api::Setting;
pub(crate) use state_def::StructureState;


#[cfg(test)]
mod tests;
