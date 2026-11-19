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
        if section == Section::Wapps && !self.wapps_loaded {
            // T072: React's mount effect (`WappManagementPage.tsx:141-143`:
            // `useEffect(() => { if (!loaded) void fetch(); }, [loaded])`) —
            // the wapp store fetches the moment the page mounts, which the
            // rail's entry into the section is here; the `!loaded` guard
            // keeps a loaded store from refetching on every entry.
            self.fetch_wapps();
        }
        if section == Section::Workflows {
            // T145: React's page mount effect (`WorkflowPage.tsx:231-241`) —
            // the real halves run: the workflow list (`loadWorkflows`) and
            // the connections (`reloadConnections`, into the form's slot).
            // The command seams stay: `workflowGetDir` keeps the dir slot
            // empty (React's own conditional then hides the open-dir
            // button), and the reload/listeners need their use cases. React
            // refetches on every mount — the section entry is this build's
            // mount — so no `loaded` guard here (the wapps page's own).
            self.fetch_workflow_list();
            self.reload_workflow_connections();
        }
    }

    /// Ask the desktop adapter for the connection list. A wired adapter's
    /// list replaces the memory port's demo list; an unwired one answers with
    /// the fixed error, which the connections page displays while the port's
    /// list stays.
    ///
    /// T020: the fetch's own failure is kept separately in
    /// [`Self::connections_load_error`] (the React store's `error`, the main
    /// window's load gate), and a failed fetch keeps the previous list —
    /// the zustand store writes only the error on failure
    /// (`connectionStore.ts:121-127`), so its `connections` survive. The
    /// unwired marker is not a load failure: the memory port answers by
    /// design (see `app.rs`'s H3 note) and the page keeps its banner.
    pub(crate) fn sync_desktop_connections(&mut self) {
        match self.desktop.list_connections() {
            Ok(connections) => {
                self.desktop_connections = Some(connections);
                self.desktop_error = None;
                self.connections_load_error = None;
            }
            Err(message) => {
                self.desktop_error = Some(message.clone());
                self.connections_load_error = if message == DESKTOP_NOT_IMPLEMENTED {
                    None
                } else {
                    Some(message)
                };
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

    /// A1: whether a session is currently open for this connection id — the
    /// live host's open session, or the memory port's. Drives the card
    /// list's Connected pill and Open button.
    pub(crate) fn connection_is_open(&self, id: &str) -> bool {
        self.desktop_connection.as_deref() == Some(id)
            || self
                .workspace
                .session()
                .is_some_and(|session| session.connection_id == id)
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
    pub(crate) fn open_connection(&mut self, id: &str) {
        // N3: a live host opens the connection as a job — real driver IO off
        // the frame loop. Until the catalog (or the failure sentence) lands
        // the page stays put and says the connect is in flight; the memory
        // port's instant path answers when the host declines.
        self.record_recent_connection(id);
        let generation = self.next_read_generation();
        if self
            .desktop
            .begin_job(AdapterJob::OpenConnection { generation, id: id.to_owned() })
        {
            self.connect_loading = Some(generation);
            return;
        }
        if self.workspace.connect(id).is_err() {
            return;
        }
        self.collapsed.clear();
        self.table_page = 0;
        self.sort = None;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        self.fk_breadcrumbs.clear();
        // H4: the initial page goes through the same read path as every
        // other one: the adapter's use case first, the port as the fallback.
        self.show_table("customers");
    }

    /// Record a connection as recently opened in session UI state (A1). Not
    /// persisted and no contract change; keeps the most recent entries first.
    pub(crate) fn record_recent_connection(&mut self, id: &str) {
        self.recent_connection_ids.retain(|existing| existing != id);
        self.recent_connection_ids.insert(0, id.to_owned());
        if self.recent_connection_ids.len() > 8 {
            self.recent_connection_ids.truncate(8);
        }
    }

    /// Connections visible to the UI: the desktop adapter's list when wired,
    /// otherwise the memory port's demo list.
    pub(crate) fn visible_connections(&self) -> Vec<ConnectionProfile> {
        self.desktop_connections
            .clone()
            .unwrap_or_else(|| self.workspace.connections().to_vec())
    }
    pub(crate) fn disconnect(&mut self) {
        self.workspace.disconnect();
        self.tabs = Tabs::new();
        self.grid.set_editable(false);
        self.table_page = 0;
        self.sort = None;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        self.result_grid = None;
        self.result_focused = false;
        self.collapsed.clear();
        self.er_schema = None;
        self.er_positions.clear();
        self.er_drag = None;
        // N1: a read the disconnect invalidates must not land later, and no
        // loading marker outlives the session it belonged to. N3: the live
        // catalog belongs to the session too.
        self.next_read_generation();
        self.table_loading = None;
        self.commit_loading = None;
        self.query_loading = None;
        self.connect_loading = None;
        self.desktop_catalog = None;
        self.desktop_connection = None;
        self.structures.clear();
        self.widget_bars.clear();
        self.inbound_refs = None;
        self.inbound_loading = None;
        self.fk_error = None;
        self.fk_breadcrumbs.clear();
    }
    pub(crate) fn open_table_tab(&mut self, title: &str) -> Option<u32> {
        let id = self.tabs.open(TabKind::Table, title)?;
        // E1: opening a table asks for its structure — the four views and
        // the outbound FK navigation render the cached answer only.
        self.begin_structure_read(title);
        self.sync_grid_to_selected_tab();
        Some(id)
    }
    pub(crate) fn open_query_tab(&mut self, title: &str) -> Option<u32> {
        let id = self.tabs.open(TabKind::Query, title)?;
        self.sync_grid_to_selected_tab();
        Some(id)
    }
    pub(crate) fn select_tab(&mut self, id: u32) -> bool {
        if !self.tabs.select(id) {
            return false;
        }
        self.sync_grid_to_selected_tab();
        true
    }

    /// T071 (segment-2's T070): the running-execution close host's ask entry —
    /// React's gate closure `confirm({...})` inside
    /// `RunningExecutionCloseHost.tsx`. The store cannot open a dialog, so it
    /// asks this host, which holds the one pending request until the human
    /// answers; each ask is a fresh mount (the epoch), so the prompt reopens
    /// at the top and its copy mark resets. The store half — the ask queue
    /// that serializes several closes behind one prompt
    /// (`askRunningExecutionGate`, "close all" over several running tabs) —
    /// is T396's port; the host assumes asks arrive serialized, as React's
    /// `useConfirmDialog` does ("one at a time by construction").
    ///
    /// No production caller yet: the close guard that would ask (the panel
    /// store's `createRunningCloseGuard` wiring) ports with T396, and until
    /// then the tab strip's close button keeps closing directly. The entry
    /// is the host's half of the contract the guard will call.
    #[allow(dead_code)]
    pub(crate) fn ask_running_execution_close(
        &mut self,
        request: crate::views::RunningExecutionCloseRequest,
    ) {
        self.running_close_request = Some(request);
        self.running_close_epoch = self.running_close_epoch.wrapping_add(1);
        self.running_close_outcome = None;
        self.running_close_sql_copied = false;
    }

    /// T071: the prompt's answer — React `handleClose(result)` plus the
    /// promise's resolve: records `(request, allowed)` for the asker (`true`
    /// is "stop and close", `false` keeps the tab) and takes the prompt down.
    /// A second answer without a pending ask changes nothing, as React's
    /// settled promise resolves only once.
    pub(crate) fn answer_running_execution_close(&mut self, allowed: bool) {
        if let Some(request) = self.running_close_request.take() {
            self.running_close_outcome = Some((request, allowed));
        }
    }
}
