//! Shared DataZen UI entry point for native and WebAssembly hosts.
//!
//! The page shell is deliberately independent of DataZen's transport and storage
//! implementations. Platform adapters will be added behind typed UI ports.
//!
//! Port of `@datazen/frontend` — the package's light entry
//! (`packages/frontend/src/index.ts`, T051). That file is the barrel that
//! keeps the package's sanctioned surface narrow: `.` re-exports only the
//! light surface — the platform ports and capability bits plus the
//! host-support helpers the desktop adapters consume — while `./app` (the
//! launch surface, the T049 ruling on the re-exports just below) and
//! `./shell` (T052) are separate entries, so a light consumer's dependency
//! graph never has to know the app graph exists. In Rust the barrel
//! dissolves into the crate split: the light surface is the
//! [`datazen_ui_api`] crate (the contracts family — `DesktopAdapter`,
//! `Setting`, `DESKTOP_NOT_IMPLEMENTED` — plus the ported shared helpers
//! the React barrel re-exported from `.`), and hosts reach it the way React
//! consumers reached `.`, with one import of the api crate. The
//! barrel's per-name mapping, in three families:
//!
//! * `export * from './platform/contracts'` → the api crate's public
//!   surface is the counterpart wholesale. `AppSettings` (from
//!   `types/index`) is today the typed `Setting` enum; the fuller profile
//!   arrives with the settings-layer task. `McpHttpStatus` rides on
//!   `McpHttpResponse::status` (the mcpClient port).
//! * capability bits (`DESKTOP_CAPABILITIES`, `WEB_CAPABILITIES`,
//!   `IS_DESKTOP_BUILD`) are a compile-time property of the form factor in
//!   React and stay one here: the form factor is the linked assembly — the
//!   native host injects its adapter through [`run_with_adapter`] /
//!   [`app_with_adapter`], the web form is the wasm `start` entry. No
//!   runtime capability table exists to re-export and none is invented.
//! * host-support helpers with no Rune counterpart yet stay out, each
//!   owned by a later task rather than faked here: the cross-window event
//!   rail (`createBrowserEventChannel`) and the updater family are
//!   web-host / release machinery; the wapp asset-origin resolver keeps
//!   its desktop custom-scheme kernel in `crates/datazen` and has no UI
//!   consumer yet; `getWindowKind` has one window to resolve today
//!   (the seam `views/app_root.rs` already records).
//!
//! The React guard script that bans `./src/*` deep imports is this crate's
//! privacy surface: the deep-import ban is the module system itself.

pub(crate) mod active_connection_store;
pub(crate) mod ai_store;
pub(crate) mod ai_store_chat;
pub(crate) mod ai_store_rest;
pub(crate) mod table_data_connection_state;
pub(crate) mod table_data_count_strategy;
pub(crate) mod components;
pub(crate) mod ai;
pub(crate) mod hooks; // the hooks/ ports (T038: useConfirmDialog is its first)
pub mod data_grid;
pub mod splitter;
pub mod sql_editor;
pub(crate) mod support;
pub(crate) mod session;
pub(crate) mod platform;
pub(crate) mod relation_metadata;
pub(crate) mod structure_editor;
mod constants;
pub(crate) mod i18n;
mod icons;
mod types;
mod state_def;
mod ai_chat_state;
mod ai_write_proposal_state;
mod session_state;
mod settings_state;
mod snippets_state;
mod connections_io_state;
mod search_state;
mod schema_workflows_state;
mod tools_state;
mod workspace_state;
mod wapps_state;
pub(crate) mod ui;
mod views;
mod app;


pub use constants::TABLE_PAGE_SIZE;
pub(crate) use constants::{APP_FONTS, APP_FONT_FAMILIES, NAVIGATOR_WIDTH_DEFAULT, NAVIGATOR_WIDTH_KEY, NAVIGATOR_WIDTH_MAX, NAVIGATOR_WIDTH_MIN, RAIL_WIDTH_COLLAPSED, RAIL_WIDTH_EXPANDED, SIDEBAR_RAIL_WIDTH_KEY};
pub use state_def::AppState;
pub(crate) use types::{Section, TableView, Tool};
// T049 (`@datazen/frontend/app` barrel): the shared-app launch surface,
// kept apart from the light port/contract surface like the React barrel.
pub use app::{app_with_adapter, run, run_with_adapter};
pub use bootstrap::start_shared_app_state;
mod bootstrap;
pub(crate) mod commands;
#[cfg(target_arch = "wasm32")]
pub use app::start;
pub(crate) use app::{app_ime, app_key, app_poll, view};
pub(crate) use views::query_selected;
pub(crate) use rune::tok::type_;
pub(crate) use datazen_ui_api::Setting;
pub(crate) use state_def::StructureState;


#[cfg(test)]
mod tests;
