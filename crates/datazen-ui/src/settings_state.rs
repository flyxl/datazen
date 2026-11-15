//! AppState method implementations.

#![allow(unused_imports)]
use crate::state_def::*;
use crate::views::{next_connection, workflow_yaml};
use crate::constants::*;
use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use crate::types::{Section, TableView, Tool};
use crate::{icons, splitter};
use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnDraft, ColumnFilter, ColumnSort, ConnectionProfile,
    DashboardWidget, DESKTOP_NOT_IMPLEMENTED, DesktopAdapter, MemoryWorkspace, ProgressSnapshot,
    QueryApi, Setting, SettingsApi, TabKind, Tabs, TablePage, UnimplementedAdapter, Workflow,
    WorkflowStepResult, WorkspaceApi,
};
#[cfg(not(target_arch = "wasm32"))]
use datazen_ui_api::UseCaseAdapter;
use rune::tok::{self, type_};
use rune::{
    App, AppSpec, Color, Cx, El, FileDialogOptions, FileDialogResult, Ime, Key, KeyEvent,
    TextEdit, Titlebar, Tx, col, oklch, row, text, text_input,
};

impl AppState {
    pub(crate) fn ui_settings(&self) -> Vec<Setting> {
        match self.desktop.read_ui_settings() {
            Ok(settings) => settings,
            Err(_) => self.workspace.read_settings(),
        }
    }

    /// The snippets in force (T3): the host's once its file store persists
    /// them, the memory port's while it does not. The host's read answers
    /// from its loaded file — memory-speed, like the settings read.
    pub(crate) fn write_ui_setting(&mut self, setting: Setting) -> Result<(), String> {
        match self.desktop.write_ui_setting(setting.clone()) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.write_setting(setting)
            }
            Err(error) => Err(error),
        }
    }

    /// One persisted layout size in points (L1), read from the settings in
    /// force. `None` means the region has never been resized: the caller
    /// applies its default.
    pub(crate) fn layout_size(&self, key: &str) -> Option<u32> {
        self.ui_settings().into_iter().find_map(|setting| match setting {
            Setting::Layout { key: region, value } if region == key => Some(value),
            _ => None,
        })
    }

    /// Persist one layout size in points (L1): the splitter stores it once
    /// the drag ends, so a restart reopens the same proportions.
    pub(crate) fn set_layout_size(&mut self, key: &str, value: u32) -> Result<(), String> {
        self.write_ui_setting(Setting::Layout { key: key.into(), value })
    }

    /// L4: the rail is expanded (icon + label) by default; collapsed shows
    /// icons only. Persisted as its width in points.
    pub(crate) fn sidebar_expanded(&self) -> bool {
        self.sidebar_rail_width() > RAIL_WIDTH_COLLAPSED
    }
    pub(crate) fn sidebar_rail_width(&self) -> u32 {
        let w = self.layout_size(SIDEBAR_RAIL_WIDTH_KEY).unwrap_or(RAIL_WIDTH_EXPANDED);
        if w <= RAIL_WIDTH_COLLAPSED { RAIL_WIDTH_COLLAPSED } else { RAIL_WIDTH_EXPANDED }
    }
    pub(crate) fn toggle_sidebar_expanded(&mut self) {
        let next = if self.sidebar_expanded() { RAIL_WIDTH_COLLAPSED } else { RAIL_WIDTH_EXPANDED };
        let _ = self.set_layout_size(SIDEBAR_RAIL_WIDTH_KEY, next);
    }

    /// L4: navigator panel width in points, clamped to its resizable range.
    pub(crate) fn navigator_width(&self) -> u32 {
        let w = self.layout_size(NAVIGATOR_WIDTH_KEY).unwrap_or(NAVIGATOR_WIDTH_DEFAULT);
        w.clamp(NAVIGATOR_WIDTH_MIN, NAVIGATOR_WIDTH_MAX)
    }

    /// The table page size from the settings in force (fallback: the built-in 25).
    pub(crate) fn table_page_size(&self) -> u32 {
        self.ui_settings()
            .iter()
            .find_map(|setting| match setting {
                Setting::TablePageSize { rows } => Some(*rows),
                _ => None,
            })
            .unwrap_or(TABLE_PAGE_SIZE)
    }

    /// Set the table page size from the settings input; an invalid value keeps
    /// the old one and shows the port's error.
    pub(crate) fn set_table_page_size(&mut self) {
        let rows = self.page_size_input.text.trim().parse::<u32>();
        match rows {
            Ok(rows) => match self.write_ui_setting(Setting::TablePageSize { rows }) {
                Ok(()) => self.settings_error = None,
                Err(error) => self.settings_error = Some(error),
            },
            Err(_) => self.settings_error = Some("table page size must be a number".into()),
        }
    }
    pub(crate) fn set_query_max_rows(&mut self) {
        let rows = self.max_rows_input.text.trim().parse::<u32>();
        match rows {
            Ok(rows) => match self.write_ui_setting(Setting::QueryMaxRows { rows }) {
                Ok(()) => self.settings_error = None,
                Err(error) => self.settings_error = Some(error),
            },
            Err(_) => self.settings_error = Some("query max rows must be a number".into()),
        }
    }

    /// Save the connection form through the port. A missing address is refused
    /// and nothing is saved. E5: the six fields travel as one draft — the
    /// live host persists it through its connection use case (the password
    /// lands sealed), the memory port keeps its semantics.
    /// Flip the theme. The only write goes to the settings in force.
    pub(crate) fn toggle_theme(&mut self) {
        let dark = self
            .ui_settings()
            .iter()
            .find_map(|setting| match setting {
                Setting::Theme { dark } => Some(*dark),
                _ => None,
            })
            .unwrap_or(true);
        let _ = self.write_ui_setting(Setting::Theme { dark: !dark });
    }
}
