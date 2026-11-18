//! Shared DataZen UI entry point for native and WebAssembly hosts.
//!
//! The page shell is deliberately independent of DataZen's transport and storage
//! implementations. Platform adapters will be added behind typed UI ports.

pub(crate) mod components;
pub mod data_grid;
pub mod splitter;
pub mod sql_editor;
pub(crate) mod support;
pub(crate) mod session;
pub(crate) mod relation_metadata;
mod constants;
pub(crate) mod i18n;
mod icons;
mod types;
mod state_def;
mod ai_chat_state;
mod session_state;
mod settings_state;
mod snippets_state;
mod connections_io_state;
mod search_state;
mod schema_workflows_state;
mod tools_state;
mod workspace_state;
pub(crate) mod ui;
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
