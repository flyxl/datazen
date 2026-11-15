//! AppState method implementations.

use crate::state_def::*;
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
    pub(crate) fn select(&mut self, section: Section) {
        self.active = section;
        if section == Section::Connections {
            self.sync_desktop_connections();
        }
        if section == Section::Dashboards {
            // E2: entering the dashboards page asks for every widget's
            // bars — the chart renders the cached answers only.
            self.refresh_widget_bars();
        }
    }

    /// Ask the desktop adapter for the connection list. A wired adapter's
    /// list replaces the memory port's demo list; an unwired one answers with
    /// the fixed error, which the connections page displays while the port's
    /// list stays.
    pub(crate) fn sync_desktop_connections(&mut self) {
        match self.desktop.list_connections() {
            Ok(connections) => {
                self.desktop_connections = Some(connections);
                self.desktop_error = None;
            }
            Err(message) => {
                self.desktop_connections = None;
                self.desktop_error = Some(message);
            }
        }
    }

    /// Collapse or expand a catalog node. The tree starts expanded; collapsed
    /// paths are UI state and survive catalog refreshes.
    pub(crate) fn toggle_expand(&mut self, path: &str) {
        if !self.collapsed.insert(path.to_owned()) {
            self.collapsed.remove(path);
        }
    }

    /// The catalog the tree, the ER view and the open-ER path read: the live
    /// host's once its open job answered one (N3), the memory port's
    /// otherwise. The demo path is unchanged.
    pub(crate) fn catalog(&self) -> datazen_ui_api::Catalog {
        self.desktop_catalog
            .clone()
            .unwrap_or_else(|| self.workspace.catalog())
    }

    /// A session is open when either world says so: the memory port's, or the
    /// live host's catalog (N3). The tree's buttons and the disconnect gate
    /// on this, not on the port alone.
    pub(crate) fn session_open(&self) -> bool {
        self.workspace.session().is_some() || self.desktop_catalog.is_some()
    }

    /// Re-take the catalog. Collapsed nodes stay collapsed: they live here,
    /// not in the port. A live session re-opens its connection through the
    /// same job path that opened it (N3); the memory port's catalog is the
    /// port's own truth and needs no ask.
    pub(crate) fn refresh_catalog(&mut self) {
        if let Some(id) = self.desktop_connection.clone() {
            let generation = self.next_read_generation();
            if self
                .desktop
                .begin_job(AdapterJob::OpenConnection { generation, id })
            {
                self.connect_loading = Some(generation);
            }
            return;
        }
        let _ = self.workspace.catalog();
    }

    pub(crate) fn set_table_view(&mut self, view: TableView) {
        self.table_view = view;
        // E1: a view switch re-asks for the selected table's structure —
        // the views render the cache, and the switch is the refresh (the
        // DDL drafts' apply refetches through the same path).
        if let Some(table) = self.selected_table() {
            self.begin_structure_read(&table);
        }
    }

    /// The settings in force: the host's once it persists them (N8), the
    /// memory port's while it does not. The adapter's read is memory-speed —
    /// the host caches its document.
    pub(crate) fn ui_settings(&self) -> Vec<Setting> {
        match self.desktop.read_ui_settings() {
            Ok(settings) => settings,
            Err(_) => self.workspace.read_settings(),
        }
    }

    /// The snippets in force (T3): the host's once its file store persists
    /// them, the memory port's while it does not. The host's read answers
    /// from its loaded file — memory-speed, like the settings read.
    pub(crate) fn host_snippets(&self) -> Vec<datazen_ui_api::Snippet> {
        self.desktop
            .snippets()
            .unwrap_or_else(|_| self.workspace.snippets())
    }

    /// The workflows in force (N13): the host's once its registry persists
    /// them, the memory port's while it does not. The host's read answers
    /// from its loaded files — the same memory-speed class as the settings
    /// read.
    pub(crate) fn host_workflows(&self) -> Vec<Workflow> {
        self.desktop
            .workflows()
            .unwrap_or_else(|_| self.workspace.workflows())
    }

    /// The dashboard's widgets in force (N13): the host's dashboard once it
    /// persists one, the memory port's while it does not.
    pub(crate) fn host_widgets(&self) -> Vec<DashboardWidget> {
        self.desktop
            .dashboard_widgets()
            .unwrap_or_else(|_| self.workspace.dashboard_widgets())
    }

    /// One widget's chart data (N13): the host's when its dashboard answers,
    /// the memory port's otherwise.
    pub(crate) fn host_widget_bars(&self, id: u64) -> Vec<(String, u32)> {
        self.desktop
            .widget_bars(id)
            .unwrap_or_else(|_| self.workspace.widget_bars(id).unwrap_or_default())
    }

    /// One structure read with the host first (N10): the live catalog's
    /// answer when the host has one, the memory port's otherwise. E1: the
    /// views no longer call these — the job fills the cache they render,
    /// and these serve the declined path (the demo process's memory port,
    /// memory-speed) that fills the same cache on the spot.
    pub(crate) fn structure_ddl(&self, table: &str) -> Result<String, String> {
        self.desktop
            .table_ddl(table)
            .or_else(|_| self.workspace.table_ddl(table))
    }

    pub(crate) fn structure_columns(&self, table: &str) -> Result<Vec<datazen_ui_api::ColumnSpec>, String> {
        self.desktop
            .table_columns(table)
            .or_else(|_| self.workspace.table_columns(table))
    }

    pub(crate) fn structure_indexes(&self, table: &str) -> Result<Vec<datazen_ui_api::IndexSpec>, String> {
        self.desktop
            .table_indexes(table)
            .or_else(|_| self.workspace.table_indexes(table))
    }

    pub(crate) fn structure_foreign_keys(
        &self,
        table: &str,
    ) -> Result<Vec<datazen_ui_api::ForeignKeySpec>, String> {
        self.desktop
            .table_foreign_keys(table)
            .or_else(|_| self.workspace.table_foreign_keys(table))
    }

    /// Write one setting: the host's use case first (N8); its fixed
    /// not-implemented error means the demo process, and the memory port
    /// answers instead; any other failure is real and the caller shows it.
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
    pub(crate) fn save_connection_form(&mut self) {
        // E5: the engines the host links are the form's whitelist — the
        // registry's own admission ("linked means offered"). A host that
        // answers none is the demo process: the memory port's own default
        // engine applies, and its rules take the save.
        let engines = self.desktop.available_engines();
        let engine = self.connection_engine.text.trim().to_owned();
        let engine = if engine.is_empty() {
            if engines.is_empty() {
                "postgres".to_owned()
            } else {
                self.connection_error = Some(format!(
                    "an engine is required — the host links {}",
                    engines.join(", ")
                ));
                return;
            }
        } else {
            engine
        };
        if !engines.is_empty() && !engines.contains(&engine) {
            self.connection_error = Some(format!(
                "the host links {} — '{engine}' is not among them",
                engines.join(", ")
            ));
            return;
        }
        let draft = datazen_ui_api::ConnectionDraft {
            name: self.connection_name.text.clone(),
            engine,
            address: self.connection_address.text.clone(),
            username: self.connection_username.text.trim().to_owned(),
            password: self.connection_password.text.clone(),
            database: self.connection_database.text.trim().to_owned(),
        };
        let saved = match self.desktop.save_connection(&draft) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.save_connection(&draft)
            }
            Err(error) => Err(error),
        };
        match saved {
            Ok(()) => {
                self.connection_error = None;
                self.connection_name = TextEdit::new(String::new());
                self.connection_engine = TextEdit::new(String::new());
                self.connection_address = TextEdit::new(String::new());
                self.connection_username = TextEdit::new(String::new());
                self.connection_password = TextEdit::new(String::new());
                self.connection_database = TextEdit::new(String::new());
            }
            Err(error) => self.connection_error = Some(error),
        }
    }

    pub(crate) fn save_snippet_form(&mut self) {
        let name = self.snippet_name.text.trim().to_owned();
        let sql = self.snippet_sql.text.trim().to_owned();
        // T3: the host's file store first; its fixed not-implemented error
        // means the demo process, and the memory port answers instead.
        let saved = match self.desktop.save_snippet(&name, &sql) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.save_snippet(&name, &sql)
            }
            Err(error) => Err(error),
        };
        match saved {
            Ok(()) => {
                self.snippet_error = None;
                self.snippet_name = TextEdit::new(String::new());
                self.snippet_sql = TextEdit::new(String::new());
            }
            Err(error) => self.snippet_error = Some(error),
        }
    }

    pub(crate) fn update_snippet_form(&mut self) {
        let name = self.snippet_name.text.trim().to_owned();
        let sql = self.snippet_sql.text.trim().to_owned();
        // T3: the host's file store first, the memory port's fallback.
        let updated = match self.desktop.update_snippet(&name, &sql) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.workspace.update_snippet(&name, &sql)
            }
            Err(error) => Err(error),
        };
        match updated {
            Ok(()) => {
                self.snippet_error = None;
                self.snippet_name = TextEdit::new(String::new());
                self.snippet_sql = TextEdit::new(String::new());
            }
            Err(error) => self.snippet_error = Some(error),
        }
    }

    pub(crate) fn delete_snippet(&mut self, name: &str) {
        // T3: the host's file store first, the memory port's fallback.
        let deleted = match self.desktop.delete_snippet(name) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => self.workspace.delete_snippet(name),
            Err(error) => Err(error),
        };
        if let Err(error) = deleted {
            self.snippet_error = Some(error);
        }
    }

    /// The favorites in force (T6): the host's root directory is the store;
    /// the demo process has none and the panel says so.
    pub(crate) fn host_favorites(&self) -> Vec<datazen_ui_api::FavoriteSpec> {
        self.desktop.favorites().unwrap_or_default()
    }

    /// Save the current query as a favorite (T6): the title from the form,
    /// the statement from the editor, the connection the live session holds.
    pub(crate) fn save_favorite_form(&mut self) {
        let title = self.favorite_title.text.trim().to_owned();
        let sql = self.sql.doc().to_owned();
        let connection = self.desktop_connection.clone().unwrap_or_default();
        let saved = match self.desktop.save_favorite(&connection, &title, &sql) {
            Ok(_) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                Err("favorites need the live host".into())
            }
            Err(error) => Err(error),
        };
        match saved {
            Ok(()) => {
                self.favorite_error = None;
                self.favorite_title = TextEdit::new(String::new());
            }
            Err(error) => self.favorite_error = Some(error),
        }
    }

    /// Open a favorite into a query tab (T6).
    pub(crate) fn open_favorite(&mut self, index: usize) {
        let Some(favorite) = self.host_favorites().get(index).cloned() else { return };
        self.active = Section::Workspace;
        if self.open_query_tab(&favorite.title).is_some() {
            self.sql = crate::sql_editor::SqlEditor::from_doc(favorite.sql);
        }
    }

    /// Rename a favorite to the form's title (T6): the title moves, the file
    /// does not — that is the host store's own semantics.
    pub(crate) fn rename_favorite_row(&mut self, index: usize) {
        let Some(favorite) = self.host_favorites().get(index).cloned() else { return };
        let title = self.favorite_title.text.trim().to_owned();
        if title.is_empty() {
            self.favorite_error = Some("a favorite needs a title".into());
            return;
        }
        let renamed = match self.desktop.rename_favorite(&favorite.id, &title) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                Err("favorites need the live host".into())
            }
            Err(error) => Err(error),
        };
        match renamed {
            Ok(()) => self.favorite_error = None,
            Err(error) => self.favorite_error = Some(error),
        }
    }

    /// Delete a favorite (T6).
    pub(crate) fn delete_favorite_row(&mut self, index: usize) {
        let Some(favorite) = self.host_favorites().get(index).cloned() else { return };
        let deleted = match self.desktop.delete_favorite(&favorite.id) {
            Ok(()) => Ok(()),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                Err("favorites need the live host".into())
            }
            Err(error) => Err(error),
        };
        if let Err(error) = deleted {
            self.favorite_error = Some(error);
        }
    }

    /// Parse a picked import file (T7): the preview before anything is
    /// saved. The host is the side with filesystem rights.
    pub(crate) fn begin_connections_import(&mut self, path: &str) {
        let password = self.import_password.text.clone();
        match self.desktop.parse_connections_import(path, &password) {
            Ok(preview) if preview.connections.is_empty() => {
                self.import_preview = None;
                self.import_notice =
                    Some("the import file carries no connections".into());
            }
            Ok(preview) => {
                self.import_preview = Some(preview);
                self.import_notice = None;
            }
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.import_preview = None;
                self.import_notice = Some("connection import needs the live host".into());
            }
            Err(error) => {
                self.import_preview = None;
                self.import_notice = Some(error);
            }
        }
    }

    /// Save the previewed import (T7): names already in the store are
    /// skipped; the notice counts what landed.
    pub(crate) fn confirm_connections_import(&mut self) {
        let Some(preview) = self.import_preview.take() else { return };
        match self.desktop.confirm_import(preview.handle) {
            Ok(landed) => {
                self.import_notice = Some(format!("{landed} connections imported"));
                self.sync_desktop_connections();
            }
            Err(error) => self.import_notice = Some(error),
        }
    }

    /// Drop the preview (T7).
    pub(crate) fn cancel_connections_import(&mut self) {
        self.import_preview = None;
        self.import_notice = None;
    }

    /// The outbound foreign key of the focused cell's column (T5), with the
    /// column's own pairing index. E1: the key list is the structure
    /// cache's foreign-key segment — the job's answer, never a live read.
    pub(crate) fn outbound_fk_of_focus(&self) -> Option<(datazen_ui_api::ForeignKeySpec, usize)> {
        let table = self.grid.table_label().to_owned();
        let column = self.grid.column_name(self.grid.focus().1)?.to_owned();
        let keys = self
            .structures
            .get(&table)
            .and_then(|entry| entry.data.as_ref())
            .map(|data| data.foreign_keys.clone())
            .unwrap_or_default();
        keys.into_iter().find_map(|key| {
            let index = key.columns.iter().position(|name| *name == column)?;
            Some((key, index))
        })
    }

    /// Open the referenced table filtered on the focused cell's value (T5).
    /// A composite key navigates by the focused column's own pairing, the
    /// same single-column answer the filter bar gives. E1: a structure that
    /// has not landed answers with an honest sentence, not a silent no-op.
    pub(crate) fn go_to_referenced_row(&mut self) {
        let table = self.grid.table_label().to_owned();
        let not_ready = match self.structures.get(&table) {
            None | Some(StructureState { loading: true, .. }) => {
                Some("the table's structure is still loading — try again once it lands".to_owned())
            }
            Some(StructureState { error: Some(error), .. }) => Some(error.clone()),
            Some(StructureState { data: None, .. }) => Some(
                "the table's structure is not loaded — switch the view or reopen the table"
                    .to_owned(),
            ),
            Some(StructureState { data: Some(_), .. }) => None,
        };
        if let Some(sentence) = not_ready {
            self.fk_error = Some(sentence);
            return;
        }
        // The structure landed: this key press clears the last FK sentence —
        // a successful navigation leaves no stale error behind.
        self.fk_error = None;
        let Some((key, index)) = self.outbound_fk_of_focus() else { return };
        let Some(target_column) = key.target_columns.get(index).cloned() else { return };
        let value = self.grid.cell(self.grid.focus().0, self.grid.focus().1);
        let target_table = key.target_table.clone();
        self.open_table_with_filter(&target_table, &target_column, &value);
    }

    /// Ask the host for the loaded table's inbound references (T5) as a job
    /// (E3) — the scan walks the whole catalog and never belongs on the
    /// frame loop. A host that declines keeps the synchronous read: the
    /// demo process has none, and the honest refusal names the live host.
    pub(crate) fn load_inbound_refs(&mut self) {
        let table = self.grid.table_label().to_owned();
        let generation = self.next_inbound_generation();
        let job = AdapterJob::InboundRefs { generation, table: table.clone() };
        if self.desktop.begin_job(job) {
            self.inbound_loading = Some(generation);
            self.inbound_refs = None;
            self.fk_error = None;
            return;
        }
        match self.desktop.table_inbound_references(&table) {
            Ok(refs) => {
                self.inbound_refs = Some(refs);
                self.fk_error = None;
            }
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                self.inbound_refs = None;
                self.fk_error = Some("the live host lists the references".into());
            }
            Err(error) => {
                self.inbound_refs = None;
                self.fk_error = Some(error);
            }
        }
    }

    /// Bump and return the next inbound generation (E3). Every ask starts
    /// one, so a delivery from a scan the state has moved past is dropped.
    pub(crate) fn next_inbound_generation(&mut self) -> u32 {
        self.inbound_generation += 1;
        self.inbound_generation
    }

    /// Hide the inbound list.
    pub(crate) fn close_inbound_refs(&mut self) {
        self.inbound_refs = None;
        self.fk_error = None;
    }

    /// Open a referencing table filtered on the focused row's value of the
    /// referenced column (T5).
    pub(crate) fn locate_inbound_ref(&mut self, index: usize) {
        let Some(list) = self.inbound_refs.as_ref() else { return };
        let Some(reference) = list.get(index).cloned() else { return };
        let Some(referenced_index) = (0..self.grid.cols)
            .find(|col| self.grid.column_name(*col) == Some(reference.referenced_column.as_str()))
        else {
            self.fk_error = Some(format!(
                "the loaded page has no column {}",
                reference.referenced_column
            ));
            return;
        };
        let value = self.grid.cell(self.grid.focus().0, referenced_index);
        self.open_table_with_filter(&reference.table, &reference.column, &value);
    }

    /// Open a table with a filter preset (T4/T5 share this): the tab's own
    /// open resets the filter, so the filter lands after it and the page
    /// re-reads.
    pub(crate) fn open_table_with_filter(&mut self, table: &str, column: &str, value: &str) {
        self.active = Section::Workspace;
        self.open_table_tab(table);
        self.filter = Some(ColumnFilter { column: column.to_owned(), contains: value.to_owned() });
        self.sort = None;
        self.table_page = 0;
        self.reload_table_page();
    }

    /// The value search's five match modes, in cycle order (T4). The ids
    /// are the React vocabulary's (`ValueSearchMode`).
    const VALUE_SEARCH_MODES: [&'static str; 5] =
        ["contains", "notContains", "exact", "prefix", "suffix"];

    pub(crate) fn value_search_mode(&self) -> &'static str {
        Self::VALUE_SEARCH_MODES
            [(self.value_search_mode as usize) % Self::VALUE_SEARCH_MODES.len()]
    }

    pub(crate) fn cycle_value_search_mode(&mut self) {
        self.value_search_mode =
            (self.value_search_mode + 1) % Self::VALUE_SEARCH_MODES.len() as u32;
    }

    /// Start a value search over the live session (T4): one job for the
    /// whole scan. The demo process has no live session and the memory port
    /// has no scan — the honest refusal names both.
    pub(crate) fn begin_value_search(&mut self) {
        let value = self.value_search_value.text.trim().to_owned();
        if value.is_empty() {
            self.value_search_error = Some("a value is required".into());
            return;
        }
        if !self.session_open() {
            self.value_search_error =
                Some("value search needs an open connection".into());
            return;
        }
        let generation = self.next_value_search_generation();
        let job = AdapterJob::ValueSearch {
            generation,
            value,
            mode: self.value_search_mode().to_owned(),
        };
        if self.desktop.begin_job(job) {
            self.value_search_loading = Some(generation);
            self.value_search_error = None;
            self.value_search_hits.clear();
            self.value_search_query = self.value_search_value.text.trim().to_owned();
        } else {
            self.value_search_error =
                Some("value search needs the live host".into());
        }
    }

    /// Cancel the running search: the generation moves past it, so the
    /// scan's delivery lands nowhere.
    pub(crate) fn cancel_value_search(&mut self) {
        if self.value_search_loading.is_some() {
            let _ = self.next_value_search_generation();
            self.value_search_loading = None;
            self.value_search_error = Some("the search was cancelled".into());
        }
    }

    pub(crate) fn next_value_search_generation(&mut self) -> u32 {
        self.value_search_generation += 1;
        self.value_search_generation
    }

    /// Open the hit's table with a filter on the hit's column (T4): the
    /// locate intent the React dialog answers with.
    pub(crate) fn locate_value_search_hit(&mut self, index: usize) {
        let Some(hit) = self.value_search_hits.get(index).cloned() else { return };
        let value = self.value_search_query.clone();
        self.open_table_with_filter(&hit.table, &hit.column, &value);
    }

    /// The export formats the dialog collects, in cycle order.
    const EXPORT_FORMATS: [&'static str; 3] = ["csv", "json", "sql"];

    pub(crate) fn export_format(&self) -> &'static str {
        Self::EXPORT_FORMATS[(self.export_format_index as usize) % Self::EXPORT_FORMATS.len()]
    }

    pub(crate) fn cycle_export_format(&mut self) {
        self.export_format_index =
            (self.export_format_index + 1) % Self::EXPORT_FORMATS.len() as u32;
    }

    /// Ask for an export job id (N11): the host's export first, the memory
    /// port's when the host has none. A missing range gets no job.
    /// T2: the save path is picked first; the answer in `export_answered`
    /// starts the job (or cancels, or falls back to the default directory
    /// when no dialog could show).
    pub(crate) fn begin_export_form(&mut self, tx: &mut Tx) {
        let table = self.grid.table_label().to_owned();
        let format = self.export_format().to_owned();
        let range = self.export_range.text.trim().to_owned();
        let options = FileDialogOptions {
            title: format!("Export {table} as {format}"),
            file_name: Some(format!("export-{table}.{format}")),
            ..Default::default()
        }
        .filter(
            if format == "csv" { "CSV files" } else { "JSON files" },
            &[format.as_str()],
        );
        tx.save_file(options, move |state: &mut AppState, result, _| {
            state.export_answered(table, format, range, result);
        });
    }

}
