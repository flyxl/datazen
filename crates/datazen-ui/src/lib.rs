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

pub mod active_connection_store;
pub mod ai_store;
// T125: the persisted-connections store (connectionStore.ts).
pub mod connection_store;
// T184: the dashboards store (dashboardStore.ts).
pub mod dashboard_store;
// T209: the per-panel editor-selection store (editorSelectionStore.ts).
pub mod editor_selection_store;
// T253: the installed-language-pack store (languagePackStore.ts).
pub mod language_pack_store;
// T254: the first-frame panel seed (stores/tableData/panelSeed.ts).
pub mod table_data_panel_seed;
pub mod ai_store_chat;
pub mod ai_store_rest;
pub mod table_data_connection_state;
pub mod table_data_count_strategy;
// T127: the dirty-state index of one table panel (dirtyIndex.ts).
pub mod table_data_dirty_index;
// T275: the pending-changes helpers (stores/tableData/pendingChanges.ts).
pub mod table_data_pending_changes;
// T294: the staged-edit actions (stores/tableData/pendingEditActions.ts), with
// the T328/T341/T353 placeholder cores (stageCellValue / stageEdit /
// stagePasteWrites) the owning tasks replace.
pub mod table_data_pending_edit_actions;
// T158: the new-row draft transitions (draftInserts.ts).
pub mod table_data_draft_inserts;
// T158: the enum-candidate cache transitions (enumCandidates.ts).
pub mod table_data_enum_candidates;
pub mod table_data_filter_utils;
// T210: the grid-selection geometry and state transitions (gridSelection.ts).
pub mod table_data_grid_selection;
// T233: the insert-location rule (insertLocation.ts) and the keyset
// half's state-only arm (keysetActions.ts — the cursor arms await T543).
pub mod table_data_insert_location;
pub mod table_data_keyset_actions;
pub mod components;
pub mod ai;
pub mod hooks; // the hooks/ ports (T038: useConfirmDialog is its first)
pub mod data_grid;
pub mod splitter;
pub mod sql_editor;
pub mod support;
pub mod session;
pub mod platform;
pub mod relation_metadata;
pub mod structure_editor;
pub mod constants;
pub mod i18n;
pub mod icons;
pub mod types;
pub mod state_def;
pub mod ai_chat_state;
pub mod ai_write_proposal_state;
pub mod session_state;
pub mod settings_state;
pub mod snippets_state;
pub mod connections_io_state;
// T293: the tab-closed notification (panelCloseNotifier.ts).
pub mod panel_close_notifier;
// T385: the query-run request bridge (queryRunRequestStore.ts).
pub mod query_run_request_store;
// T421: the schema request tracker (schemaRequestTracker.ts).
pub mod schema_request_tracker;
pub mod schema_metadata_keys;
pub mod search_state;
pub mod schema_workflows_state;
pub mod tools_state;
pub mod workflow_page_state;
// T471: the shell layout store (uiStore.ts).
pub mod ui_store;
// T232: the foreign-key navigation stack (fkNavigationStore.ts).
pub mod fk_navigation_store;
// T274: the metadata-identity binder seam, the pane-focus writer, and
// the pane key space (metadataIdentityBinding.ts / paneFocus.ts / paneKeys.ts).
pub mod metadata_identity_binding;
pub mod pane_focus;
pub mod pane_keys;
// T486: the workspace tabs store and the per-mode view-state snapshots.
pub mod workspace_tabs_store;
pub mod workspace_panel_state_store;
pub mod workspace_state;
pub mod wapps_state;
pub mod ui;
pub mod views;
pub mod app;


pub use constants::TABLE_PAGE_SIZE;
pub use constants::{APP_FONTS, APP_FONT_FAMILIES, NAVIGATOR_WIDTH_DEFAULT, NAVIGATOR_WIDTH_KEY, NAVIGATOR_WIDTH_MAX, NAVIGATOR_WIDTH_MIN, RAIL_WIDTH_COLLAPSED, RAIL_WIDTH_EXPANDED, SIDEBAR_RAIL_WIDTH_KEY};
pub use state_def::AppState;
pub use types::{Section, TableView, Tool};
// T049 (`@datazen/frontend/app` barrel): the shared-app launch surface,
// kept apart from the light port/contract surface like the React barrel.
pub use app::{app_with_adapter, run, run_with_adapter};
pub use bootstrap::start_shared_app_state;
pub mod bootstrap;
pub mod commands;
#[cfg(target_arch = "wasm32")]
pub use app::start;
pub use app::{app_ime, app_key, app_poll, view};
pub use views::query_selected;
pub use rune::tok::type_;
pub use ::datazen_ui_api::Setting;
pub use state_def::StructureState;



#[doc(hidden)]
pub mod test_support {
    pub use crate::app::view;
    pub use crate::{sql_editor, components, views, ui, session, support, icons, data_grid, splitter, relation_metadata, structure_editor, ai, i18n, constants, types, state_def};
    pub use crate::{AppState, Section, TableView, StructureState, query_selected, type_};
    pub use crate::constants::*;
    pub use ::datazen_ui_api::*;
    pub use rune::{Env, Runtime};
    pub fn runtime() -> Runtime<AppState> {
        let env = Env { system_dark: true, scale: 1.0 };
        Runtime::new(AppState::default(), view, (1440.0, 900.0), env)
    }
    pub fn answer_save_dialog(rt: &mut Runtime<AppState>, path: Option<&str>) {
        rt.frame(0.0);
        let (kind, _) = rt.file_dialog().expect("the press asked for a save dialog");
        assert_eq!(kind, rune::FileDialogKind::Save);
        rt.resolve_file_dialog(path.map(std::path::PathBuf::from)).expect("the dialog answers");
        rt.frame(0.0);
    }
}
