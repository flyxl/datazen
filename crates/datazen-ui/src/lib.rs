//! Shared DataZen UI entry point for native and WebAssembly hosts.
//!
//! The page shell is deliberately independent of DataZen's transport and storage
//! implementations. Platform adapters will be added behind typed UI ports.

mod data_grid;
mod sql_editor;

use data_grid::DataGrid;
use datazen_ui_api::{AdapterDelivery, AdapterJob, ColumnDraft, ColumnFilter, ColumnSort, ConnectionProfile, DESKTOP_NOT_IMPLEMENTED, DesktopAdapter, MemoryWorkspace, ProgressSnapshot, QueryApi, Setting, SettingsApi, TabKind, Tabs, TablePage, UnimplementedAdapter, WorkflowStepResult, WorkspaceApi};
// The desktop host's use-case adapter; the wasm host brings its own later.
#[cfg(not(target_arch = "wasm32"))]
use datazen_ui_api::UseCaseAdapter;
use rune::tok::{self, type_};
use rune::{App, AppSpec, Cx, El, Ime, Key, KeyEvent, TextEdit, Tx, col, oklch, row, text, text_input};
use sql_editor::SqlEditor;

/// The app's font pool (H8): rune's own Inter faces keep the UI's Latin look
/// and the web's sans fallback, IBM Plex Mono keeps the editor's mono slot,
/// and Noto Sans SC covers Chinese in the interface and the grid — the sans
/// slot lists it after Inter, so Han text falls back to it glyph by glyph.
/// Replaces rune's built-in set (AppSpec.fonts semantics) so every platform,
/// the web included, draws the same text.
const APP_FONTS: &[&[u8]] = &[
    include_bytes!("../assets/Inter-Regular.ttf"),
    include_bytes!("../assets/Inter-Medium.ttf"),
    include_bytes!("../assets/Inter-SemiBold.ttf"),
    include_bytes!("../assets/NotoSansSC-Regular.ttf"),
    include_bytes!("../assets/IBMPlexMono-Regular.ttf"),
];
const APP_FONT_FAMILIES: (&str, &str) = ("Inter, Noto Sans SC", "IBM Plex Mono");
pub const TABLE_PAGE_SIZE: u32 = 25;

/// What the workspace shows for the selected table tab.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum TableView {
    #[default]
    Data,
    Ddl,
    Columns,
    Indexes,
    ForeignKeys,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Section {
    #[default]
    Connections,
    Workspace,
    Settings,
    Tools,
    Dashboards,
    Workflows,
    Wapps,
}

impl Section {
    const ALL: [Self; 7] = [
        Self::Connections,
        Self::Workspace,
        Self::Settings,
        Self::Tools,
        Self::Dashboards,
        Self::Workflows,
        Self::Wapps,
    ];

    const fn key(self) -> &'static str {
        match self {
            Self::Connections => "connections",
            Self::Workspace => "workspace",
            Self::Settings => "settings",
            Self::Tools => "tools",
            Self::Dashboards => "dashboards",
            Self::Workflows => "workflows",
            Self::Wapps => "wapps",
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Connections => "Connections",
            Self::Workspace => "Workspace",
            Self::Settings => "Settings",
            Self::Tools => "Tools",
            Self::Dashboards => "Dashboards",
            Self::Workflows => "Workflows",
            Self::Wapps => "WAPPS",
        }
    }
}

/// Which data tool the tools page shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum Tool {
    #[default]
    Sync,
    Compare,
    Transfer,
}

struct AppState {
    active: Section,
    sql: SqlEditor,
    grid: DataGrid,
    workspace: MemoryWorkspace,
    /// The desktop host's seam. H3 wired list_connections to the connection
    /// use case; the remaining methods answer the fixed error until H4/H5.
    desktop: Box<dyn DesktopAdapter>,
    /// The adapter's connection list once it answers (H3); None while the
    /// page falls back to the memory port's list.
    desktop_connections: Option<Vec<ConnectionProfile>>,
    /// The adapter's last error, displayed on the connections page.
    desktop_error: Option<String>,
    tabs: Tabs,
    /// The page the workspace last opened for the selected table tab.
    table_page: u32,
    /// The sort the table page is opened with.
    sort: Option<ColumnSort>,
    /// The filter the table page is opened with.
    filter: Option<ColumnFilter>,
    /// The column the filter bar targets, as an index into the loaded page.
    filter_column: u32,
    filter_editor: TextEdit,
    /// The read-only query result grid of the selected query tab.
    result_grid: Option<DataGrid>,
    /// The query generation: bumped by every run and every cancel. A result page
    /// from a generation that is not the current one is dropped.
    query_generation: u32,
    /// The table read generation (N1): bumped by every read request. A job
    /// delivery from a generation that is not the current one is dropped.
    read_generation: u32,
    /// A read job the host accepted and has not answered yet (N1). While it
    /// is in flight the grid keeps its previous page and the footer says so.
    table_loading: Option<u32>,
    /// A commit job the host accepted and has not answered yet (N1). The
    /// dirty marks stay until the delivery lands.
    commit_loading: Option<u32>,
    /// A query job the host accepted and has not answered yet (N1).
    query_loading: Option<u32>,
    /// Catalog nodes the tree keeps collapsed, by path: "db" or "db/schema".
    collapsed: std::collections::BTreeSet<String>,
    /// Which side of the selected table tab the workspace shows.
    table_view: TableView,
    /// The ER diagram's open schema, as (database, schema).
    er_schema: Option<(String, String)>,
    /// ER box positions per table, canvas points. UI state only, never the port.
    er_positions: std::collections::BTreeMap<String, (f32, f32)>,
    /// The box being dragged: its table and where the gesture started.
    er_drag: Option<(String, (f32, f32))>,
    /// Column-change drafts per table, kept until applied. UI state.
    column_drafts: std::collections::BTreeMap<String, Vec<ColumnDraft>>,
    draft_name: TextEdit,
    draft_type: TextEdit,
    /// The settings page's inputs and its last rejected write.
    page_size_input: TextEdit,
    max_rows_input: TextEdit,
    settings_error: Option<String>,
    /// The connection form's fields and its last rejected save.
    connection_name: TextEdit,
    connection_engine: TextEdit,
    connection_address: TextEdit,
    connection_error: Option<String>,
    /// The snippet form's fields and its last rejected write.
    snippet_name: TextEdit,
    snippet_sql: TextEdit,
    snippet_error: Option<String>,
    /// The export dialog's format, range and its job.
    export_format_index: u32,
    export_range: TextEdit,
    export_job: Option<u64>,
    export_error: Option<String>,
    /// The export job's last snapshot. Refreshed only on demand.
    export_snapshot: Option<ProgressSnapshot>,
    /// The import wizard: 0 source step, 1 target step, 2 ready.
    import_step: u32,
    import_source: TextEdit,
    import_target: TextEdit,
    import_error: Option<String>,
    /// The backup job and its last snapshot.
    backup_job: Option<u64>,
    backup_snapshot: Option<ProgressSnapshot>,
    backup_error: Option<String>,
    /// The tools page's active tool.
    tool: Tool,
    /// The sync wizard: endpoints, tables, options, preview, run.
    sync_step: u32,
    sync_source: String,
    sync_target: String,
    sync_tables: std::collections::BTreeSet<String>,
    sync_options: [bool; 3],
    sync_error: Option<String>,
    /// The sync preview grid (read-only, the B8 sheet).
    sync_preview_grid: Option<DataGrid>,
    /// The sync run job and its last snapshot.
    sync_job: Option<u64>,
    sync_snapshot: Option<ProgressSnapshot>,
    /// The compare tool's endpoints.
    compare_source: String,
    compare_target: String,
    /// The compare plan's apply job and its last snapshot.
    compare_job: Option<u64>,
    compare_snapshot: Option<ProgressSnapshot>,
    compare_error: Option<String>,
    /// The transfer tool's tables, pairing chips and the pairs themselves.
    transfer_left: String,
    transfer_right: String,
    transfer_pair_left: String,
    transfer_pair_right: String,
    transfer_pairs: Vec<(String, String)>,
    transfer_error: Option<String>,
    /// The transfer's confirmation step and its job.
    transfer_confirming: bool,
    transfer_job: Option<u64>,
    transfer_snapshot: Option<ProgressSnapshot>,
    /// The widget form's fields and its last rejected save.
    widget_title: TextEdit,
    widget_query: TextEdit,
    widget_error: Option<String>,
    /// The workflow being edited: its name and the step list being reordered.
    workflow_editing: Option<(String, Vec<String>)>,
    /// The workflow's YAML, edited through the SQL editor's document model.
    workflow_yaml: SqlEditor,
    /// The last workflow run's results.
    workflow_results: Vec<WorkflowStepResult>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            active: Section::Connections,
            sql: SqlEditor::new(),
            grid: DataGrid::new(),
            workspace: MemoryWorkspace::new(),
            desktop: Box::new(UnimplementedAdapter),
            desktop_connections: None,
            desktop_error: None,
            tabs: Tabs::new(),
            table_page: 0,
            sort: None,
            filter: None,
            filter_column: 0,
            filter_editor: TextEdit::new(String::new()),
            result_grid: None,
            query_generation: 0,
            read_generation: 0,
            table_loading: None,
            commit_loading: None,
            query_loading: None,
            collapsed: std::collections::BTreeSet::new(),
            table_view: TableView::Data,
            er_schema: None,
            er_positions: std::collections::BTreeMap::new(),
            er_drag: None,
            column_drafts: std::collections::BTreeMap::new(),
            draft_name: TextEdit::new(String::new()),
            draft_type: TextEdit::new(String::new()),
            page_size_input: TextEdit::new(String::new()),
            max_rows_input: TextEdit::new(String::new()),
            settings_error: None,
            connection_name: TextEdit::new(String::new()),
            connection_engine: TextEdit::new(String::new()),
            connection_address: TextEdit::new(String::new()),
            connection_error: None,
            snippet_name: TextEdit::new(String::new()),
            snippet_sql: TextEdit::new(String::new()),
            snippet_error: None,
            export_format_index: 0,
            export_range: TextEdit::new(String::new()),
            export_job: None,
            export_error: None,
            export_snapshot: None,
            import_step: 0,
            import_source: TextEdit::new(String::new()),
            import_target: TextEdit::new(String::new()),
            import_error: None,
            backup_job: None,
            backup_snapshot: None,
            backup_error: None,
            tool: Tool::Sync,
            sync_step: 0,
            sync_source: "local".into(),
            sync_target: "replica".into(),
            sync_tables: std::collections::BTreeSet::new(),
            sync_options: [false, false, true],
            sync_error: None,
            sync_preview_grid: None,
            sync_job: None,
            sync_snapshot: None,
            compare_source: "local".into(),
            compare_target: "replica".into(),
            compare_job: None,
            compare_snapshot: None,
            compare_error: None,
            transfer_left: "customers".into(),
            transfer_right: "orders".into(),
            transfer_pair_left: "id".into(),
            transfer_pair_right: "id".into(),
            transfer_pairs: Vec::new(),
            transfer_error: None,
            transfer_confirming: false,
            transfer_job: None,
            transfer_snapshot: None,
            widget_title: TextEdit::new(String::new()),
            widget_query: TextEdit::new(String::new()),
            widget_error: None,
            workflow_editing: None,
            workflow_yaml: SqlEditor::new(),
            workflow_results: Vec::new(),
        }
    }
}

impl AppState {
    fn select(&mut self, section: Section) {
        self.active = section;
        if section == Section::Connections {
            self.sync_desktop_connections();
        }
    }

    /// Ask the desktop adapter for the connection list. A wired adapter's
    /// list replaces the memory port's demo list; an unwired one answers with
    /// the fixed error, which the connections page displays while the port's
    /// list stays.
    fn sync_desktop_connections(&mut self) {
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
    fn toggle_expand(&mut self, path: &str) {
        if !self.collapsed.insert(path.to_owned()) {
            self.collapsed.remove(path);
        }
    }

    /// Re-take the catalog from the port. Collapsed nodes stay collapsed: they
    /// live here, not in the port.
    fn refresh_catalog(&mut self) {
        let _ = self.workspace.catalog();
    }

    fn set_table_view(&mut self, view: TableView) {
        self.table_view = view;
    }

    /// The table page size from the settings port (fallback: the built-in 25).
    fn table_page_size(&self) -> u32 {
        self.workspace
            .read_settings()
            .iter()
            .find_map(|setting| match setting {
                Setting::TablePageSize { rows } => Some(*rows),
                _ => None,
            })
            .unwrap_or(TABLE_PAGE_SIZE)
    }

    /// Set the table page size from the settings input; an invalid value keeps
    /// the old one and shows the port's error.
    fn set_table_page_size(&mut self) {
        let rows = self.page_size_input.text.trim().parse::<u32>();
        match rows {
            Ok(rows) => match self.workspace.write_setting(Setting::TablePageSize { rows }) {
                Ok(()) => self.settings_error = None,
                Err(error) => self.settings_error = Some(error),
            },
            Err(_) => self.settings_error = Some("table page size must be a number".into()),
        }
    }

    fn set_query_max_rows(&mut self) {
        let rows = self.max_rows_input.text.trim().parse::<u32>();
        match rows {
            Ok(rows) => match self.workspace.write_setting(Setting::QueryMaxRows { rows }) {
                Ok(()) => self.settings_error = None,
                Err(error) => self.settings_error = Some(error),
            },
            Err(_) => self.settings_error = Some("query max rows must be a number".into()),
        }
    }

    /// Save the connection form through the port. A missing address is refused
    /// and nothing is saved.
    fn save_connection_form(&mut self) {
        let name = self.connection_name.text.clone();
        let engine = self.connection_engine.text.clone();
        let address = self.connection_address.text.clone();
        match self.workspace.save_connection(&name, &engine, &address) {
            Ok(()) => {
                self.connection_error = None;
                self.connection_name = TextEdit::new(String::new());
                self.connection_engine = TextEdit::new(String::new());
                self.connection_address = TextEdit::new(String::new());
            }
            Err(error) => self.connection_error = Some(error),
        }
    }

    fn save_snippet_form(&mut self) {
        let name = self.snippet_name.text.trim().to_owned();
        let sql = self.snippet_sql.text.trim().to_owned();
        match self.workspace.save_snippet(&name, &sql) {
            Ok(()) => {
                self.snippet_error = None;
                self.snippet_name = TextEdit::new(String::new());
                self.snippet_sql = TextEdit::new(String::new());
            }
            Err(error) => self.snippet_error = Some(error),
        }
    }

    fn update_snippet_form(&mut self) {
        let name = self.snippet_name.text.trim().to_owned();
        let sql = self.snippet_sql.text.trim().to_owned();
        match self.workspace.update_snippet(&name, &sql) {
            Ok(()) => {
                self.snippet_error = None;
                self.snippet_name = TextEdit::new(String::new());
                self.snippet_sql = TextEdit::new(String::new());
            }
            Err(error) => self.snippet_error = Some(error),
        }
    }

    fn delete_snippet(&mut self, name: &str) {
        if let Err(error) = self.workspace.delete_snippet(name) {
            self.snippet_error = Some(error);
        }
    }

    /// The export formats the dialog collects, in cycle order.
    const EXPORT_FORMATS: [&'static str; 3] = ["csv", "json", "sql"];

    fn export_format(&self) -> &'static str {
        Self::EXPORT_FORMATS[(self.export_format_index as usize) % Self::EXPORT_FORMATS.len()]
    }

    fn cycle_export_format(&mut self) {
        self.export_format_index =
            (self.export_format_index + 1) % Self::EXPORT_FORMATS.len() as u32;
    }

    /// Ask the port for an export job id. A missing range gets no job.
    fn start_export_form(&mut self) {
        let format = self.export_format().to_owned();
        let range = self.export_range.text.trim().to_owned();
        match self.workspace.start_export(&format, &range) {
            Ok(job) => {
                self.export_job = Some(job);
                self.export_error = None;
                self.export_snapshot = self.workspace.job_progress(job);
            }
            Err(error) => {
                self.export_job = None;
                self.export_error = Some(error);
            }
        }
    }

    /// Pull one progress snapshot from the port. No streaming, no polling.
    fn refresh_export_progress(&mut self) {
        if let Some(job) = self.export_job {
            self.export_snapshot = self.workspace.job_progress(job);
        }
    }

    /// The import wizard's step 1: a source is required to move on.
    fn import_next(&mut self) {
        if self.import_source.text.trim().is_empty() {
            self.import_error = Some("a source is required".into());
            return;
        }
        self.import_error = None;
        self.import_step = 1;
    }

    /// Step 2: a target table is required to finish.
    fn import_finish(&mut self) {
        if self.import_target.text.trim().is_empty() {
            self.import_error = Some("a target table is required".into());
            return;
        }
        self.import_error = None;
        self.import_step = 2;
    }

    fn import_back(&mut self) {
        self.import_step = 0;
        self.import_error = None;
    }

    fn start_backup_form(&mut self) {
        match self.workspace.start_backup() {
            Ok(job) => {
                self.backup_job = Some(job);
                self.backup_error = None;
                self.backup_snapshot = self.workspace.job_progress(job);
            }
            Err(error) => self.backup_error = Some(error),
        }
    }

    /// Stop the backup and pull the stopped snapshot.
    fn stop_backup(&mut self) {
        let Some(job) = self.backup_job else { return };
        match self.workspace.stop_job(job) {
            Ok(()) => {
                self.backup_error = None;
                self.backup_snapshot = self.workspace.job_progress(job);
            }
            Err(error) => self.backup_error = Some(error),
        }
    }

    fn refresh_backup_progress(&mut self) {
        if let Some(job) = self.backup_job {
            self.backup_snapshot = self.workspace.job_progress(job);
        }
    }

    fn set_tool(&mut self, tool: Tool) {
        self.tool = tool;
    }

    fn cycle_compare_source(&mut self) {
        self.compare_source = next_connection(&self.workspace, &self.compare_source);
    }

    fn cycle_compare_target(&mut self) {
        self.compare_target = next_connection(&self.workspace, &self.compare_target);
    }

    /// D8: apply the plan through the port. A failure keeps the plan (the
    /// difference is still there), and so does the error.
    fn apply_compare_plan(&mut self) {
        let plan = match self.workspace.compare_plan(&self.compare_source, &self.compare_target) {
            Ok(plan) => plan,
            Err(error) => {
                self.compare_error = Some(error);
                return;
            }
        };
        match self.workspace.run_compare_plan(&plan) {
            Ok(job) => {
                self.compare_job = Some(job);
                self.compare_snapshot = self.workspace.job_progress(job);
                self.compare_error = None;
            }
            Err(error) => {
                self.compare_job = None;
                self.compare_error = Some(error);
            }
        }
    }

    fn refresh_compare_progress(&mut self) {
        if let Some(job) = self.compare_job {
            self.compare_snapshot = self.workspace.job_progress(job);
        }
    }

    /// The columns of a table (its ColumnSpec names).
    fn table_columns_of(&self, table: &str) -> Vec<String> {
        self.workspace
            .table_columns(table)
            .unwrap_or_default()
            .into_iter()
            .map(|column| column.name)
            .collect()
    }

    fn cycle_transfer_table(&mut self, side: bool) {
        let tables = self.workspace.connection_tables("local");
        let current = if side { &self.transfer_left } else { &self.transfer_right };
        let next = match tables.iter().position(|name| name == current) {
            Some(index) => tables[(index + 1) % tables.len()].clone(),
            None => tables.first().cloned().unwrap_or_default(),
        };
        if side {
            self.transfer_left = next;
        } else {
            self.transfer_right = next;
        }
        self.transfer_pair_left = self.table_columns_of(&self.transfer_left).first().cloned().unwrap_or_default();
        self.transfer_pair_right = self.table_columns_of(&self.transfer_right).first().cloned().unwrap_or_default();
    }

    fn cycle_transfer_pair_column(&mut self, side: bool) {
        let (table, current) = if side {
            (&self.transfer_left, self.transfer_pair_left.clone())
        } else {
            (&self.transfer_right, self.transfer_pair_right.clone())
        };
        let columns = self.table_columns_of(table);
        let next = match columns.iter().position(|name| *name == current) {
            Some(index) => columns[(index + 1) % columns.len()].clone(),
            None => columns.first().cloned().unwrap_or_default(),
        };
        if side {
            self.transfer_pair_left = next;
        } else {
            self.transfer_pair_right = next;
        }
    }

    /// Pair the two picked columns. A column that already appears in a pair —
    /// a duplicate name — cannot pair again.
    fn pair_transfer_columns(&mut self) {
        let left = self.transfer_pair_left.clone();
        let right = self.transfer_pair_right.clone();
        if left.is_empty() || right.is_empty() {
            self.transfer_error = Some("pick a column on both ends".into());
            return;
        }
        if self.transfer_pairs.iter().any(|(l, r)| *l == left || *r == right) {
            self.transfer_error =
                Some(format!("{left} or {right} is already paired — a column pairs once"));
            return;
        }
        self.transfer_pairs.push((left, right));
        self.transfer_error = None;
    }

    /// Open the confirmation row. Cancelling it leaves no job.
    fn begin_transfer_confirm(&mut self) {
        self.transfer_confirming = true;
    }

    fn cancel_transfer(&mut self) {
        self.transfer_confirming = false;
    }

    fn confirm_transfer(&mut self) {
        self.transfer_confirming = false;
        let pairs = self.transfer_pairs.clone();
        match self.workspace.run_transfer(&pairs) {
            Ok(job) => {
                self.transfer_job = Some(job);
                self.transfer_snapshot = self.workspace.job_progress(job);
                self.transfer_error = None;
            }
            Err(error) => {
                self.transfer_job = None;
                self.transfer_error = Some(error);
            }
        }
    }

    fn refresh_transfer_progress(&mut self) {
        if let Some(job) = self.transfer_job {
            self.transfer_snapshot = self.workspace.job_progress(job);
        }
    }

    fn edit_workflow(&mut self, name: &str) {
        if let Some(flow) = self.workspace.workflows().into_iter().find(|flow| flow.name == name) {
            self.workflow_yaml = SqlEditor::from_doc(workflow_yaml(&flow.name, &flow.steps));
            self.workflow_editing = Some((flow.name, flow.steps));
        }
    }

    /// Move a step up or down. The first step cannot move up, the last cannot
    /// move down; everything else swaps with its neighbour.
    fn move_workflow_step(&mut self, index: usize, up: bool) {
        let Some((_, steps)) = &mut self.workflow_editing else { return };
        let target = if up { index.checked_sub(1) } else { Some(index + 1) };
        match target {
            Some(target) if target < steps.len() => steps.swap(index, target),
            _ => {}
        }
    }

    fn save_workflow_form(&mut self) {
        let Some((name, steps)) = self.workflow_editing.clone() else { return };
        if self.workspace.save_workflow(&name, &steps).is_ok() {
            self.workflow_editing = None;
        }
    }

    fn close_workflow_editor(&mut self) {
        self.workflow_editing = None;
        self.workflow_results.clear();
    }

    /// W7: run the edited workflow; the results list below is read-only.
    fn run_edited_workflow(&mut self) {
        let Some((name, _)) = self.workflow_editing.clone() else { return };
        match self.workspace.run_workflow(&name) {
            Ok(results) => self.workflow_results = results,
            Err(_) => self.workflow_results.clear(),
        }
    }

    /// Save the widget form through the port. An empty title is refused.
    fn save_widget_form(&mut self) {
        let title = self.widget_title.text.trim().to_owned();
        let query = self.widget_query.text.trim().to_owned();
        match self.workspace.save_dashboard_widget(&title, &query) {
            Ok(_) => {
                self.widget_error = None;
                self.widget_title = TextEdit::new(String::new());
                self.widget_query = TextEdit::new(String::new());
            }
            Err(error) => self.widget_error = Some(error),
        }
    }

    /// The left table's columns not yet paired.
    fn transfer_unmapped(&self, side: bool) -> Vec<String> {
        let (table, used_left): (String, Vec<String>) = if side {
            (self.transfer_left.clone(), self.transfer_pairs.iter().map(|(l, _)| l.clone()).collect())
        } else {
            (self.transfer_right.clone(), self.transfer_pairs.iter().map(|(_, r)| r.clone()).collect())
        };
        self.table_columns_of(&table)
            .into_iter()
            .filter(|column| !used_left.contains(column))
            .collect()
    }

    /// Cycle the sync source through the connections.
    fn cycle_sync_source(&mut self) {
        self.sync_source = next_connection(&self.workspace, &self.sync_source);
    }

    fn cycle_sync_target(&mut self) {
        self.sync_target = next_connection(&self.workspace, &self.sync_target);
    }

    fn toggle_sync_table(&mut self, table: &str) {
        if !self.sync_tables.insert(table.to_owned()) {
            self.sync_tables.remove(table);
        }
    }

    fn sync_select_all(&mut self) {
        self.sync_tables = self
            .workspace
            .connection_tables(&self.sync_source)
            .into_iter()
            .collect();
    }

    fn sync_clear_tables(&mut self) {
        self.sync_tables.clear();
    }

    /// Step 2 done: at least one table must be picked.
    fn sync_next_tables(&mut self) {
        if self.sync_tables.is_empty() {
            self.sync_error = Some("pick at least one table".into());
            return;
        }
        self.sync_error = None;
        self.sync_step = 2;
    }

    /// The sync options' labels, in order.
    const SYNC_OPTION_LABELS: [&'static str; 3] =
        ["Create missing tables", "Drop extra tables", "Copy data"];

    fn toggle_sync_option(&mut self, index: usize) {
        if let Some(option) = self.sync_options.get_mut(index) {
            *option = !*option;
        }
    }

    /// D5: run the sync. No selected tables, no job.
    fn sync_run(&mut self) {
        let source = self.sync_source.clone();
        let target = self.sync_target.clone();
        let tables: Vec<String> = self.sync_tables.iter().cloned().collect();
        match self.workspace.run_sync(&source, &target, &tables) {
            Ok(job) => {
                self.sync_job = Some(job);
                self.sync_snapshot = self.workspace.job_progress(job);
                self.sync_error = None;
            }
            Err(error) => {
                self.sync_job = None;
                self.sync_error = Some(error);
            }
        }
    }

    fn refresh_sync_progress(&mut self) {
        if let Some(job) = self.sync_job {
            self.sync_snapshot = self.workspace.job_progress(job);
        }
    }

    /// Step 3 done: build the read-only preview of the first picked table.
    fn sync_show_preview(&mut self) {
        let Some(table) = self.sync_tables.iter().next().cloned() else {
            self.sync_error = Some("pick at least one table".into());
            return;
        };
        let source = self.sync_source.clone();
        match self.workspace.sync_preview(&source, &table) {
            Ok(page) => {
                let mut grid = DataGrid::new();
                grid.load_page(&page);
                grid.set_editable(false);
                self.sync_preview_grid = Some(grid);
                self.sync_error = None;
                self.sync_step = 3;
            }
            Err(error) => self.sync_error = Some(error),
        }
    }

    /// Step 1 done: the port validates the two endpoints.
    fn sync_next_endpoints(&mut self) {
        let source = self.sync_source.clone();
        let target = self.sync_target.clone();
        match self.workspace.sync_endpoints(&source, &target) {
            Ok(()) => {
                self.sync_error = None;
                self.sync_step = 1;
            }
            Err(error) => self.sync_error = Some(error),
        }
    }

    /// Flip the theme. The only write goes to the settings port.
    fn toggle_theme(&mut self) {
        let dark = self
            .workspace
            .read_settings()
            .iter()
            .find_map(|setting| match setting {
                Setting::Theme { dark } => Some(*dark),
                _ => None,
            })
            .unwrap_or(true);
        let _ = self.workspace.write_setting(Setting::Theme { dark: !dark });
    }

    /// Open the ER diagram for the current schema: the selected table's schema,
    /// else the first.
    fn open_er(&mut self) {
        let catalog = self.workspace.catalog();
        let Some(database) = catalog.databases.first() else {
            return;
        };
        let selected = self.tabs.selected().map(|tab| tab.title.clone());
        let schema = selected
            .and_then(|table| {
                database
                    .schemas
                    .iter()
                    .find(|schema| schema.tables.contains(&table))
                    .map(|schema| schema.name.clone())
            })
            .or_else(|| database.schemas.first().map(|schema| schema.name.clone()));
        if let Some(schema) = schema {
            self.er_schema = Some((database.name.clone(), schema));
            self.active = Section::Workspace;
        }
    }

    fn close_er(&mut self) {
        self.er_schema = None;
    }

    /// The selected table tab's title, if any.
    fn selected_table(&self) -> Option<String> {
        let tab = self.tabs.selected()?;
        (tab.kind == TabKind::Table).then(|| tab.title.clone())
    }

    /// Queue an add-column draft from the draft inputs.
    fn add_column_draft(&mut self) {
        let Some(table) = self.selected_table() else { return };
        let name = self.draft_name.text.trim().to_owned();
        if name.is_empty() {
            return;
        }
        let sql_type = if self.draft_type.text.trim().is_empty() {
            "text".to_owned()
        } else {
            self.draft_type.text.trim().to_owned()
        };
        self.column_drafts.entry(table).or_default().push(ColumnDraft::AddColumn { name, sql_type });
        self.draft_name = TextEdit::new(String::new());
        self.draft_type = TextEdit::new(String::new());
    }

    /// Apply the table's drafts through the port. A failure keeps the drafts.
    fn apply_column_drafts(&mut self) {
        let Some(table) = self.selected_table() else { return };
        let Some(drafts) = self.column_drafts.get(&table) else { return };
        if drafts.is_empty() {
            return;
        }
        let sql = drafts.iter().map(|draft| draft.sql(&table)).collect::<Vec<_>>().join("\n");
        if self.workspace.apply_ddl(&table, &sql).is_ok() {
            self.column_drafts.remove(&table);
        }
    }

    /// Queue a retype draft for the column named in the draft inputs.
    fn add_retype_draft(&mut self) {
        let Some(table) = self.selected_table() else { return };
        let column = self.draft_name.text.trim().to_owned();
        if column.is_empty() {
            return;
        }
        let sql_type = if self.draft_type.text.trim().is_empty() {
            "text".to_owned()
        } else {
            self.draft_type.text.trim().to_owned()
        };
        self.column_drafts
            .entry(table)
            .or_default()
            .push(ColumnDraft::ChangeType { column, sql_type });
        self.draft_name = TextEdit::new(String::new());
        self.draft_type = TextEdit::new(String::new());
    }

    fn open_connection(&mut self, id: &str) {
        if self.workspace.connect(id).is_err() {
            return;
        }
        self.collapsed.clear();
        self.table_page = 0;
        self.sort = None;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        // H4: the initial page goes through the same read path as every
        // other one: the adapter's use case first, the port as the fallback.
        self.show_table("customers");
    }

    fn disconnect(&mut self) {
        self.workspace.disconnect();
        self.tabs = Tabs::new();
        self.grid.set_editable(false);
        self.table_page = 0;
        self.sort = None;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        self.result_grid = None;
        self.collapsed.clear();
        self.er_schema = None;
        self.er_positions.clear();
        self.er_drag = None;
        // N1: a read the disconnect invalidates must not land later, and no
        // loading marker outlives the session it belonged to.
        self.next_read_generation();
        self.table_loading = None;
        self.commit_loading = None;
        self.query_loading = None;
    }

    fn open_table_tab(&mut self, title: &str) -> Option<u32> {
        let id = self.tabs.open(TabKind::Table, title)?;
        self.sync_grid_to_selected_tab();
        Some(id)
    }

    fn open_query_tab(&mut self, title: &str) -> Option<u32> {
        let id = self.tabs.open(TabKind::Query, title)?;
        self.sync_grid_to_selected_tab();
        Some(id)
    }

    fn select_tab(&mut self, id: u32) -> bool {
        if !self.tabs.select(id) {
            return false;
        }
        self.sync_grid_to_selected_tab();
        true
    }

    fn show_table(&mut self, title: &str) -> bool {
        // N1: a live host takes the read as a job and answers on its own
        // schedule. Until the delivery lands the grid keeps its previous
        // page and the footer says the read is in flight; a declined job
        // falls through to the synchronous paths below (H4's contract).
        let generation = self.next_read_generation();
        let job = AdapterJob::ReadTable {
            generation,
            table: title.to_owned(),
            page: self.table_page,
            page_size: self.table_page_size(),
            sort: self.sort.clone(),
            filter: self.filter.clone(),
        };
        if self.desktop.begin_job(job) {
            self.table_loading = Some(generation);
            self.active = Section::Workspace;
            return true;
        }
        // H4: the desktop adapter's read use case first; the port's memory
        // use case answers while the adapter is unwired. When both fail the
        // grid keeps its previous page.
        let page = self.table_page;
        let size = self.table_page_size();
        let sort = self.sort.clone();
        let filter = self.filter.clone();
        let read = self
            .desktop
            .read_table_page(title, page, size, sort.as_ref(), filter.as_ref())
            .or_else(|_| {
                self.workspace
                    .open_table(title, page, size, sort.as_ref(), filter.as_ref())
            });
        match read {
            Ok(page) => {
                self.table_page = page.page;
                self.grid.load_page(&page);
                self.grid.set_editable(page.read_only.is_none());
                self.active = Section::Workspace;
                true
            }
            Err(_) => false,
        }
    }

    /// Reopen the selected table tab at the current page, sort and filter.
    fn reload_table_page(&mut self) {
        if let Some(tab) = self.tabs.selected() {
            if tab.kind == TabKind::Table {
                self.show_table(&tab.title.clone());
            }
        }
    }

    fn next_table_page(&mut self) {
        if !self.grid.has_page() || self.grid.page() + 1 >= self.grid.pages() {
            return;
        }
        self.table_page += 1;
        self.reload_table_page();
    }

    fn prev_table_page(&mut self) {
        if !self.grid.has_page() || self.grid.page() == 0 {
            return;
        }
        self.table_page = self.table_page.saturating_sub(1);
        self.reload_table_page();
    }

    /// A header click cycles its column: none, ascending, descending, back to none.
    fn cycle_sort(&mut self, column: u32) {
        let Some(name) = self.grid.column_name(column).map(str::to_owned) else {
            return;
        };
        let next = match &self.sort {
            Some(sort) if sort.column == name && !sort.descending => {
                Some(ColumnSort { column: name, descending: true })
            }
            Some(sort) if sort.column == name => None,
            _ => Some(ColumnSort { column: name, descending: false }),
        };
        self.sort = next;
        self.table_page = 0;
        self.reload_table_page();
    }

    /// The filter bar's Enter: read the editor, target the chosen column, return to page 1.
    fn apply_filter(&mut self) {
        let column = self.grid.column_name(self.filter_column).map(str::to_owned);
        let text = self.filter_editor.text.trim().to_owned();
        self.filter = match (column, text.is_empty()) {
            (Some(column), false) => Some(ColumnFilter { column, contains: text }),
            _ => None,
        };
        self.table_page = 0;
        self.reload_table_page();
    }

    fn cycle_filter_column(&mut self) {
        if self.grid.cols == 0 {
            return;
        }
        self.filter_column = (self.filter_column + 1) % self.grid.cols;
        if self.filter.is_some() {
            self.apply_filter();
        }
    }

    /// Commit every staged edit to the stored rows, then reload the page.
    fn commit_staged_edits(&mut self) {
        // N1: a live host takes the batch as a job. The dirty marks stay
        // until the delivery lands — the job twin of H5's rule that a real
        // failure keeps them.
        let generation = self.read_generation;
        let table = self.grid.table_label().to_owned();
        let edits = self.grid.staged_edits();
        let job = AdapterJob::CommitCells { generation, table, edits };
        if self.desktop.begin_job(job) {
            self.commit_loading = Some(generation);
            return;
        }
        // H5: the desktop adapter's commit use case first — the grid's staged
        // edits travel as one batch. The fixed not-implemented error means
        // the host has not wired the use case yet, and the port's own staged
        // edits answer instead; any other failure is real and keeps the
        // dirty marks (no reload, so the grid keeps its staged state).
        let table = self.grid.table_label().to_owned();
        let edits = self.grid.staged_edits();
        match self.desktop.commit_cells(&table, &edits) {
            Ok(_) => self.reload_table_page(),
            Err(error) if error == DESKTOP_NOT_IMPLEMENTED => {
                if self.workspace.commit_staged().is_ok() {
                    self.reload_table_page();
                }
            }
            Err(_) => {}
        }
    }

    /// Drop every staged edit and reload the page back to the stored values.
    fn rollback_staged_edits(&mut self) {
        self.workspace.rollback_staged();
        self.reload_table_page();
    }

    /// Load a query result into the shared grid, read-only, below the editor.
    fn load_result_page(&mut self, columns: Vec<String>, rows: Vec<Vec<String>>) {
        let page = self.workspace.open_result(columns, rows);
        let mut grid = DataGrid::new();
        grid.load_page(&page);
        grid.set_editable(false);
        self.result_grid = Some(grid);
    }

    /// Bump and return the next query generation. Every run and every cancel
    /// starts one.
    fn next_query_generation(&mut self) -> u32 {
        self.query_generation += 1;
        self.query_generation
    }

    /// A result page arrives for a generation. Stale generations — superseded by
    /// a newer run or invalidated by a cancel — are dropped on the floor.
    fn deliver_query_result(&mut self, generation: u32, page: TablePage) {
        if generation != self.query_generation {
            return;
        }
        self.load_result_page(page.columns, page.rows);
    }

    /// Cancel the in-flight query: bumping the generation invalidates every
    /// outstanding one, so their results are dropped when they arrive. A
    /// query job in flight also stops waiting: its marker leaves with the
    /// generation, and a late delivery finds only the stale check.
    fn cancel_query(&mut self) {
        self.next_query_generation();
        self.query_loading = None;
    }

    /// Run the statement at the cursor. A failed execution changes nothing; a
    /// successful one lands in the result grid only if still current.
    fn run_query(&mut self) {
        let generation = self.next_query_generation();
        let Some(sql) = self.sql.statement_at_cursor() else {
            return;
        };
        // N1: a live host runs the statement as a job; the editor keeps its
        // document and the result area says the query is in flight until the
        // delivery lands (dropped when its generation went stale, as in Q5).
        let job = AdapterJob::RunQuery { generation, sql: sql.clone() };
        if self.desktop.begin_job(job) {
            self.query_loading = Some(generation);
            return;
        }
        match self.workspace.execute(&sql) {
            Ok(page) => self.deliver_query_result(generation, page),
            Err(_) => {}
        }
    }

    /// Bump and return the next table read generation. Every read request —
    /// a fresh open, a page turn, a reload — starts one, so a delivery from
    /// a read the state has moved past is dropped (N1, plan invariant 3).
    fn next_read_generation(&mut self) -> u32 {
        self.read_generation += 1;
        self.read_generation
    }

    /// A host job's answer arrived on the poll path. The loading marker of
    /// the finished request always clears; the state itself is written only
    /// when the delivery's generation is still the current one.
    fn deliver_host_job(&mut self, delivery: AdapterDelivery) {
        match delivery {
            AdapterDelivery::TablePage { generation, page } => {
                if self.table_loading == Some(generation) {
                    self.table_loading = None;
                }
                if generation != self.read_generation {
                    return;
                }
                if let Ok(page) = page {
                    self.table_page = page.page;
                    self.grid.load_page(&page);
                    self.grid.set_editable(page.read_only.is_none());
                }
            }
            AdapterDelivery::CommitCount { generation, count } => {
                if self.commit_loading == Some(generation) {
                    self.commit_loading = None;
                }
                if generation != self.read_generation || count.is_err() {
                    return;
                }
                self.reload_table_page();
            }
            AdapterDelivery::QueryPage { generation, page } => {
                if self.query_loading == Some(generation) {
                    self.query_loading = None;
                }
                if let Ok(page) = page {
                    self.deliver_query_result(generation, page);
                }
            }
        }
    }

    /// Any tab event lands in the workspace: the selected tab decides what the
    /// workspace shows, so the tab strip and its content stay on one page.
    fn sync_grid_to_selected_tab(&mut self) {
        self.active = Section::Workspace;
        self.er_schema = None;
        self.table_page = 0;
        self.filter = None;
        self.filter_column = 0;
        self.filter_editor = TextEdit::new(String::new());
        let selected = self.tabs.selected().map(|tab| (tab.kind, tab.title.clone()));
        match selected {
            Some((TabKind::Table, title)) => {
                self.show_table(&title);
            }
            Some((TabKind::Query, _)) | None => {}
        }
    }

    fn close_tab(&mut self, id: u32) -> bool {
        let previous = self.tabs.selected().map(|tab| tab.id);
        if !self.tabs.close(id) {
            return false;
        }
        if self.tabs.selected().map(|tab| tab.id) != previous {
            self.sync_grid_to_selected_tab();
        }
        true
    }

    fn commit_grid_edit(&mut self) {
        if !self.grid.is_editable() {
            return;
        }
        let (row, col) = self.grid.focus();
        let column = self.grid.column_name(col).map(str::to_owned);
        self.grid.commit_edit();
        if let Some(column) = column {
            let value = self.grid.cell(row, col);
            let _ = self.workspace.stage_cell(row, &column, value);
        }
    }
}

fn navigation_item(state: &AppState, section: Section, dark: bool) -> El {
    let theme = tok::theme(dark);
    let selected = state.active == section;

    row()
        .key(format!("nav-{}", section.key()))
        .w_full()
        .h(36.0)
        .px(12.0)
        .center_y()
        .radius(7.0)
        .fill(if selected {
            tok::accent(0, dark)
        } else {
            theme.control_off
        })
        .child(
            text(section.label())
                .font(type_::LABEL)
                .color(theme.text),
        )
        .role("tab")
        .label(section.label())
        .on_press(move |state: &mut AppState| state.select(section))
}
/// The connection after `current` in the port's list, wrapping around.
/// A workflow as YAML text.
fn workflow_yaml(name: &str, steps: &[String]) -> String {
    let mut out = format!("name: {name}\nsteps:\n");
    for step in steps {
        out.push_str(&format!("  - {step}\n"));
    }
    out
}

fn next_connection(workspace: &MemoryWorkspace, current: &str) -> String {
    let ids: Vec<String> = workspace.connections().iter().map(|profile| profile.id.clone()).collect();
    match ids.iter().position(|id| id == current) {
        Some(index) => ids[(index + 1) % ids.len()].clone(),
        None => ids.first().cloned().unwrap_or_else(|| current.to_owned()),
    }
}

fn query_selected(state: &AppState) -> bool {
    state.tabs.selected().is_some_and(|tab| tab.kind == TabKind::Query)
}

fn page_body(state: &AppState, theme: tok::Theme) -> El {
    match state.active {
        Section::Connections => connection_list(state, theme),
        Section::Workspace => workspace_body(state, theme),
        Section::Settings => settings_page(state, theme),
        Section::Tools => tools_page(state, theme),
        Section::Dashboards => dashboards_page(state, theme),
        Section::Workflows => workflows_page(state, theme),
        Section::Wapps => wapps_page(theme),
    }
}

/// The areas the migration has not reached. Each says so, and says where
/// the product still serves it: the React interface, which stays.
fn wapps_page(theme: tok::Theme) -> El {
    let entry = |key: &'static str, title: &'static str, body: &'static str| {
        col()
            .key(key)
            .w_full()
            .gap(6.0)
            .px(12.0)
            .py(10.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(text(title.to_owned()).font(type_::TITLE).color(theme.text))
            .child(
                text(body.to_owned())
                    .font(type_::LABEL)
                    .color(theme.text_secondary)
                    .w_full(),
            )
    };
    col()
        .key("wapps-page")
        .w_full()
        .gap(8.0)
        .child(
            text("Not yet migrated".to_owned())
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            text("These areas are not yet migrated to Rune. The React pages are still there — use the React interface for them.")
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .w_full(),
        )
        .child(entry(
            "wapps-pending",
            "WAPPS management",
            "Not yet migrated. The React page is still there: install and manage WAPPS in the React interface. WAPP iframes do not go into Rune.",
        ))
        .child(entry(
            "onboarding-pending",
            "Onboarding wizard",
            "Not yet migrated. The React wizard is still there: first-run onboarding runs in the React interface.",
        ))
}

/// The workflows page: the list, sorted by name, and the step editor.
fn workflows_page(state: &AppState, theme: tok::Theme) -> El {
    let workflows = state.workspace.workflows();
    let mut page = col()
        .key("workflows-page")
        .w_full()
        .gap(6.0)
        .child(text("Workflows".to_owned()).font(type_::TITLE).color(theme.text));
    for workflow in &workflows {
        let name = workflow.name.clone();
        page = page.child(
            row()
                .key(format!("workflow-{}", workflow.name))
                .w_full()
                .h(28.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!("{} · {} steps", workflow.name, workflow.steps.len()))
                        .font(type_::LABEL)
                        .color(theme.text),
                )
                .on_press(move |state: &mut AppState| state.edit_workflow(&name)),
        );
    }
    if let Some((name, steps)) = &state.workflow_editing {
        let mut editor = col()
            .key("workflow-form")
            .w_full()
            .gap(4.0)
            .px(8.0)
            .py(6.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(
                row()
                    .key("workflow-form-head")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(text(name.clone()).font(type_::LABEL).color(theme.text))
                    .child(
                        row()
                            .key("workflow-save")
                            .h(20.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(text("Save").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.save_workflow_form()),
                    )
                    .child(
                        row()
                            .key("workflow-run")
                            .h(20.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(text("Run").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.run_edited_workflow()),
                    )
                    .child(
                        row()
                            .key("workflow-cancel")
                            .h(20.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(text("Close".to_owned()).font(type_::LABEL).color(theme.text_secondary))
                            .on_press(|state: &mut AppState| state.close_workflow_editor()),
                    ),
            );
        for (index, step) in steps.iter().enumerate() {
            editor = editor.child(
                row()
                    .key(format!("step-row-{index}"))
                    .w_full()
                    .h(24.0)
                    .gap(6.0)
                    .center_y()
                    .child(text(step.clone()).font(type_::LABEL).color(theme.text_secondary))
                    .child(
                        row()
                            .key(format!("step-up-{index}"))
                            .h(18.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(text("up".to_owned()).font(type_::LABEL).color(theme.text))
                            .on_press(move |state: &mut AppState| state.move_workflow_step(index, true)),
                    )
                    .child(
                        row()
                            .key(format!("step-down-{index}"))
                            .h(18.0)
                            .px(6.0)
                            .center_y()
                            .radius(4.0)
                            .fill(theme.control_off)
                            .child(text("down".to_owned()).font(type_::LABEL).color(theme.text))
                            .on_press(move |state: &mut AppState| state.move_workflow_step(index, false)),
                    ),
            );
        }
        page = page.child(editor);
        page = page.child(sql_editor::plain_panel(
            &state.workflow_yaml,
            theme.text,
            theme.text_secondary,
        ));
        if !state.workflow_results.is_empty() {
            let mut results = col()
                .key("run-results")
                .w_full()
                .gap(4.0)
                .px(8.0)
                .py(6.0)
                .radius(8.0)
                .fill(theme.control_off);
            for (index, result) in state.workflow_results.iter().enumerate() {
                results = results.child(
                    row()
                        .key(format!("run-result-{index}"))
                        .w_full()
                        .h(24.0)
                        .gap(8.0)
                        .center_y()
                        .child(
                            row()
                                .key(format!("run-{}-{index}", if result.ok { "ok" } else { "fail" }))
                                .w(10.0)
                                .h(10.0)
                                .radius(5.0)
                                .fill(if result.ok {
                                    oklch(0.75, 0.12, 145.0)
                                } else {
                                    oklch(0.7, 0.14, 25.0)
                                }),
                        )
                        .child(
                            text(format!(
                                "{} · {} — {}",
                                result.step,
                                if result.ok { "ok" } else { "failed" },
                                result.summary
                            ))
                            .font(type_::LABEL)
                            .color(theme.text),
                        ),
                );
            }
            page = page.child(results);
        }
    }
    page
}

/// The dashboards page: the widget list. An empty list has its own state.
fn dashboards_page(state: &AppState, theme: tok::Theme) -> El {
    let widgets = state.workspace.dashboard_widgets();
    let mut page = col()
        .key("dashboards-page")
        .w_full()
        .gap(8.0)
        .child(text("Dashboards".to_owned()).font(type_::TITLE).color(theme.text))
        .child(
            row()
                .key("widget-form")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text_input(&state.widget_title, type_::LABEL)
                        .key("widget-title-input")
                        .w(160.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.widget_title),
                )
                .child(
                    text_input(&state.widget_query, type_::LABEL)
                        .key("widget-query-input")
                        .w(260.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.widget_query),
                )
                .child(
                    row()
                        .key("widget-save")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Save widget").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.save_widget_form()),
                ),
        );
    if let Some(error) = &state.widget_error {
        page = page.child(
            row()
                .key("widget-form-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if widgets.is_empty() {
        return page.child(
            col()
                .key("dashboard-empty")
                .w_full()
                .h(120.0)
                .px(12.0)
                .py(8.0)
                .radius(8.0)
                .fill(theme.control_off)
                .child(text("No widgets yet".to_owned()).font(type_::TITLE).color(theme.text))
                .child(
                    text("The widget form arrives with the next task")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    for widget in &widgets {
        page = page.child(
            col()
                .key(format!("widget-{}", widget.id))
                .w_full()
                .gap(4.0)
                .px(8.0)
                .py(6.0)
                .radius(8.0)
                .fill(theme.control_off)
                .child(
                    row()
                        .key(format!("widget-head-{}", widget.id))
                        .w_full()
                        .h(24.0)
                        .center_y()
                        .child(
                            text(format!("#{} · {} · {}", widget.id, widget.title, widget.kind))
                                .font(type_::LABEL)
                                .color(theme.text),
                        ),
                )
                .child(bar_chart(state, widget.id, theme)),
        );
    }
    page
}

/// A bar chart drawn with color blocks — the port's integers, no chart library.
/// The zero bar keeps a visible sliver and its label.
fn bar_chart(state: &AppState, id: u64, theme: tok::Theme) -> El {
    let bars = state.workspace.widget_bars(id).unwrap_or_default();
    let max = bars.iter().map(|(_, value)| *value).max().unwrap_or(1).max(1);
    let mut chart = row()
        .key(format!("widget-chart-{id}"))
        .w_full()
        .h(120.0)
        .gap(10.0);
    for (label, value) in &bars {
        let height = if *value == 0 {
            2.0
        } else {
            12.0 + (*value as f32 / max as f32) * 84.0
        };
        chart = chart.child(
            col()
                .key(format!("widget-bar-{id}-{label}"))
                .w(46.0)
                .gap(4.0)
                .child(
                    row()
                        .key(format!("widget-block-{id}-{label}"))
                        .w(30.0)
                        .h(height)
                        .radius(3.0)
                        .fill(oklch(0.72, 0.13, 250.0)),
                )
                .child(
                    text(format!("{label} {value}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    chart
}

/// The tools page: one switcher, three tools. Sync, compare, transfer.
fn tools_page(state: &AppState, theme: tok::Theme) -> El {
    let entry = |key: &str, label: &str, tool: Tool, active: bool| {
        row()
            .key(key)
            .h(24.0)
            .px(10.0)
            .center_y()
            .radius(6.0)
            .fill(if active { tok::accent(0, true) } else { theme.control_off })
            .child(text(label).font(type_::LABEL).color(theme.text))
            .on_press(move |state: &mut AppState| state.set_tool(tool))
    };
    col()
        .key("tools-page")
        .w_full()
        .gap(8.0)
        .child(
            row()
                .key("tool-switcher")
                .w_full()
                .h(28.0)
                .gap(6.0)
                .center_y()
                .child(text("Data tools".to_owned()).font(type_::TITLE).color(theme.text))
                .child(entry("tool-sync", "Sync", Tool::Sync, state.tool == Tool::Sync))
                .child(entry("tool-compare", "Compare", Tool::Compare, state.tool == Tool::Compare))
                .child(entry("tool-transfer", "Transfer", Tool::Transfer, state.tool == Tool::Transfer)),
        )
        .child(match state.tool {
            Tool::Sync => sync_tool(state, theme),
            Tool::Compare => compare_tool(state, theme),
            Tool::Transfer => transfer_tool(state, theme),
        })
}

/// The transfer tool: pair the two tables' columns; unmapped columns are
/// listed explicitly.
fn transfer_tool(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("transfer-tool")
        .w_full()
        .gap(6.0)
        .child(
            row()
                .key("transfer-tables")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("transfer-left-table")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("left: {}", state.transfer_left))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_transfer_table(true)),
                )
                .child(
                    row()
                        .key("transfer-right-table")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("right: {}", state.transfer_right))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_transfer_table(false)),
                ),
        )
        .child(
            row()
                .key("transfer-pair-controls")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("transfer-left-col")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("col: {}", state.transfer_pair_left))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_transfer_pair_column(true)),
                )
                .child(
                    row()
                        .key("transfer-right-col")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("col: {}", state.transfer_pair_right))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_transfer_pair_column(false)),
                )
                .child(
                    row()
                        .key("transfer-pair")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Pair").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.pair_transfer_columns()),
                ),
        );
    for (index, (left, right)) in state.transfer_pairs.iter().enumerate() {
        section = section.child(
            row()
                .key(format!("transfer-pair-row-{index}"))
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!("{left} → {right}"))
                        .font(type_::LABEL)
                        .color(theme.text),
                ),
        );
    }
    for (side, key) in [(true, "transfer-unmapped-left"), (false, "transfer-unmapped-right")] {
        let label = if side { "left" } else { "right" };
        let unmapped = state.transfer_unmapped(side);
        let mut block = col()
            .key(key)
            .w_full()
            .gap(2.0)
            .px(8.0)
            .py(4.0)
            .radius(6.0)
            .fill(theme.control_off)
            .child(
                text(format!("unmapped {label}: {}", unmapped.join(", ")))
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            );
        let _ = &mut block;
        section = section.child(block);
    }
    // D10: confirm before the port hands out a job; cancelling leaves none.
    if state.transfer_confirming {
        section = section.child(
            row()
                .key("transfer-confirm")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(text("Transfer now?".to_owned()).font(type_::LABEL).color(theme.text))
                .child(
                    row()
                        .key("transfer-yes")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Confirm".to_owned()).font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.confirm_transfer()),
                )
                .child(
                    row()
                        .key("transfer-no")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Cancel".to_owned()).font(type_::LABEL).color(theme.text_secondary))
                        .on_press(|state: &mut AppState| state.cancel_transfer()),
                ),
        );
    } else {
        section = section.child(
            row()
                .key("transfer-start")
                .w(140.0)
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text("Transfer".to_owned()).font(type_::LABEL).color(theme.text))
                .on_press(|state: &mut AppState| state.begin_transfer_confirm()),
        );
    }
    if let Some(error) = &state.transfer_error {
        section = section.child(
            row()
                .key("transfer-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if let Some(job) = state.transfer_job {
        section = section.child(
            row()
                .key("transfer-job")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(format!("job #{job}")).font(type_::LABEL).color(theme.text_secondary))
                .child(
                    row()
                        .key("transfer-refresh")
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(text("Refresh").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.refresh_transfer_progress()),
                ),
        );
    }
    if let Some(snapshot) = &state.transfer_snapshot {
        section = section.child(progress_panel(snapshot, "transfer", theme));
    }
    section
}

/// The structure comparison. Step 1: the objects, same names marked.
fn compare_tool(state: &AppState, theme: tok::Theme) -> El {
    let objects = state.workspace.compare_objects(&state.compare_source, &state.compare_target)
        .unwrap_or_default();
    let mut section = col()
        .key("compare-tool")
        .w_full()
        .gap(6.0)
        .child(
            row()
                .key("compare-endpoints")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("compare-source")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("source: {}", state.compare_source))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_compare_source()),
                )
                .child(
                    row()
                        .key("compare-target")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("target: {}", state.compare_target))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_compare_target()),
                ),
        );
    let mut list = col().key("compare-objects").w_full().gap(4.0);
    for object in &objects {
        let mark = if object.present_on_target {
            format!("compare-match-{}", object.name)
        } else {
            format!("compare-missing-{}", object.name)
        };
        let label = format!(
            "{} {}",
            object.name,
            if object.present_on_target { "= both ends" } else { "= source only" }
        );
        list = list.child(
            row()
                .key(format!("compare-object-{}", object.name))
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(label).font(type_::LABEL).color(theme.text))
                .child(row().key(mark).w(10.0).h(10.0).radius(5.0).fill(
                    if object.present_on_target {
                        oklch(0.75, 0.12, 145.0)
                    } else {
                        oklch(0.7, 0.14, 25.0)
                    },
                )),
        );
    }
    // The plan text: the port's string verbatim; an empty plan has its own state.
    match state.workspace.compare_plan(&state.compare_source, &state.compare_target) {
        Ok(plan) if !plan.is_empty() => {
            let mut block = col()
                .key("compare-plan")
                .w_full()
                .gap(2.0)
                .px(12.0)
                .py(8.0)
                .radius(8.0)
                .fill(theme.control_off);
            for line in plan.lines() {
                block = block.child(
                    text(line.to_owned())
                        .font(sql_editor::editor_font())
                        .color(theme.text),
                );
            }
            section = section.child(block);
        }
        Ok(_) => section = section.child(
            row()
                .key("compare-plan-empty")
                .w_full()
                .h(40.0)
                .px(12.0)
                .center_y()
                .radius(8.0)
                .fill(theme.control_off)
                .child(
                    text("No differences".to_owned())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        ),
        Err(_) => {}
    }
    // D8: apply the plan; the progress reuses the snapshot panel.
    section = section.child(
        row()
            .key("compare-apply-controls")
            .w_full()
            .h(28.0)
            .gap(8.0)
            .center_y()
            .child(
                row()
                    .key("compare-apply")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(text("Apply plan").font(type_::LABEL).color(theme.text))
                    .on_press(|state: &mut AppState| state.apply_compare_plan()),
            )
            .child(
                row()
                    .key("compare-refresh")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(text("Refresh").font(type_::LABEL).color(theme.text))
                    .on_press(|state: &mut AppState| state.refresh_compare_progress()),
            ),
    );
    if let Some(error) = &state.compare_error {
        section = section.child(
            row()
                .key("compare-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if let Some(snapshot) = &state.compare_snapshot {
        section = section.child(progress_panel(snapshot, "compare", theme));
    }
    section.child(list)
}

/// The sync wizard. Step 1: pick the two endpoints.
fn sync_tool(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col().key("sync-tool").w_full().gap(6.0);
    match state.sync_step {
        0 => section = section
            .child(
                row()
                    .key("sync-step-1")
                    .w_full()
                    .h(28.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        row()
                            .key("sync-source")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(
                                text(format!("source: {}", state.sync_source))
                                    .font(type_::LABEL)
                                    .color(theme.text),
                            )
                            .on_press(|state: &mut AppState| state.cycle_sync_source()),
                    )
                    .child(
                        row()
                            .key("sync-target")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(
                                text(format!("target: {}", state.sync_target))
                                    .font(type_::LABEL)
                                    .color(theme.text),
                            )
                            .on_press(|state: &mut AppState| state.cycle_sync_target()),
                    )
                    .child(
                        row()
                            .key("sync-endpoints-next")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Next").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.sync_next_endpoints()),
                    ),
            ),
        1 => {
            section = section.child(
                row()
                    .key("sync-step-1-done")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(
                        text(format!("{} → {}", state.sync_source, state.sync_target))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            );
            let mut tables = col().key("sync-step-2").w_full().gap(4.0);
            for table in state.workspace.connection_tables(&state.sync_source) {
                let selected = state.sync_tables.contains(&table);
                tables = tables.child(
                    row()
                        .key(format!("sync-table-{table}"))
                        .w(220.0)
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(if selected { tok::accent(0, true) } else { theme.control_off })
                        .child(text(table.clone()).font(type_::LABEL).color(theme.text))
                        .on_press(move |state: &mut AppState| state.toggle_sync_table(&table)),
                );
            }
            section = section
                .child(tables)
                .child(
                    row()
                        .key("sync-table-controls")
                        .w_full()
                        .h(28.0)
                        .gap(8.0)
                        .center_y()
                        .child(
                            row()
                                .key("sync-select-all")
                                .h(24.0)
                                .px(8.0)
                                .center_y()
                                .radius(6.0)
                                .fill(theme.control_off)
                                .child(text("Select all").font(type_::LABEL).color(theme.text))
                                .on_press(|state: &mut AppState| state.sync_select_all()),
                        )
                        .child(
                            row()
                                .key("sync-clear")
                                .h(24.0)
                                .px(8.0)
                                .center_y()
                                .radius(6.0)
                                .fill(theme.control_off)
                                .child(text("Clear").font(type_::LABEL).color(theme.text))
                                .on_press(|state: &mut AppState| state.sync_clear_tables()),
                        )
                        .child(
                            row()
                                .key("sync-tables-next")
                                .h(24.0)
                                .px(8.0)
                                .center_y()
                                .radius(6.0)
                                .fill(theme.control_off)
                                .child(text("Next").font(type_::LABEL).color(theme.text))
                                .on_press(|state: &mut AppState| state.sync_next_tables()),
                        ),
                );
        }
        2 => {
            section = section.child(
                row()
                    .key("sync-step-1-done")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(
                        text(format!("{} → {}", state.sync_source, state.sync_target))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            );
            let mut options = col().key("sync-step-3").w_full().gap(4.0);
            for (index, label) in AppState::SYNC_OPTION_LABELS.iter().enumerate() {
                let on = state.sync_options[index];
                options = options.child(
                    row()
                        .key(format!("sync-option-{index}"))
                        .w(260.0)
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(if on { tok::accent(0, true) } else { theme.control_off })
                        .child(
                            text(format!("{}: {}", if on { "on" } else { "off" }, label))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(move |state: &mut AppState| state.toggle_sync_option(index)),
                );
            }
            section = section
                .child(options)
                .child(
                    row()
                        .key("sync-preview-next")
                        .w(120.0)
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Preview").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.sync_show_preview()),
                );
        }
        3 => {
            section = section.child(
                row()
                    .key("sync-step-1-done")
                    .w_full()
                    .h(24.0)
                    .center_y()
                    .child(
                        text(format!("{} → {}", state.sync_source, state.sync_target))
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    ),
            );
            if let Some(grid) = &state.sync_preview_grid {
                section = section.child(data_grid::result_panel(
                    grid,
                    data_grid::GridSlot::SyncPreview,
                    theme.text,
                    theme.text_secondary,
                    theme.control_off,
                ));
            }
            section = section.child(
                row()
                    .key("sync-run-controls")
                    .w_full()
                    .h(28.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        row()
                            .key("sync-run")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Run sync").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.sync_run()),
                    )
                    .child(
                        row()
                            .key("sync-refresh")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Refresh").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.refresh_sync_progress()),
                    ),
            );
            if let Some(snapshot) = &state.sync_snapshot {
                section = section.child(progress_panel(snapshot, "sync", theme));
            }
        }
        _ => section = section.child(
            row()
                .key("sync-step-1-done")
                .w_full()
                .h(24.0)
                .center_y()
                .child(
                    text(format!("{} → {}", state.sync_source, state.sync_target))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        ),
    }
    if let Some(error) = &state.sync_error {
        section = section.child(
            row()
                .key("sync-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    section
}

/// The workspace is driven by the tabs: a query tab shows the SQL editor above the
/// results, a loaded table page shows the grid, and anything else is an empty state.
/// The old standalone editor and demo-grid sections are no longer product pages.
fn workspace_body(state: &AppState, theme: tok::Theme) -> El {
    if query_selected(state) {
        return query_workspace(state, theme);
    }
    if let Some((database, schema)) = &state.er_schema {
        return er_view(state, database, schema, theme);
    }
    if state.grid.has_page() {
        return table_workspace(state, theme);
    }
    workspace_empty(theme)
}

/// The ER page: one box per table of the open schema. Wide tables (more than 12
/// columns) draw collapsed, showing the column count. Boxes drag; their
/// positions live in UI state.
fn er_view(state: &AppState, database: &str, schema: &str, theme: tok::Theme) -> El {
    let catalog = state.workspace.catalog();
    let tables: Vec<String> = catalog
        .databases
        .iter()
        .find(|db| db.name == database)
        .and_then(|db| db.schemas.iter().find(|node| node.name == schema))
        .map(|node| node.tables.clone())
        .unwrap_or_default();
    let mut canvas = col()
        .key("er-canvas")
        .w_full()
        .h(420.0)
        .radius(8.0)
        .fill(theme.control_off);
    for (index, table) in tables.iter().enumerate() {
        let columns = state.workspace.table_columns(table).unwrap_or_default();
        let default = ((index % 3) as f32 * 230.0, (index / 3) as f32 * 180.0);
        let (x, y) = state.er_positions.get(table).copied().unwrap_or(default);
        canvas = canvas.child(er_box(table, &columns, x, y, theme));
    }
    col()
        .key("er-page")
        .w_full()
        .gap(8.0)
        .child(
            row()
                .key("er-toolbar")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text(format!("ER · {database}/{schema}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    row()
                        .key("er-close")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Close").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.close_er()),
                ),
        )
        .child(canvas)
}

/// One table box. Collapsed boxes show the column count instead of every column.
fn er_box(
    table: &str,
    columns: &[datazen_ui_api::ColumnSpec],
    x: f32,
    y: f32,
    theme: tok::Theme,
) -> El {
    let collapsed = columns.len() > 12;
    let table = table.to_owned();
    let drag_table = table.clone();
    let mut class_box = col()
        .key(format!("er-box-{table}"))
        .absolute(x, y)
        .w(200.0)
        .radius(8.0)
        .fill(oklch(0.24, 0.03, 250.0))
        .px(8.0)
        .py(6.0)
        .gap(2.0)
        .child(
            text(table.to_owned())
                .key(format!("er-title-{table}"))
                .font(type_::TITLE)
                .color(theme.text),
        )
        .on_drag(move |state: &mut AppState, drag: &mut rune::Drag, _: &mut Tx| {
            match drag.phase {
                rune::Phase::Down => {
                    state.er_drag = Some((drag_table.clone(), (x, y)));
                }
                rune::Phase::Move => {
                    if let Some((dragged, (start_x, start_y))) = &state.er_drag {
                        if *dragged == drag_table {
                            state
                                .er_positions
                                .insert(drag_table.clone(), ((start_x + drag.dx).max(0.0), (start_y + drag.dy).max(0.0)));
                        }
                    }
                }
                rune::Phase::Up => {
                    state.er_drag = None;
                }
            }
        });
    if collapsed {
        class_box = class_box.child(
            text(format!("{} columns", columns.len()))
                .key(format!("er-collapsed-{table}"))
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );
    } else {
        for (index, column) in columns.iter().enumerate() {
            class_box = class_box.child(
                text(column.name.clone())
                    .key(format!("er-col-{table}-{index}"))
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            );
        }
    }
    class_box
}

/// A table tab shows the view switcher above either the data grid or the DDL.
fn table_workspace(state: &AppState, theme: tok::Theme) -> El {
    let title = state.tabs.selected().map(|tab| tab.title.clone()).unwrap_or_default();
    let _ = &title;
    let body = match state.table_view {
        TableView::Data => data_grid::panel(state, theme.text, theme.text_secondary, theme.control_off),
        TableView::Ddl => ddl_view(&state.workspace, &title, theme),
        TableView::Columns => columns_view(state, &title, theme),
        TableView::Indexes => indexes_view(&state.workspace, &title, theme),
        TableView::ForeignKeys => foreign_keys_view(&state.workspace, &title, theme),
    };
    col()
        .key("table-workspace")
        .w_full()
        .gap(8.0)
        .child(view_switcher(state, theme))
        .child(body)
}

fn view_switcher(state: &AppState, theme: tok::Theme) -> El {
    let entry = |key: &str, label: &str, view: TableView, active: bool| {
        row()
            .key(key)
            .h(24.0)
            .px(10.0)
            .center_y()
            .radius(6.0)
            .fill(if active { tok::accent(0, true) } else { theme.control_off })
            .child(text(label).font(type_::LABEL).color(theme.text))
            .on_press(move |state: &mut AppState| state.set_table_view(view))
    };
    row()
        .key("table-view-switcher")
        .w_full()
        .h(28.0)
        .gap(6.0)
        .center_y()
        .child(entry("view-data", "Data", TableView::Data, state.table_view == TableView::Data))
        .child(entry("view-columns", "Columns", TableView::Columns, state.table_view == TableView::Columns))
        .child(entry("view-indexes", "Indexes", TableView::Indexes, state.table_view == TableView::Indexes))
        .child(entry("view-fks", "Foreign keys", TableView::ForeignKeys, state.table_view == TableView::ForeignKeys))
        .child(entry("view-ddl", "DDL", TableView::Ddl, state.table_view == TableView::Ddl))
}

/// The column list: name, type, nullability — one row per column, plus the
/// drafts section. The official list is read-only; drafts are separate.
fn columns_view(state: &AppState, title: &str, theme: tok::Theme) -> El {
    let columns = state.workspace.table_columns(title).unwrap_or_default();
    let header = |label: &str, width: f32| {
        row()
            .w(width)
            .h(24.0)
            .px(8.0)
            .center_y()
            .child(text(label).font(type_::LABEL).color(theme.text_secondary))
    };
    let mut list = col()
        .key("columns-list")
        .w_full()
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key("columns-header")
                .w_full()
                .h(24.0)
                .child(header("Column", 240.0))
                .child(header("Type", 140.0))
                .child(header("Nullable", 100.0)),
        );
    for (index, column) in columns.iter().enumerate() {
        let cell = |value: &str, width: f32| {
            row()
                .w(width)
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(text(value.to_owned()).font(type_::LABEL).color(theme.text))
        };
        list = list.child(
            row()
                .key(format!("column-row-{index}"))
                .w_full()
                .h(24.0)
                .child(cell(&column.name, 240.0))
                .child(cell(&column.sql_type, 140.0))
                .child(cell(if column.nullable { "yes" } else { "no" }, 100.0)),
        );
    }
    let mut list = list.child(column_drafts_section(state, title, theme));
    list
}

/// The drafts area under the column list: inputs, the queued drafts, separate
/// from the official columns.
fn column_drafts_section(state: &AppState, title: &str, theme: tok::Theme) -> El {
    let drafts = state.column_drafts.get(title).cloned().unwrap_or_default();
    let mut section = col()
        .key("column-drafts")
        .w_full()
        .gap(6.0)
        .child(
            row()
                .key("draft-inputs")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text_input(&state.draft_name, type_::LABEL)
                        .key("draft-name-input")
                        .w(140.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.draft_name),
                )
                .child(
                    text_input(&state.draft_type, type_::LABEL)
                        .key("draft-type-input")
                        .w(110.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.draft_type),
                )
                .child(
                    row()
                        .key("draft-add")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("+ Column").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.add_column_draft()),
                )
                .child(
                    row()
                        .key("draft-retype")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Retype").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.add_retype_draft()),
                ),
        );
    for (index, draft) in drafts.iter().enumerate() {
        let label = match draft {
            ColumnDraft::AddColumn { name, sql_type } => format!("add {name} {sql_type}"),
            ColumnDraft::ChangeType { column, sql_type } => format!("retype {column} → {sql_type}"),
        };
        section = section.child(
            row()
                .key(format!("draft-row-{index}"))
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(label).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if !drafts.is_empty() {
        section = section.child(
            row()
                .key("draft-apply")
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text("Apply").font(type_::LABEL).color(theme.text))
                .on_press(|state: &mut AppState| state.apply_column_drafts()),
        );
    }
    if !drafts.is_empty() {
        let preview = drafts
            .iter()
            .map(|draft| draft.sql(title))
            .collect::<Vec<_>>()
            .join("\n");
        let mut sql_block = col()
            .key("draft-sql")
            .w_full()
            .px(12.0)
            .py(8.0)
            .radius(8.0)
            .fill(theme.control_off);
        for line in preview.lines() {
            sql_block = sql_block.child(
                text(line.to_owned())
                    .font(sql_editor::editor_font())
                    .color(theme.text),
            );
        }
        section = section.child(sql_block);
    }
    section
}

/// The index list: name, columns in index order, uniqueness.
fn indexes_view(workspace: &MemoryWorkspace, title: &str, theme: tok::Theme) -> El {
    let indexes = workspace.table_indexes(title).unwrap_or_default();
    let header = |label: &str, width: f32| {
        row()
            .w(width)
            .h(24.0)
            .px(8.0)
            .center_y()
            .child(text(label).font(type_::LABEL).color(theme.text_secondary))
    };
    let mut list = col()
        .key("indexes-list")
        .w_full()
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key("indexes-header")
                .w_full()
                .h(24.0)
                .child(header("Index", 260.0))
                .child(header("Columns", 320.0))
                .child(header("Unique", 100.0)),
        );
    for (index, spec) in indexes.iter().enumerate() {
        let cell = |value: &str, width: f32| {
            row()
                .w(width)
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(text(value.to_owned()).font(type_::LABEL).color(theme.text))
        };
        list = list.child(
            row()
                .key(format!("index-row-{index}"))
                .w_full()
                .h(24.0)
                .child(cell(&spec.name, 260.0))
                .child(cell(&spec.columns.join(", "), 320.0))
                .child(cell(if spec.unique { "yes" } else { "no" }, 100.0)),
        );
    }
    list
}

/// The foreign key list: this table's columns, the target table, its columns.
fn foreign_keys_view(workspace: &MemoryWorkspace, title: &str, theme: tok::Theme) -> El {
    let keys = workspace.table_foreign_keys(title).unwrap_or_default();
    let header = |label: &str, width: f32| {
        row()
            .w(width)
            .h(24.0)
            .px(8.0)
            .center_y()
            .child(text(label).font(type_::LABEL).color(theme.text_secondary))
    };
    let mut list = col()
        .key("fks-list")
        .w_full()
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key("fks-header")
                .w_full()
                .h(24.0)
                .child(header("Name", 260.0))
                .child(header("Columns", 240.0))
                .child(header("References", 320.0)),
        );
    for (index, key) in keys.iter().enumerate() {
        let cell = |value: &str, width: f32| {
            row()
                .w(width)
                .h(24.0)
                .px(8.0)
                .center_y()
                .child(text(value.to_owned()).font(type_::LABEL).color(theme.text))
        };
        let references = format!(
            "{} ({})",
            key.target_table,
            key.target_columns.join(", ")
        );
        list = list.child(
            row()
                .key(format!("fk-row-{index}"))
                .w_full()
                .h(24.0)
                .child(cell(&key.name, 260.0))
                .child(cell(&key.columns.join(", "), 240.0))
                .child(cell(&references, 320.0)),
        );
    }
    list
}

/// The DDL page: the port's string, verbatim, one line per row. An empty string
/// is its own empty state.
fn ddl_view(workspace: &MemoryWorkspace, title: &str, theme: tok::Theme) -> El {
    let ddl = workspace.table_ddl(title).ok().filter(|ddl| !ddl.is_empty());
    match ddl {
        Some(ddl) => {
            let mut page = col()
                .key("table-ddl")
                .w_full()
                .px(12.0)
                .py(8.0)
                .radius(8.0)
                .fill(theme.control_off);
            for line in ddl.lines() {
                page = page.child(text(line.to_owned()).font(sql_editor::editor_font()).color(theme.text));
            }
            page
        }
        None => col()
            .key("table-ddl-empty")
            .w_full()
            .h(160.0)
            .px(12.0)
            .py(8.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(text("No DDL available for this table").font(type_::TITLE).color(theme.text))
            .child(
                text("The port returned an empty DDL string")
                    .font(type_::LABEL)
                    .color(theme.text_secondary),
            ),
    }
}

fn workspace_empty(theme: tok::Theme) -> El {
    col()
        .key("workspace-empty")
        .w_full()
        .h(220.0)
        .px(20.0)
        .py(20.0)
        .gap(12.0)
        .radius(12.0)
        .fill(theme.control_off)
        .child(text("No tab open").font(type_::TITLE).color(theme.text))
        .child(
            text("Open a table from the session object tree, or start a new query from the sidebar.")
                .font(type_::LABEL)
                .color(theme.text_secondary)
                .w_full(),
        )
}

fn settings_page(state: &AppState, theme: tok::Theme) -> El {
    let dark = state
        .workspace
        .read_settings()
        .iter()
        .find_map(|setting| match setting {
            Setting::Theme { dark } => Some(*dark),
            _ => None,
        })
        .unwrap_or(true);
    let page_size = state
        .workspace
        .read_settings()
        .iter()
        .find_map(|setting| match setting {
            Setting::TablePageSize { rows } => Some(*rows),
            _ => None,
        })
        .unwrap_or(TABLE_PAGE_SIZE);
    let max_rows = state
        .workspace
        .read_settings()
        .iter()
        .find_map(|setting| match setting {
            Setting::QueryMaxRows { rows } => Some(*rows),
            _ => None,
        })
        .unwrap_or(100);
    let mut page = col()
        .key("settings-page")
        .w_full()
        .gap(12.0)
        .child(
            row()
                .key("settings-appearance")
                .w_full()
                .h(32.0)
                .gap(12.0)
                .center_y()
                .child(
                    text("Appearance".to_owned())
                        .font(type_::TITLE)
                        .color(theme.text),
                )
                .child(
                    row()
                        .key("settings-theme")
                        .h(28.0)
                        .px(10.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(if dark { "Theme: Dark" } else { "Theme: Light" }.to_owned())
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.toggle_theme()),
                ),
        )
        .child(
            row()
                .key("settings-data")
                .w_full()
                .h(32.0)
                .gap(8.0)
                .center_y()
                .child(text("Data".to_owned()).font(type_::TITLE).color(theme.text))
                .child(
                    text(format!("Rows per page: {page_size}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    text_input(&state.page_size_input, type_::LABEL)
                        .key("settings-page-size-input")
                        .w(90.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.page_size_input),
                )
                .child(
                    row()
                        .key("settings-page-size-set")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Set").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.set_table_page_size()),
                )
                .child(
                    text(format!("Query max rows: {max_rows}"))
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    text_input(&state.max_rows_input, type_::LABEL)
                        .key("settings-max-rows-input")
                        .w(90.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.max_rows_input),
                )
                .child(
                    row()
                        .key("settings-max-rows-set")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Set").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.set_query_max_rows()),
                ),
        );
    if let Some(error) = &state.settings_error {
        page = page.child(
            row()
                .key("settings-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }

    // SQL snippets: add, update, delete.
    let mut snippets = col()
        .key("settings-snippets")
        .w_full()
        .gap(6.0)
        .child(text("SQL snippets".to_owned()).font(type_::TITLE).color(theme.text))
        .child(
            row()
                .key("snippet-form")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text_input(&state.snippet_name, type_::LABEL)
                        .key("snippet-name-input")
                        .w(140.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.snippet_name),
                )
                .child(
                    text_input(&state.snippet_sql, type_::LABEL)
                        .key("snippet-sql-input")
                        .w(280.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.snippet_sql),
                )
                .child(
                    row()
                        .key("snippet-save")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Save").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.save_snippet_form()),
                )
                .child(
                    row()
                        .key("snippet-update")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Update").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.update_snippet_form()),
                ),
        );
    for (index, snippet) in state.workspace.snippets().iter().enumerate() {
        let name = snippet.name.clone();
        snippets = snippets.child(
            row()
                .key(format!("snippet-row-{index}"))
                .w_full()
                .h(24.0)
                .gap(8.0)
                .center_y()
                .px(8.0)
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(snippet.name.clone()).font(type_::LABEL).color(theme.text))
                .child(text(snippet.sql.clone()).font(type_::LABEL).color(theme.text_secondary))
                .child(
                    row()
                        .key(format!("snippet-delete-{index}"))
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(text("Delete").font(type_::LABEL).color(theme.text_secondary))
                        .on_press(move |state: &mut AppState| state.delete_snippet(&name)),
                ),
        );
    }
    if let Some(error) = &state.snippet_error {
        snippets = snippets.child(
            row()
                .key("snippet-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    page.child(snippets)
        .child(export_section(state, theme))
        .child(import_section(state, theme))
        .child(backup_section(state, theme))
}

/// The backup page: start, stop, and the same progress panel as exports.
fn backup_section(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("backup-section")
        .w_full()
        .gap(6.0)
        .child(text("Backup".to_owned()).font(type_::TITLE).color(theme.text))
        .child(
            row()
                .key("backup-controls")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("backup-start")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Start backup").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.start_backup_form()),
                )
                .child(
                    row()
                        .key("backup-stop")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Stop").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.stop_backup()),
                )
                .child(
                    row()
                        .key("backup-refresh")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Refresh").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.refresh_backup_progress()),
                ),
        );
    if let Some(error) = &state.backup_error {
        section = section.child(
            row()
                .key("backup-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if let Some(snapshot) = &state.backup_snapshot {
        section = section.child(progress_panel(snapshot, "backup", theme));
    }
    section
}

/// The import wizard's first two steps: source, then target table. No target,
/// no next step.
fn import_section(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("import-section")
        .w_full()
        .gap(6.0)
        .child(text("Import".to_owned()).font(type_::TITLE).color(theme.text));
    match state.import_step {
        0 => section = section.child(
            row()
                .key("import-step-1")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    text("Step 1 · Source".to_owned())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                )
                .child(
                    text_input(&state.import_source, type_::LABEL)
                        .key("import-source-input")
                        .w(240.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.import_source),
                )
                .child(
                    row()
                        .key("import-next")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Next").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.import_next()),
                ),
        ),
        1 => section = section
            .child(
                row()
                    .key("import-step-2")
                    .w_full()
                    .h(28.0)
                    .gap(8.0)
                    .center_y()
                    .child(
                        text("Step 2 · Target table".to_owned())
                            .font(type_::LABEL)
                            .color(theme.text_secondary),
                    )
                    .child(
                        text_input(&state.import_target, type_::LABEL)
                            .key("import-target-input")
                            .w(180.0)
                            .h(24.0)
                            .on_edit(|state: &mut AppState| &mut state.import_target),
                    )
                    .child(
                        row()
                            .key("import-finish")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Finish").font(type_::LABEL).color(theme.text))
                            .on_press(|state: &mut AppState| state.import_finish()),
                    )
                    .child(
                        row()
                            .key("import-back")
                            .h(24.0)
                            .px(8.0)
                            .center_y()
                            .radius(6.0)
                            .fill(theme.control_off)
                            .child(text("Back").font(type_::LABEL).color(theme.text_secondary))
                            .on_press(|state: &mut AppState| state.import_back()),
                    ),
            ),
        _ => section = section.child(
            row()
                .key("import-ready")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(format!(
                        "Import ready: {} into {}",
                        state.import_source.text.trim(),
                        state.import_target.text.trim()
                    ))
                    .font(type_::LABEL)
                    .color(theme.text),
                ),
        ),
    }
    if let Some(error) = &state.import_error {
        section = section.child(
            row()
                .key("import-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    section
}

/// The export dialog collects a format and a range, then asks the port for a
/// job id. No range, no job.
fn export_section(state: &AppState, theme: tok::Theme) -> El {
    let mut section = col()
        .key("export-section")
        .w_full()
        .gap(6.0)
        .child(text("Export".to_owned()).font(type_::TITLE).color(theme.text))
        .child(
            row()
                .key("export-form")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("export-format")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(
                            text(format!("format: {}", state.export_format()))
                                .font(type_::LABEL)
                                .color(theme.text),
                        )
                        .on_press(|state: &mut AppState| state.cycle_export_format()),
                )
                .child(
                    text_input(&state.export_range, type_::LABEL)
                        .key("export-range-input")
                        .w(220.0)
                        .h(24.0)
                        .on_edit(|state: &mut AppState| &mut state.export_range),
                )
                .child(
                    row()
                        .key("export-start")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Export").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.start_export_form()),
                ),
        );
    if let Some(error) = &state.export_error {
        section = section.child(
            row()
                .key("export-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    if let Some(job) = state.export_job {
        section = section.child(
            row()
                .key("export-job")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(format!("job #{job}")).font(type_::LABEL).color(theme.text_secondary))
                .child(
                    row()
                        .key("export-refresh")
                        .h(20.0)
                        .px(6.0)
                        .center_y()
                        .radius(4.0)
                        .fill(theme.control_off)
                        .child(text("Refresh").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.refresh_export_progress()),
                ),
        );
    }
    if let Some(snapshot) = &state.export_snapshot {
        section = section.child(progress_panel(snapshot, "export", theme));
    }
    section
}

/// One job's progress drawn from a single snapshot: status, percent, the log.
/// Export and backup share this component.
fn progress_panel(snapshot: &ProgressSnapshot, prefix: &str, theme: tok::Theme) -> El {
    let mut panel = col()
        .key(format!("{prefix}-progress"))
        .w_full()
        .gap(4.0)
        .px(8.0)
        .py(6.0)
        .radius(8.0)
        .fill(theme.control_off)
        .child(
            row()
                .key(format!("{prefix}-status"))
                .w_full()
                .h(24.0)
                .center_y()
                .child(
                    text(format!("{} · {}%", snapshot.status, snapshot.percent))
                        .font(type_::LABEL)
                        .color(theme.text),
                ),
        );
    for (index, line) in snapshot.log.iter().enumerate() {
        panel = panel.child(
            row()
                .key(format!("{prefix}-log-{index}"))
                .w_full()
                .h(18.0)
                .center_y()
                .child(
                    text(line.clone())
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    panel
}

fn tab_strip(state: &AppState, theme: tok::Theme) -> El {
    let mut strip = row().key("tab-strip").w_full().h(32.0).gap(6.0).center_y();
    for tab in state.tabs.tabs() {
        let id = tab.id;
        let selected = state.tabs.selected().is_some_and(|open| open.id == id);
        strip = strip.child(
            row()
                .key(format!("tab-{id}"))
                .h(28.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(if selected { tok::accent(0, true) } else { theme.control_off })
                .child(text(tab.title.clone()).font(type_::LABEL).color(theme.text))
                .on_press(move |state: &mut AppState| {
                    state.select_tab(id);
                }),
        );
        strip = strip.child(
            row()
                .key(format!("tab-close-{id}"))
                .h(28.0)
                .px(6.0)
                .center_y()
                .child(text("×").font(type_::LABEL).color(theme.text))
                .on_press(move |state: &mut AppState| {
                    state.close_tab(id);
                }),
        );
    }
    strip
}

/// The session object tree: databases, schemas and tables from the port's
/// catalog, with the tree's collapsed nodes kept in UI state.
fn object_tree(state: &AppState, theme: tok::Theme) -> El {
    let catalog = state.workspace.catalog();
    let mut rows: Vec<El> = Vec::new();
    for database in &catalog.databases {
        let db_path = database.name.clone();
        rows.push(
            row()
                .key(format!("tree-db-{}", database.name))
                .w_full()
                .h(28.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(format!("{} · database", database.name)).font(type_::LABEL).color(theme.text))
                .on_press(move |state: &mut AppState| state.toggle_expand(&db_path)),
        );
        if state.collapsed.contains(&database.name) {
            continue;
        }
        for schema in &database.schemas {
            let schema_path = format!("{}/{}", database.name, schema.name);
            let schema_key = format!("tree-schema-{}-{}", database.name, schema.name);
            let collapsed = state.collapsed.contains(&schema_path);
            rows.push(
                row()
                    .key(schema_key)
                    .w_full()
                    .h(28.0)
                    .px(20.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(text(format!("{} · schema", schema.name)).font(type_::LABEL).color(theme.text_secondary))
                    .on_press(move |state: &mut AppState| state.toggle_expand(&schema_path)),
            );
            if collapsed {
                continue;
            }
            for name in &schema.tables {
                let title = name.clone();
                rows.push(
                    row()
                        .key(format!("tree-table-{name}"))
                        .w_full()
                        .h(28.0)
                        .px(32.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text(name.clone()).font(type_::LABEL).color(theme.text))
                        .on_press(move |state: &mut AppState| {
                            state.open_table_tab(&title);
                        }),
                );
            }
        }
    }
    let count = rows.len() as f32;
    let height = if count == 0.0 { 0.0 } else { count * 28.0 + (count - 1.0) * 4.0 };
    let mut tree = col().key("object-tree").w_full().h(height).gap(4.0);
    for row in rows {
        tree = tree.child(row);
    }
    tree
}

fn query_workspace(state: &AppState, theme: tok::Theme) -> El {
    // N1: a query job in flight says so above whatever the area shows —
    // the placeholder before the first result, the stale grid of the run
    // before it.
    let loading = state.query_loading.is_some().then(|| {
        row()
            .key("query-loading")
            .w_full()
            .h(20.0)
            .center_y()
            .child(text("Running…".to_owned()).font(type_::LABEL).color(theme.text_secondary))
    });
    let results = match &state.result_grid {
        Some(grid) => data_grid::result_panel(
            grid,
            data_grid::GridSlot::Result,
            theme.text,
            theme.text_secondary,
            theme.control_off,
        ),
        None => col()
            .key("query-results")
            .w_full()
            .h(120.0)
            .px(12.0)
            .py(8.0)
            .radius(8.0)
            .fill(theme.control_off)
            .child(text("No results").font(type_::LABEL).color(theme.text_secondary)),
    };
    col()
        .key("query-tab")
        .w_full()
        .gap(8.0)
        .child(sql_editor::panel(&state.sql, theme.text, theme.text_secondary))
        .child(
            row()
                .key("query-toolbar")
                .w_full()
                .h(28.0)
                .gap(8.0)
                .center_y()
                .child(
                    row()
                        .key("sql-run")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Run").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.run_query()),
                )
                .child(
                    row()
                        .key("sql-cancel")
                        .h(24.0)
                        .px(8.0)
                        .center_y()
                        .radius(6.0)
                        .fill(theme.control_off)
                        .child(text("Cancel").font(type_::LABEL).color(theme.text))
                        .on_press(|state: &mut AppState| state.cancel_query()),
                )
                .child(
                    text("Command+Enter runs the statement at the cursor")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        )
        .children(loading)
        .child(results)
}

fn open_query_button(theme: tok::Theme) -> El {
    row()
        .key("open-query")
        .w_full()
        .h(32.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("New query").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| {
            state.open_query_tab("Query");
        })
}

fn er_button(state: &AppState, theme: tok::Theme) -> El {
    if state.workspace.session().is_none() {
        return row().key("er-button-absent").h(0.0);
    }
    row()
        .key("er-button")
        .w_full()
        .h(28.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("ER diagram").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.open_er())
}

fn refresh_button(state: &AppState, theme: tok::Theme) -> El {
    if state.workspace.session().is_none() {
        return row().key("tree-refresh-absent").h(0.0);
    }
    row()
        .key("tree-refresh")
        .w_full()
        .h(28.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("Refresh catalog").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.refresh_catalog())
}

fn disconnect_button(state: &AppState, theme: tok::Theme) -> El {
    if state.workspace.session().is_none() {
        return row().key("disconnect-absent").h(0.0);
    }
    row()
        .key("disconnect")
        .w_full()
        .h(32.0)
        .px(8.0)
        .center_y()
        .radius(6.0)
        .fill(theme.control_off)
        .child(text("Disconnect").font(type_::LABEL).color(theme.text))
        .on_press(|state: &mut AppState| state.disconnect())
}

fn connection_list(state: &AppState, theme: tok::Theme) -> El {
    let mut list = col().key("connection-list").w_full().gap(8.0);
    // The desktop adapter's answer: its list once it is wired (H3), or its
    // fixed error over the memory port's demo list until then (H2).
    if let Some(message) = &state.desktop_error {
        list = list.child(
            row()
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(
                    text(message.clone())
                        .key("desktop-adapter-message")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );
    }
    let port_connections = state.workspace.connections();
    let connections = state
        .desktop_connections
        .as_deref()
        .unwrap_or(port_connections);
    for profile in connections {
        let id = profile.id.clone();
        let label = format!("{} · {}", profile.name, profile.engine);
        list = list.child(
            row()
                .key(format!("connection-{id}"))
                .w_full()
                .h(40.0)
                .px(12.0)
                .center_y()
                .radius(8.0)
                .fill(theme.control_off)
                .child(text(label).font(type_::LABEL).color(theme.text))
                .on_press(move |state: &mut AppState| state.open_connection(&id)),
        );
    }
    let mut form = col().key("connection-form").w_full().gap(8.0).child(
        row()
            .key("connection-form-fields")
            .w_full()
            .h(28.0)
            .gap(8.0)
            .center_y()
            .child(
                text_input(&state.connection_name, type_::LABEL)
                    .key("connection-name-input")
                    .w(150.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_name),
            )
            .child(
                text_input(&state.connection_engine, type_::LABEL)
                    .key("connection-engine-input")
                    .w(110.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_engine),
            )
            .child(
                text_input(&state.connection_address, type_::LABEL)
                    .key("connection-address-input")
                    .w(170.0)
                    .h(24.0)
                    .on_edit(|state: &mut AppState| &mut state.connection_address),
            )
            .child(
                row()
                    .key("connection-save")
                    .h(24.0)
                    .px(8.0)
                    .center_y()
                    .radius(6.0)
                    .fill(theme.control_off)
                    .child(text("Save connection").font(type_::LABEL).color(theme.text))
                    .on_press(|state: &mut AppState| state.save_connection_form()),
            ),
    );
    if let Some(error) = &state.connection_error {
        form = form.child(
            row()
                .key("connection-form-error")
                .w_full()
                .h(24.0)
                .px(8.0)
                .center_y()
                .radius(6.0)
                .fill(theme.control_off)
                .child(text(error.clone()).font(type_::LABEL).color(theme.text_secondary)),
        );
    }
    list.child(form)
}

fn view(state: &AppState, cx: &mut Cx<AppState>) -> El {
    let dark = state
        .workspace
        .read_settings()
        .iter()
        .find_map(|setting| match setting {
            Setting::Theme { dark } => Some(*dark),
            _ => None,
        })
        .unwrap_or_else(|| cx.system_dark());
    let theme = tok::theme(dark);

    let mut navigation = col().w_full().gap(4.0);
    for section in Section::ALL {
        navigation = navigation.child(navigation_item(state, section, dark));
    }

    let sidebar = col()
        .key("datazen-sidebar")
        .w(224.0)
        .h_full()
        .px(16.0)
        .py(20.0)
        .gap(18.0)
        .fill(theme.bg)
        .child(
            text("DataZen")
                .font(type_::TITLE)
                .color(theme.text),
        )
        .child(
            text("DATABASE WORKSPACE")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        )
        .child(navigation)
        .child(object_tree(state, theme))
        .child(refresh_button(state, theme))
        .child(er_button(state, theme))
        .child(open_query_button(theme))
        .child(disconnect_button(state, theme))
        .child(
            col()
                .w_full()
                .h(1.0)
                .fill(theme.control_off),
        )
        .child(
            text("Rune native UI preview")
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );

    let header = row()
        .key("datazen-header")
        .w_full()
        .h(48.0)
        .center_y()
        .gap(12.0)
        .child(
            text(state.active.label())
                .font(type_::TITLE)
                .color(theme.text)
                .w_full(),
        )
        .child(
            text(if dark { "DARK" } else { "LIGHT" })
                .font(type_::LABEL)
                .color(theme.text_secondary),
        );

    let page = col()
        .key("datazen-page")
        .w_full()
        .h_full()
        .px(24.0)
        .py(24.0)
        .gap(16.0)
        .fill(theme.bg)
        .child(header)
        .child(if state.active == Section::Workspace {
            tab_strip(state, theme)
        } else {
            row().key("tab-strip-absent").h(0.0)
        })
        .child(page_body(state, theme))
        .child(
            row()
                .w_full()
                .h(28.0)
                .center_y()
                .child(
                    text("Migration preview · existing React and Tauri entrypoints are unchanged")
                        .font(type_::LABEL)
                        .color(theme.text_secondary),
                ),
        );

    row()
        .key("datazen-root")
        .w_full()
        .h_full()
        .fill(theme.bg)
        .child(sidebar)
        .child(page)
}

/// Launch the same Rune view on native desktop and WebAssembly.
pub fn run() {
    App::new(AppSpec {
        on_key: Some(app_key),
        poll: Some(app_poll),
        fonts: APP_FONTS,
        families: APP_FONT_FAMILIES,
        ..AppSpec::new("DataZen", (1440.0, 900.0), init_state, view)
    })
    .on_ime(app_ime)
    .run();
}

/// The desktop process starts on the connections page, so the adapter is
/// consulted before the first frame: its answer (or its fixed error) is on
/// the page from the start. H3: the native process lists connections through
/// the connection use case, over the adapter's in-memory store; the wasm
/// host starts on the unwired adapter and the port answers.
fn init_state() -> AppState {
    let mut state = AppState::default();
    #[cfg(not(target_arch = "wasm32"))]
    {
        state.desktop = Box::new(UseCaseAdapter::with_demo_connections());
    }
    state.sync_desktop_connections();
    state
}

fn app_key(state: &mut AppState, ev: &KeyEvent, tx: &mut Tx) -> bool {
    if state.active != Section::Workspace {
        return state.active == Section::Workflows
            && state.workflow_editing.is_some()
            && sql_editor::on_key(&mut state.workflow_yaml, ev);
    }
    if query_selected(state) {
        if matches!(ev.key, Key::Enter) && ev.mods.cmd {
            state.run_query();
            return true;
        }
        return sql_editor::on_key(&mut state.sql, ev);
    }
    if state.grid.has_page() {
        return data_grid::on_key(&mut state.grid, ev, tx);
    }
    false
}

fn app_poll(state: &mut AppState, _: f64, _: &mut Tx) -> bool {
    // N1: the host's finished jobs land first, whatever section is showing;
    // a delivery redraws (it wrote state or cleared its marker), and stale
    // generations were dropped inside `deliver_host_job`.
    let mut dirty = false;
    for delivery in state.desktop.poll_jobs() {
        state.deliver_host_job(delivery);
        dirty = true;
    }
    if state.active != Section::Workspace {
        return dirty;
    }
    if query_selected(state) {
        return dirty || state.result_grid.as_mut().is_some_and(|grid| grid.sync_scroll());
    }
    dirty || (state.grid.has_page() && state.grid.sync_scroll())
}

fn app_ime(state: &mut AppState, ev: &Ime, _: &mut Tx) -> bool {
    if state.active == Section::Workflows && state.workflow_editing.is_some() {
        return sql_editor::on_ime(&mut state.workflow_yaml, ev);
    }
    state.active == Section::Workspace && query_selected(state) && sql_editor::on_ime(&mut state.sql, ev)
}

/// WebAssembly entrypoint consumed by wasm-bindgen.
#[cfg(target_arch = "wasm32")]
#[wasm_bindgen::prelude::wasm_bindgen(start)]
pub fn start() {
    run();
}

#[cfg(test)]
mod tests {
    use super::{AppState, APP_FONT_FAMILIES, APP_FONTS, Section, TABLE_PAGE_SIZE, query_selected, sql_editor, type_, view};
    use rune::{Env, Runtime};

    fn runtime() -> Runtime<AppState> {
        let env = Env {
            system_dark: true,
            scale: 1.0,
        };
        Runtime::new(AppState::default(), view, (1440.0, 900.0), env)
    }

    /// Glyph ids of `text` shaped by `system`, in layout order.
    fn glyph_ids(system: &mut rune::text::TextSystem, text: &str) -> Vec<u32> {
        use parley::PositionedLayoutItem;

        let layout = system.layout_rich(text, &[], type_::LABEL, 2.0, None);
        let mut ids = Vec::new();
        for line in layout.lines() {
            for item in line.items() {
                if let PositionedLayoutItem::GlyphRun(run) = item {
                    ids.extend(run.glyphs().map(|glyph| glyph.id));
                }
            }
        }
        ids
    }

    #[test]
    fn the_apps_font_pool_shapes_chinese_without_tofu() {
        // The pool run() installs (H8): Inter for the UI's Latin, Noto Sans SC
        // for Chinese in the interface and the grid, IBM Plex Mono for the
        // editor. A shaped Han character is a real glyph, not .notdef.
        let mut system = rune::text::TextSystem::new(APP_FONTS, APP_FONT_FAMILIES);
        let han = glyph_ids(&mut system, "中文数据");
        assert_eq!(han.len(), 4, "one glyph per Han character");
        assert!(han.iter().all(|id| *id != 0), "tofu: {han:?}");

        // The UI's Latin look is unchanged: Inter still serves Latin.
        let latin = glyph_ids(&mut system, "DataZen");
        assert!(latin.iter().all(|id| *id != 0), "tofu: {latin:?}");
    }

    #[test]
    fn page_home_and_end_move_the_focus_and_it_stays_in_the_model() {
        use super::app_key;
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.focus(), (0, 0));

        let key = |runtime: &mut rune::Runtime<AppState>, key: Key| {
            runtime.key(KeyEvent { key, mods: Mods::default() });
            runtime.frame(0.0);
        };

        // PageDown jumps a loaded page of rows, clamped to the boundary.
        key(&mut runtime, Key::PageDown);
        assert_eq!(runtime.state.grid.focus(), (24, 0), "one page down lands on the last row");
        key(&mut runtime, Key::PageDown);
        assert_eq!(runtime.state.grid.focus(), (24, 0), "the clamp keeps the focus inside the model");

        // PageUp goes back a page.
        key(&mut runtime, Key::PageUp);
        assert_eq!(runtime.state.grid.focus(), (0, 0));

        // End is the row's last column; Home is the first again.
        key(&mut runtime, Key::End);
        let (_, cols) = {
            let grid = &runtime.state.grid;
            (grid.rows, grid.cols)
        };
        assert_eq!(runtime.state.grid.focus(), (0, cols - 1), "End is the last column");
        key(&mut runtime, Key::Home);
        assert_eq!(runtime.state.grid.focus(), (0, 0), "Home is the first column");

        // After every move the focus is still inside the grid's model.
        let (rows, cols) = {
            let grid = &runtime.state.grid;
            (grid.rows, grid.cols)
        };
        let (r, c) = runtime.state.grid.focus();
        assert!(r < rows && c < cols, "the focus stays in the model");
    }

    #[test]
    fn the_grid_reads_its_shape_and_current_cell_to_the_accessibility_tree() {
        use super::app_key;
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};
        use serde_json::Value;

        fn find<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
            if v["key"] == key {
                return Some(v);
            }
            v["children"].as_array()?.iter().find_map(|child| find(child, key))
        }

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        // The grid reads its page's shape and where the focus is.
        let tree = runtime.accessibility_json();
        let grid = find(&tree, "data-grid-scroll").expect("the grid is in the tree");
        assert_eq!(grid["role"], "grid");
        assert_eq!(grid["name"], "customers");
        assert_eq!(grid["value"], "25 rows × 8 columns");
        assert_eq!(grid["activeDescendant"], "cell-0-0");
        let cell = find(&tree, "cell-0-0").expect("the focused cell is mounted");
        assert_eq!(cell["role"], "grid-cell");
        assert_eq!(cell["name"], "0");

        // Moving the focus moves what the tree reads.
        let key = |runtime: &mut rune::Runtime<AppState>, key: Key| {
            runtime.key(KeyEvent { key, mods: Mods::default() });
            runtime.frame(0.0);
        };
        key(&mut runtime, Key::Down);
        key(&mut runtime, Key::Down);
        key(&mut runtime, Key::Right);
        assert_eq!(runtime.state.grid.focus(), (2, 1));
        let tree = runtime.accessibility_json();
        let grid = find(&tree, "data-grid-scroll").unwrap();
        assert_eq!(grid["activeDescendant"], "cell-2-1", "the tree follows the focus");
        assert_eq!(find(&tree, "cell-2-1").unwrap()["name"], "customer-2");

        // The next page reads its own shape: 40 rows over a page size of 25 leaves 15.
        // `update` marks the state dirty; a bare mutation would leave the committed
        // view, and so the a11y tree, reading the old page.
        runtime.update(|state, _| state.next_table_page());
        runtime.frame(0.0);
        let tree = runtime.accessibility_json();
        let grid = find(&tree, "data-grid-scroll").unwrap();
        assert_eq!(grid["value"], "15 rows × 8 columns");
        assert_eq!(grid["name"], "customers");
    }

    #[test]
    fn twenty_thousand_rows_mount_a_window_and_two_hundred_columns_build_one_window_of_cells() {
        use super::app_key;
        use datazen_ui_api::WorkspaceApi;
        use serde_json::Value;

        fn find<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
            if v["key"] == key {
                return Some(v);
            }
            v["children"].as_array()?.iter().find_map(|child| find(child, key))
        }
        fn walk<'a>(v: &'a Value, f: &mut dyn FnMut(&'a Value)) {
            f(v);
            if let Some(children) = v["children"].as_array() {
                for child in children {
                    walk(child, f);
                }
            }
        }

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        // The POC's scale as a loaded page: 20,000 rows × 200 columns. The
        // records carry no values — the grid's fallback reads r{r}c{c} — so
        // the page costs 20,000 empty records, not four million cells. The
        // surface stays the product table tab's; the sheet virtualizes the
        // 20,000-row list at the table page's size.
        let columns: Vec<String> = (0..200).map(|c| format!("c{c}")).collect();
        let records: Vec<Vec<String>> = (0..20_000).map(|_| Vec::new()).collect();
        let page = datazen_ui_api::TablePage {
            table: "customers".into(),
            columns,
            rows: records,
            page: 0,
            page_size: 20_000,
            total_rows: 20_000,
            sort: None,
            filter: None,
            read_only: None,
        };
        runtime.update(|state, _| state.grid.load_page(&page));
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.rows, 20_000);
        assert_eq!(runtime.state.grid.cols, 200);
        assert!(runtime.state.grid.has_page());

        let tree = runtime.accessibility_json();
        let scroll = find(&tree, "data-grid-scroll").expect("the grid is in the tree");
        assert_eq!(scroll["value"], "20000 rows × 200 columns");
        assert_eq!(scroll["name"], "customers");

        // One row node per mounted row; one cell node per built cell.
        let mut rows: Vec<u32> = Vec::new();
        let mut cells: Vec<(u32, u32)> = Vec::new();
        let mut headers: Vec<u32> = Vec::new();
        walk(scroll, &mut |node| {
            let Some(key) = node["key"].as_str() else { return };
            if let Some(row) = key.strip_prefix("grid-row-").and_then(|r| r.parse().ok()) {
                rows.push(row);
            } else if let Some(col) = key.strip_prefix("head-").and_then(|c| c.parse().ok()) {
                headers.push(col);
            } else if let Some((r, c)) = key
                .strip_prefix("cell-")
                .and_then(|rest| rest.split_once('-'))
                .and_then(|(r, c)| Some((r.parse().ok()?, c.parse().ok()?)))
            {
                cells.push((r, c));
            }
        });
        rows.sort_unstable();
        rows.dedup();
        headers.sort_unstable();
        headers.dedup();
        cells.sort_unstable();
        cells.dedup();

        // 20,000 rows mount a 21-row window at the top of the list: the first
        // frame's window over the 380-point sheet at the 28-point row height,
        // whatever the list's length — not the list.
        assert_eq!(
            rows,
            (0..21).collect::<Vec<u32>>(),
            "20,000 rows mount a 21-row window, not the list"
        );

        // A 200-column page builds one column window per row: 12 cells at rest —
        // COL_WINDOW with the frozen first column inside it — and 12 headers.
        assert_eq!(headers, (0..12).collect::<Vec<u32>>(), "the header builds the window");
        for row in rows.iter().copied() {
            let built: Vec<u32> = cells.iter().filter(|(r, _)| *r == row).map(|(_, c)| *c).collect();
            assert_eq!(built, (0..12).collect::<Vec<u32>>(), "row {row} builds the 12-column window");
        }
        assert!(cells.contains(&(0, 0)), "the focus cell is mounted");
    }

    #[test]
    fn the_unmigrated_areas_say_not_yet_migrated_and_keep_the_react_pages() {
        use super::app_key;
        use serde_json::Value;

        fn find<'a>(v: &'a Value, key: &str) -> Option<&'a Value> {
            if v["key"] == key {
                return Some(v);
            }
            v["children"].as_array()?.iter().find_map(|child| find(child, key))
        }
        fn names(v: &Value, out: &mut Vec<String>) {
            if let Some(name) = v["name"].as_str() {
                out.push(name.to_owned());
            }
            if let Some(children) = v["children"].as_array() {
                for child in children {
                    names(child, out);
                }
            }
        }

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        // The area is one of the sidebar's; pick it like the nav does.
        runtime.state.select(Section::Wapps);
        runtime.frame(0.0);

        let tree = runtime.accessibility_json();
        let nav = find(&tree, "nav-wapps").expect("the sidebar lists the area");
        assert_eq!(nav["role"], "tab");
        assert_eq!(nav["name"], "WAPPS");

        let page = find(&tree, "wapps-page").expect("the page is in the tree");
        assert!(find(&tree, "wapps-pending").is_some(), "the WAPPS entry is mounted");
        assert!(find(&tree, "onboarding-pending").is_some(), "the onboarding entry is mounted");

        // It says the status, and says where each area still lives.
        let mut texts = Vec::new();
        names(page, &mut texts);
        let joined = texts.join(" | ");
        assert!(joined.contains("Not yet migrated"), "the page says it: {joined}");
        assert!(joined.contains("The React page is still there"), "WAPPS points at React: {joined}");
        assert!(joined.contains("The React wizard is still there"), "onboarding points at React: {joined}");
        assert!(joined.contains("do not go into Rune"), "WAPP iframes stay out: {joined}");

        // And nowhere does the page claim a replacement or a removal.
        let lowered = joined.to_lowercase();
        assert!(!lowered.contains("replaced"), "no claim of replacement: {joined}");
        assert!(!lowered.contains("removed"), "no claim of removal: {joined}");
    }

    #[test]
    fn a_chinese_grid_cell_is_not_a_box_under_the_apps_fonts() {
        // The grid runs under the app's font pool; a Chinese cell value
        // renders with the glyphs the pool shaped above, not fallback boxes.
        let env = Env {
            system_dark: true,
            scale: 1.0,
        };
        let text = rune::text::TextSystem::new(APP_FONTS, APP_FONT_FAMILIES);
        let mut runtime =
            Runtime::with_text(AppState::default(), view, (1440.0, 900.0), env, text);
        use datazen_ui_api::WorkspaceApi;
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor = rune::TextEdit::new(String::new());
        runtime.state.grid.editor.insert("中文");
        runtime.state.commit_grid_edit();
        runtime.frame(0.0);

        assert_eq!(runtime.state.grid.cell(0, 1), "中文");
        let mut probe = rune::text::TextSystem::new(APP_FONTS, APP_FONT_FAMILIES);
        let ids = glyph_ids(&mut probe, "中文");
        assert!(ids.iter().all(|id| *id != 0), "tofu: {ids:?}");
    }

    #[test]
    fn shell_renders_the_datazen_navigation() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        assert!(runtime.find("datazen-root").is_some());
        for section in Section::ALL {
            let id = format!("nav-{}", section.key());
            assert!(runtime.find(&id).is_some(), "missing navigation node: {id}");
        }
    }

    #[test]
    fn the_sidebar_lists_only_connections_workspace_and_settings() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        for key in ["connections", "workspace", "settings"] {
            let id = format!("nav-{key}");
            assert!(runtime.find(&id).is_some(), "missing navigation node: {id}");
        }
        // Three of the four old placeholder pages are gone for good; workflows
        // returned as a real page with the W series.
        for key in ["sql-editor", "data-browser", "schema"] {
            let id = format!("nav-{key}");
            assert!(runtime.find(&id).is_none(), "{id} is not a product page anymore");
        }
    }

    #[test]
    fn clicking_navigation_changes_the_active_section() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime
            .find("nav-workspace")
            .expect("Workspace navigation item should exist");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0 / 60.0);

        assert_eq!(runtime.state.active, Section::Workspace);
    }

    #[test]
    fn a_workspace_without_a_tab_or_a_loaded_page_is_not_the_demo_grid() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime.find("nav-workspace").expect("workspace navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Workspace);
        assert!(runtime.find("workspace-empty").is_some(), "the workspace has an empty state");
        assert!(runtime.find("data-grid").is_none(), "the POC demo grid is not a product page");
        assert!(runtime.find("grid-row-0").is_none(), "no demo row is mounted");
        assert!(runtime.find("datazen-preview-card").is_none(), "the placeholder card is gone");
    }

    #[test]
    fn sql_editor_scrolls_a_surface_wider_than_the_viewport() {
        let mut runtime = runtime();
        runtime.state.open_query_tab("Query");
        runtime.frame(0.0);

        let field = runtime.presented("sql-editor-field").expect("editor field");
        let scroll = runtime.presented("sql-editor-scroll").expect("editor scroll");
        assert!(field.0[2] > scroll.0[2] + 100.0, "field {} viewport {}", field.0[2], scroll.0[2]);
    }

    #[test]
    fn chinese_ime_is_not_part_of_the_document_until_commit() {
        use super::sql_editor::on_key;
        use rune::{Key, KeyEvent, Mods};

        let mut runtime = runtime();
        runtime.on_key = Some(|state, ev, _| {
            state.active == Section::Workspace && query_selected(state) && on_key(&mut state.sql, ev)
        });
        runtime.state.open_query_tab("Query");
        runtime.state.sql.focus();
        runtime.frame(0.0);
        assert!(runtime.presented("sql-caret").is_some(), "main caret is the input-method cursor");

        let before = runtime.state.sql.doc().to_owned();
        runtime.state.sql.preedit("中");
        runtime.frame(0.0);
        assert_eq!(runtime.state.sql.doc(), before);
        assert_eq!(runtime.state.sql.composition(), Some("中"));

        runtime.state.sql.commit("中文");
        runtime.frame(0.0);
        assert!(runtime.state.sql.composition().is_none());
        assert!(runtime.state.sql.doc().starts_with("中文"), "{}", runtime.state.sql.doc());

        runtime.key(KeyEvent { key: Key::Text("Q".into()), mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.sql.doc().starts_with("中文Q"), "{}", runtime.state.sql.doc());
    }

    #[test]
    fn data_grid_mounts_a_row_window_and_a_column_window() {
        use super::app_poll;
        use datazen_ui_api::TablePage;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        let columns: Vec<String> = (0..200).map(|index| format!("c{index}")).collect();
        let rows: Vec<Vec<String>> = (0..6000)
            .map(|row| columns.iter().map(|col| format!("{col}-{row}")).collect())
            .collect();
        runtime.state.grid.load_page(&TablePage {
            table: "wide".into(),
            columns,
            rows,
            page: 0,
            page_size: 6000,
            total_rows: 6000,
            sort: None,
            filter: None,
            read_only: None,
        });
        runtime.state.active = Section::Workspace;
        runtime.frame(0.0);

        assert!(runtime.find("grid-row-0").is_some());
        assert!(runtime.find("cell-0-0").is_some());
        assert!(runtime.find("cell-0-11").is_some());
        assert!(runtime.find("grid-row-5000").is_none(), "rows far outside the viewport are not built");
        assert!(runtime.find("cell-0-40").is_none(), "columns outside the window are not built");

        let mut mounted = 0;
        for index in 0..200 {
            if runtime.find(&format!("grid-row-{index}")).is_some() {
                mounted += 1;
            }
        }
        assert!(mounted > 0 && mounted < 80, "mounted rows {mounted}");

        runtime.state.grid.scroll_x().set(super::data_grid::COL_W * 50.0);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.origin_col(), 50);
        assert!(runtime.find("cell-0-0").is_some(), "frozen column");
        assert!(runtime.find("cell-0-50").is_some());
        assert!(runtime.find("cell-0-11").is_none());
    }

    #[test]
    fn data_grid_edits_a_cell_and_copies_the_selection() {
        use super::data_grid::on_key;
        use rune::{Key, KeyEvent, Mods};

        let mut runtime = runtime();
        runtime.on_key = Some(|state, ev, tx| {
            state.active == Section::Workspace && !query_selected(state) && on_key(&mut state.grid, ev, tx)
        });
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").expect("connection row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("cell-0-1").expect("cell");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.focus(), (0, 1));

        runtime.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.grid.is_editing());
        // The second Enter is the production commit: the focused input's on_submit
        // commits the cell and blurs, so later Command keys still reach the grid.
        runtime.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        runtime.frame(0.0);
        assert!(!runtime.state.grid.is_editing());

        runtime.state.grid.paste_tsv("A\tB");
        assert_eq!(runtime.state.grid.cell(0, 1), "A");
        assert_eq!(runtime.state.grid.cell(0, 2), "B");
        runtime.key(KeyEvent { key: Key::Command('c'), mods: Mods::default() });
        runtime.frame(0.0);
        let copied = runtime.copy_request.as_ref().map(|copy| copy.content().text().to_owned());
        assert_eq!(copied.as_deref(), Some("A"));
    }

    #[test]
    fn opening_a_connection_loads_the_table_page_into_the_grid() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").expect("connection row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Workspace);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");
        assert_eq!(runtime.state.grid.column_name(1), Some("name"));
        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor.insert("-edited");
        runtime.state.commit_grid_edit();
        assert_eq!(
            runtime.state.workspace.open_table("customers", 0, TABLE_PAGE_SIZE, None, None).unwrap().rows[0][1],
            "customer-0-edited"
        );
    }

    #[test]
    fn opening_a_table_loads_the_first_page_of_the_table() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let grid = &runtime.state.grid;
        assert_eq!(grid.page(), 0);
        assert_eq!(grid.page_size(), TABLE_PAGE_SIZE);
        assert_eq!(grid.total_rows(), 40);
        assert_eq!(grid.rows, 25, "only the page is loaded, not the whole table");
        assert_eq!(grid.cell(24, 1), "customer-24");
        assert_eq!(grid.cell(0, 1), "customer-0");
    }

    #[test]
    fn the_footer_pages_through_the_table_and_stops_at_the_edges() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        assert!(runtime.find("grid-footer").is_some());
        assert!(runtime.find("page-prev").is_some());
        assert!(runtime.find("page-next").is_some());

        let (x, y) = runtime.find("page-prev").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 0, "prev on the first page is a no-op");

        let (x, y) = runtime.find("page-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1);
        assert_eq!(runtime.state.grid.rows, 15);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-25");

        let (x, y) = runtime.find("page-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1, "next on the last page is a no-op");

        let (x, y) = runtime.find("page-prev").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 0);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");
        assert!(runtime.find("cell-0-1").is_some(), "the frozen column is still built");
    }

    #[test]
    fn clicking_a_header_cycles_none_ascending_descending() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        // Page to the second page first: a sort must return to the first page.
        runtime.state.next_table_page();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1);

        let (x, y) = runtime.find("head-1").expect("name header");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 0, "sorting returns to the first page");
        assert_eq!(runtime.state.grid.sort(), Some((1, false)));
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");
        assert_eq!(runtime.state.grid.cell(2, 1), "customer-10", "lexicographic ascending order");

        let (x, y) = runtime.find("head-1").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.sort(), Some((1, true)));
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-9", "descending starts at the max");
        assert_eq!(runtime.state.grid.cell(6, 1), "customer-39");

        let (x, y) = runtime.find("head-1").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.sort(), None);
        assert_eq!(runtime.state.grid.cell(2, 1), "customer-2", "storage order returns");
    }

    #[test]
    fn the_filter_bar_narrows_the_page_and_resets_to_the_first_page() {
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        assert!(runtime.find("grid-filter").is_some());
        assert!(runtime.find("filter-column").is_some());
        assert!(runtime.find("filter-input").is_some());

        // Page to the second page first: applying a filter returns to the first page.
        runtime.state.next_table_page();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1);

        // The column chip cycles from id (column 0) to name (column 1).
        let (x, y) = runtime.find("filter-column").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.filter_column, 1);

        let (x, y) = runtime.find("filter-input").expect("filter input");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        runtime.key(KeyEvent { key: Key::Text("customer-3".into()), mods: Mods::default() });
        runtime.frame(0.0);
        runtime.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        runtime.frame(0.0);

        assert_eq!(runtime.state.grid.page(), 0, "filtering returns to the first page");
        assert_eq!(runtime.state.grid.total_rows(), 11, "customer-3 and customer-30..39");
        assert_eq!(runtime.state.grid.rows, 11);
        assert!(runtime.state.grid.cell(0, 1).contains("customer-3"));
        assert!(runtime.state.grid.cell(10, 1).contains("customer-3"));
        assert_eq!(runtime.state.filter_editor.focused, false, "submit blurs the filter input");

        // An empty editor clears the filter and brings the whole table back.
        runtime.state.filter_editor = rune::TextEdit::new(String::new());
        runtime.state.apply_filter();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.total_rows(), 40);
        assert_eq!(runtime.state.grid.rows, 25);
    }

    #[test]
    fn commit_and_rollback_buttons_drive_the_staged_edits() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let edit = |runtime: &mut rune::Runtime<AppState>| {
            runtime.state.grid.select(0, 1);
            runtime.state.grid.begin_edit();
            runtime.state.grid.editor = rune::TextEdit::new(String::new());
            runtime.state.grid.editor.insert("renamed");
            runtime.state.commit_grid_edit();
            runtime.frame(0.0);
        };

        edit(&mut runtime);
        assert_eq!(runtime.state.grid.cell(0, 1), "renamed");
        assert_eq!(runtime.state.grid.dirty_cells(), 1);

        let (x, y) = runtime.find("rollback-staged").expect("rollback button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0", "rollback restores the original");
        assert_eq!(runtime.state.grid.dirty_cells(), 0);

        edit(&mut runtime);
        let (x, y) = runtime.find("commit-staged").expect("commit button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.cell(0, 1), "renamed");
        assert_eq!(
            runtime
                .state
                .workspace
                .open_table("customers", 0, TABLE_PAGE_SIZE, None, None)
                .unwrap()
                .rows[0][1],
            "renamed",
            "committed values survive reopening the table"
        );
    }

    #[test]
    fn a_read_only_table_blocks_editing_and_shows_its_reason() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("orders").unwrap();
        runtime.frame(0.0);

        assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
        assert!(!runtime.state.grid.is_editable(), "a read-only page is not editable");
        runtime.state.grid.begin_edit();
        assert!(!runtime.state.grid.is_editing(), "a read-only page cannot enter the editor");
        assert!(runtime.find("grid-readonly").is_some(), "the reason banner is on the page");
        assert!(
            runtime.state.workspace.stage_cell(0, "name", "x".into()).is_err(),
            "staging through the port is rejected too"
        );

        // Back on a writable table the editor opens again.
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.state.grid.is_editable());
        assert!(runtime.find("grid-readonly").is_none(), "the banner leaves with the read-only page");
    }

    #[test]
    fn a_query_result_renders_in_the_shared_grid_and_cannot_stage() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.frame(0.0);
        assert!(runtime.find("query-results").is_some(), "no result yet");

        runtime.state.load_result_page(
            vec!["id".into(), "label".into()],
            vec![
                vec!["1".into(), "one".into()],
                vec!["2".into(), "two".into()],
                vec!["3".into(), "three".into()],
            ],
        );
        runtime.settle_all();
        runtime.frame(0.0);

        assert!(runtime.find("result-grid").is_some());
        assert!(runtime.find("result-cell-0-0").is_some());
        assert!(runtime.find("result-cell-2-1").is_some());
        assert!(runtime.find("query-results").is_none(), "the placeholder leaves");
        assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 1), "one");
        assert!(!runtime.state.result_grid.as_ref().unwrap().is_editable());
        assert!(
            runtime.state.workspace.stage_cell(0, "name", "x".into()).is_err(),
            "the port rejects staging while the result is open"
        );

        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("cell-0-1").is_some(), "the table grid keeps its own keys");
        assert!(
            runtime.state.workspace.stage_cell(0, "name", "ok".into()).is_ok(),
            "reopening a table releases the result lock"
        );
    }

    #[test]
    fn command_z_on_a_query_tab_undoes_and_redoes_typing() {
        use rune::{Key, KeyEvent, Mods};
        use super::app_key;

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.open_query_tab("Query").unwrap();
        runtime.frame(0.0);
        runtime.state.sql.focus();

        let before = runtime.state.sql.doc().to_owned();
        runtime.key(KeyEvent { key: Key::Text("Q".into()), mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.sql.doc().starts_with('Q'), "{}", runtime.state.sql.doc());

        runtime.key(KeyEvent { key: Key::Command('z'), mods: Mods::default() });
        runtime.frame(0.0);
        assert_eq!(runtime.state.sql.doc(), before, "undo returns the document");

        runtime.key(KeyEvent { key: Key::Command('z'), mods: Mods { shift: true, ..Mods::default() } });
        runtime.frame(0.0);
        assert!(runtime.state.sql.doc().starts_with('Q'), "shift-command-z redoes");
    }

    #[test]
    fn a_result_from_an_old_generation_is_dropped() {
        use datazen_ui_api::{QueryApi, WorkspaceApi};

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.frame(0.0);

        let first = runtime.state.next_query_generation();
        let second = runtime.state.next_query_generation();
        let stale = runtime.state.workspace.execute("SELECT * FROM CUSTOMERS").unwrap();
        runtime.state.deliver_query_result(first, stale);
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(
            runtime.state.result_grid.is_none(),
            "generation 1 arriving after generation 2 is ignored"
        );

        let fresh = runtime.state.workspace.execute("select 1").unwrap();
        runtime.state.deliver_query_result(second, fresh);
        runtime.settle_all();
        runtime.frame(0.0);
        let grid = runtime.state.result_grid.as_ref().expect("the current generation lands");
        assert_eq!(grid.cell(0, 0), "1");
        assert!(runtime.find("result-grid").is_some());
    }

    #[test]
    fn a_cancelled_generation_never_reaches_the_grid() {
        use datazen_ui_api::{QueryApi, WorkspaceApi};

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.frame(0.0);

        let generation = runtime.state.next_query_generation();
        runtime.state.cancel_query();
        let cancelled = runtime.state.workspace.execute("select 1").unwrap();
        runtime.state.deliver_query_result(generation, cancelled);
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(
            runtime.state.result_grid.is_none(),
            "the cancelled result does not enter the grid"
        );

        // A fresh run after the cancel still lands.
        let fresh = runtime.state.next_query_generation();
        let page = runtime.state.workspace.execute("select 1").unwrap();
        runtime.state.deliver_query_result(fresh, page);
        runtime.settle_all();
        runtime.frame(0.0);
        assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 0), "1");
    }

    #[test]
    fn the_run_button_fills_the_lower_half_with_the_result_grid() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
        runtime.frame(0.0);
        assert!(runtime.find("query-results").is_some(), "no result yet");

        let (x, y) = runtime.find("sql-run").expect("run button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("result-grid").is_some(), "the result lands in the lower half");
        assert!(runtime.find("result-cell-0-0").is_some(), "the returned cell is visible");
        assert!(runtime.find("query-results").is_none(), "the placeholder leaves");
        assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 0), "1");
    }

    #[test]
    fn command_enter_runs_the_statement_at_the_cursor() {
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};
        use super::app_key;

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql =
            sql_editor::SqlEditor::from_doc("SELECT * FROM CUSTOMERS; select 1");
        runtime.state.sql.focus();
        runtime.frame(0.0);

        // The caret sits at 0: the first statement runs.
        runtime.key(KeyEvent { key: Key::Enter, mods: Mods { cmd: true, ..Mods::default() } });
        runtime.frame(0.0);
        let grid = runtime.state.result_grid.as_ref().expect("first statement ran");
        assert_eq!(grid.rows, 40, "SELECT * FROM CUSTOMERS returns up to the query cap");
        assert_eq!(grid.cell(0, 1), "customer-0");

        // Move the caret into the second statement: only that one runs.
        let doc = runtime.state.sql.doc().to_owned();
        let second = doc.find("select 1").unwrap();
        runtime.state.sql.set_main_caret(second + 2);
        runtime.key(KeyEvent { key: Key::Enter, mods: Mods { cmd: true, ..Mods::default() } });
        runtime.frame(0.0);
        let grid = runtime.state.result_grid.as_ref().expect("second statement ran");
        assert_eq!(grid.rows, 1);
        assert_eq!(grid.cell(0, 0), "1");
    }

    #[test]
    fn a_failed_run_keeps_the_previous_result() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
        runtime.frame(0.0);

        let (x, y) = runtime.find("sql-run").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.result_grid.is_some());

        runtime.state.sql = sql_editor::SqlEditor::from_doc("drop table customers");
        let (x, y) = runtime.find("sql-run").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.result_grid.as_ref().unwrap().cell(0, 0),
            "1",
            "a failed execution leaves the interface as it was"
        );
    }

    #[test]
    fn the_cancel_button_drops_a_result_that_arrives_after_it() {
        use datazen_ui_api::{QueryApi, WorkspaceApi};

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
        runtime.frame(0.0);

        let (x, y) = runtime.find("sql-run").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.result_grid.as_ref().unwrap().cell(0, 0), "1");

        let generation = runtime.state.query_generation;
        let (x, y) = runtime.find("sql-cancel").expect("cancel button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        let late = runtime.state.workspace.execute("SELECT * FROM CUSTOMERS").unwrap();
        runtime.state.deliver_query_result(generation, late);
        runtime.settle_all();
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.result_grid.as_ref().unwrap().cell(0, 0),
            "1",
            "the cancelled delivery is dropped, the previous result stays"
        );
    }

    #[test]
    fn the_completion_list_is_drawn_in_the_canvas() {
        use datazen_ui_api::WorkspaceApi;
        use rune::{Key, KeyEvent, Mods};
        use super::app_key;

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("");
        runtime.state.sql.focus();
        runtime.frame(0.0);

        for ch in ["S", "E", "L"] {
            runtime.key(KeyEvent { key: Key::Text(ch.into()), mods: Mods::default() });
        }
        runtime.frame(0.0);
        assert!(runtime.find("sql-completion").is_some(), "the list is in the canvas");
        assert!(runtime.find("sql-completion-item-0").is_some());
        assert!(runtime.find("sql-editor-field").is_some());

        runtime.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
        runtime.frame(0.0);
        assert_eq!(runtime.state.sql.doc(), "SELECT");
        assert!(runtime.find("sql-completion").is_none(), "the list leaves after the insert");
    }

    #[test]
    fn the_tree_collapses_and_a_refresh_keeps_it_collapsed() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        assert!(runtime.find("tree-db-local").is_some());
        assert!(runtime.find("tree-schema-local-public").is_some());
        assert!(runtime.find("tree-schema-local-analytics").is_some());
        assert!(runtime.find("tree-table-customers").is_some());
        assert!(runtime.find("tree-table-catalog").is_some());

        let (x, y) = runtime.find("tree-schema-local-public").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("tree-table-customers").is_none(), "public is collapsed");
        assert!(runtime.find("tree-table-orders").is_none());
        assert!(runtime.find("tree-table-catalog").is_some(), "analytics stays open");

        let (x, y) = runtime.find("tree-refresh").expect("refresh button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime.find("tree-table-customers").is_none(),
            "still collapsed after the catalog refresh"
        );

        let (x, y) = runtime.find("tree-schema-local-public").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("tree-table-customers").is_some(), "expand brings them back");
    }

    #[test]
    fn collapsing_the_database_hides_the_whole_tree() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("tree-db-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("tree-schema-local-public").is_none());
        assert!(runtime.find("tree-table-customers").is_none());
        assert!(runtime.find("tree-db-local").is_some(), "the database row stays");

        let (x, y) = runtime.find("tree-db-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("tree-table-customers").is_some());
    }

    #[test]
    fn the_ddl_view_shows_the_port_string_and_an_empty_state() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        assert!(runtime.find("view-data").is_some());
        assert!(runtime.find("view-ddl").is_some());
        assert!(runtime.find("data-grid").is_some(), "data is the default view");

        let (x, y) = runtime.find("view-ddl").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("table-ddl").is_some(), "the DDL string has its own page");
        assert!(runtime.find("data-grid").is_none(), "the grid leaves");

        let (x, y) = runtime.find("view-data").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("data-grid").is_some());

        // The analytics table has no DDL: its own empty state.
        runtime.state.set_table_view(super::TableView::Ddl);
        runtime.state.open_table_tab("catalog").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("table-ddl-empty").is_some(), "an empty DDL string is an empty state");
        assert!(runtime.find("table-ddl").is_none());
    }

    #[test]
    fn the_columns_view_lists_one_row_per_column() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find("view-columns").expect("columns button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("columns-list").is_some());
        assert!(runtime.find("columns-header").is_some());
        for index in 0..8 {
            assert!(
                runtime.find(&format!("column-row-{index}")).is_some(),
                "one row per column, row {index}"
            );
        }
        assert!(runtime.find("column-row-8").is_none(), "customers has 8 columns");
        assert!(runtime.find("data-grid").is_none(), "the grid leaves for the list");
    }

    #[test]
    fn the_indexes_view_lists_the_indexes_with_ordered_columns() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find("view-indexes").expect("indexes button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("indexes-list").is_some());
        assert!(runtime.find("indexes-header").is_some());
        assert!(runtime.find("index-row-0").is_some());
        assert!(runtime.find("index-row-1").is_some());
        assert!(runtime.find("index-row-2").is_none(), "customers has two indexes");
        let indexes = runtime.state.workspace.table_indexes("customers").unwrap();
        assert_eq!(indexes[1].columns, vec!["name", "city"], "the compound order is in the row");
    }

    #[test]
    fn the_foreign_keys_view_keeps_compound_keys_together() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("orders").unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find("view-fks").expect("foreign keys button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("fks-list").is_some());
        assert!(runtime.find("fks-header").is_some());
        assert!(runtime.find("fk-row-0").is_some(), "orders has one foreign key");
        assert!(runtime.find("fk-row-1").is_none());
        let keys = runtime.state.workspace.table_foreign_keys("orders").unwrap();
        assert_eq!(keys[0].columns, vec!["id", "name"]);
        assert_eq!(keys[0].target_columns, vec!["id", "name"]);
    }

    #[test]
    fn the_er_page_draws_the_current_schema_and_collapses_wide_tables() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find("er-button").expect("er button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("er-page").is_some());
        assert!(runtime.find("er-canvas").is_some());
        assert!(runtime.find("er-box-customers").is_some(), "the current schema's tables");
        assert!(runtime.find("er-box-orders").is_some());
        assert!(runtime.find("er-col-customers-0").is_some(), "narrow tables list their columns");
        assert!(runtime.find("er-col-customers-7").is_some());
        assert!(runtime.find("er-box-catalog").is_none(), "analytics is another schema");

        // A wide table in its own schema collapses to the count.
        runtime.state.open_table_tab("catalog").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        let (x, y) = runtime.find("er-button").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("er-box-catalog").is_some());
        assert!(runtime.find("er-collapsed-catalog").is_some(), "30 columns collapse to the count");
        assert!(runtime.find("er-col-catalog-12").is_none(), "not every column is drawn");
    }

    #[test]
    fn er_boxes_drag_and_the_position_survives_rerenders() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        let (x, y) = runtime.find("er-button").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let before = runtime.presented("er-box-customers").expect("the box is drawn").0;
        let (x, y) = runtime.find("er-box-customers").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_move(x + 40.0, y + 20.0);
        runtime.pointer_move(x + 80.0, y + 50.0);
        runtime.pointer_up(x + 80.0, y + 50.0);
        runtime.frame(0.0);

        assert_eq!(
            runtime.state.er_positions.get("customers"),
            Some(&(80.0, 50.0)),
            "the drag moved the box by its travel"
        );
        let after = runtime.presented("er-box-customers").expect("still drawn").0;
        assert!(
            after[0] >= before[0] + 70.0 && after[1] >= before[1] + 40.0,
            "before {before:?} after {after:?}"
        );

        // A plain rerender keeps the position.
        runtime.frame(1.0);
        let kept = runtime.presented("er-box-customers").unwrap().0;
        assert!((kept[0] - after[0]).abs() < 1.0 && (kept[1] - after[1]).abs() < 1.0, "kept {kept:?}");
        assert!(runtime.state.er_drag.is_none(), "the gesture ended");
    }

    #[test]
    fn column_drafts_stay_separate_from_the_official_columns() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        let (x, y) = runtime.find("view-columns").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("column-drafts").is_some());
        assert!(runtime.find("column-row-8").is_none(), "8 official columns only");

        runtime.state.draft_name = rune::TextEdit::new("score");
        runtime.state.draft_type = rune::TextEdit::new("int");
        let (x, y) = runtime.find("draft-add").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("draft-row-0").is_some(), "the draft is queued");
        assert!(runtime.find("column-row-8").is_none(), "the official list is untouched");
        assert_eq!(
            runtime.state.workspace.table_columns("customers").unwrap().len(),
            8,
            "the port's columns are unchanged"
        );

        runtime.state.draft_name = rune::TextEdit::new("name");
        runtime.state.draft_type = rune::TextEdit::new("varchar(80)");
        let (x, y) = runtime.find("draft-retype").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("draft-row-1").is_some(), "the retype draft is queued too");
    }

    #[test]
    fn the_drafts_preview_as_sql_without_executing() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        let (x, y) = runtime.find("view-columns").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("draft-sql").is_none(), "no drafts, no preview");

        runtime.state.draft_name = rune::TextEdit::new("name");
        runtime.state.draft_type = rune::TextEdit::new("varchar(80)");
        let (x, y) = runtime.find("draft-retype").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("draft-sql").is_some(), "the preview appears");
        let drafts = runtime.state.column_drafts.get("customers").unwrap();
        assert_eq!(
            drafts[0].sql("customers"),
            "ALTER TABLE customers ALTER COLUMN name TYPE varchar(80);",
            "a retype draft shows its SQL"
        );
        assert_eq!(
            runtime.state.workspace.table_columns("customers").unwrap().len(),
            8,
            "the preview executed nothing"
        );
    }

    #[test]
    fn applying_the_drafts_lands_and_a_failure_keeps_them() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        let (x, y) = runtime.find("view-columns").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        runtime.state.draft_name = rune::TextEdit::new("score");
        runtime.state.draft_type = rune::TextEdit::new("int");
        let (x, y) = runtime.find("draft-add").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("draft-apply").expect("apply button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("draft-row-0").is_none(), "the drafts are gone");
        assert!(runtime.find("draft-sql").is_none());
        assert!(runtime.find("column-row-8").is_some(), "the official list grew");
        assert_eq!(
            runtime.state.workspace.table_columns("customers").unwrap()[8].name,
            "score"
        );
        assert_eq!(
            runtime.state.workspace.applied_ddl().len(),
            1,
            "the port recorded the apply"
        );

        // A read-only table refuses: the drafts stay, the official list is untouched.
        runtime.state.open_table_tab("orders").unwrap();
        runtime.settle_all();
        runtime.frame(1.0);
        runtime.state.draft_name = rune::TextEdit::new("x");
        runtime.state.draft_type = rune::TextEdit::new("int");
        let (x, y) = runtime.find("draft-add").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("draft-apply").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("draft-row-0").is_some(), "the failed apply keeps the drafts");
        assert!(runtime.find("column-row-2").is_none(), "the official columns are unchanged");
        assert_eq!(runtime.state.workspace.table_columns("orders").unwrap().len(), 2);
    }

    #[test]
    fn the_theme_toggles_through_the_port_and_survives_reopening() {
        use datazen_ui_api::{Setting, SettingsApi};

        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("settings-page").is_some());
        assert!(runtime.find("settings-theme").is_some());

        runtime
            .state
            .workspace
            .write_setting(Setting::Theme { dark: true })
            .unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find("settings-theme").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime
                .state
                .workspace
                .read_settings()
                .contains(&Setting::Theme { dark: false }),
            "the toggle wrote the port"
        );

        // Leave and come back: the setting is where the port keeps it.
        let (x, y) = runtime.find("nav-connections").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime
                .state
                .workspace
                .read_settings()
                .contains(&Setting::Theme { dark: false }),
            "reopening the page keeps the theme"
        );

        let (x, y) = runtime.find("settings-theme").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime
                .state
                .workspace
                .read_settings()
                .contains(&Setting::Theme { dark: true })
        );
    }

    #[test]
    fn the_page_size_setting_drives_the_grid_paging() {
        use datazen_ui_api::{Setting, SettingsApi, WorkspaceApi};

        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page_size(), 25, "the default");

        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.page_size_input = rune::TextEdit::new("10");
        let (x, y) = runtime.find("settings-page-size-set").expect("set button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime.state.workspace.read_settings().contains(&Setting::TablePageSize { rows: 10 }),
            "the write went through the port"
        );

        // The next page now turns at the new size.
        runtime.state.active = Section::Workspace;
        runtime.state.next_table_page();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.page(), 1);
        assert_eq!(runtime.state.grid.rows, 10, "the second page has 10 rows");
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-10");
        assert_eq!(runtime.state.grid.cell(9, 1), "customer-19");

        // An invalid value keeps the old setting and shows the error.
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.page_size_input = rune::TextEdit::new("0");
        let (x, y) = runtime.find("settings-page-size-set").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime.state.workspace.read_settings().contains(&Setting::TablePageSize { rows: 10 }),
            "the rejected write changed nothing"
        );
        assert!(runtime.find("settings-error").is_some(), "the rejection is visible");
    }

    #[test]
    fn a_connection_form_save_needs_an_address() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        assert!(runtime.find("connection-local").is_some());
        assert!(runtime.find("connection-form").is_some());

        let (x, y) = runtime.find("connection-save").expect("save button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("connection-remote-postgres").is_none(), "nothing was saved");
        assert!(runtime.find("connection-form-error").is_some(), "the refusal is visible");

        runtime.state.connection_name = rune::TextEdit::new("Remote Postgres");
        runtime.state.connection_engine = rune::TextEdit::new("postgres");
        runtime.state.connection_address = rune::TextEdit::new("db.example.com:5432");
        let (x, y) = runtime.find("connection-save").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime.find("connection-remote-postgres").is_some(),
            "the saved connection joins the list"
        );
        assert!(runtime.find("connection-form-error").is_none(), "the error clears");
    }

    #[test]
    fn snippets_add_update_delete_and_duplicate_names_are_refused() {
        use datazen_ui_api::{SettingsApi, WorkspaceApi};

        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("settings-snippets").is_some());

        runtime.state.snippet_name = rune::TextEdit::new("top");
        runtime.state.snippet_sql = rune::TextEdit::new("SELECT * FROM customers LIMIT 10");
        let (x, y) = runtime.find("snippet-save").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("snippet-row-0").is_some(), "the snippet is listed");

        runtime.state.snippet_name = rune::TextEdit::new("top");
        runtime.state.snippet_sql = rune::TextEdit::new("SELECT 1");
        let (x, y) = runtime.find("snippet-save").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("snippet-error").is_some(), "the duplicate name is refused");
        assert!(runtime.find("snippet-row-1").is_none(), "still one snippet");

        runtime.state.snippet_name = rune::TextEdit::new("top");
        runtime.state.snippet_sql = rune::TextEdit::new("SELECT 1");
        let (x, y) = runtime.find("snippet-update").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.workspace.snippets()[0].sql,
            "SELECT 1",
            "update changed the sql"
        );

        runtime.frame(1.0);
        let (x, y) = runtime.find("snippet-delete-0").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("snippet-row-0").is_none(), "the snippet is gone");
        assert!(runtime.state.workspace.snippets().is_empty());
    }

    #[test]
    fn an_export_without_a_range_gets_no_job() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("export-section").is_some());

        let (x, y) = runtime.find("export-start").expect("export button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.export_job.is_none(), "no range, no job");
        assert!(runtime.find("export-error").is_some(), "the refusal is visible");
        assert!(runtime.find("export-job").is_none());

        runtime.state.export_range = rune::TextEdit::new("customers");
        let (x, y) = runtime.find("export-start").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.export_job, Some(1), "the port handed out a job id");
        assert!(runtime.find("export-job").is_some());
        assert!(runtime.find("export-error").is_none());

        // The format chip cycles.
        let (x, y) = runtime.find("export-format").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.export_format(), "json");
    }

    #[test]
    fn a_failed_export_shows_its_error_line_from_the_snapshot() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        // A range naming no table fails on the third snapshot.
        runtime.state.export_range = rune::TextEdit::new("no-such-table");
        let (x, y) = runtime.find("export-start").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let snapshot = runtime.state.export_snapshot.clone().unwrap();
        assert_eq!(snapshot.status, "running");
        assert_eq!(snapshot.percent, 0);
        assert!(runtime.find("export-progress").is_some());
        assert!(runtime.find("export-log-0").is_some());

        let (x, y) = runtime.find("export-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.export_snapshot.as_ref().unwrap().percent, 50);

        runtime.frame(1.0);
        let (x, y) = runtime.find("export-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let snapshot = runtime.state.export_snapshot.clone().unwrap();
        assert_eq!(snapshot.status, "failed");
        assert_eq!(
            snapshot.log.last().map(String::as_str),
            Some("no such table no-such-table"),
            "the failure's error sentence is in the snapshot"
        );
        assert!(
            runtime.find(&format!("export-log-{}", snapshot.log.len() - 1)).is_some(),
            "the error line is drawn"
        );
        assert!(runtime.find("export-status").is_some());
    }

    #[test]
    fn the_import_wizard_needs_a_target_to_move_on() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("import-step-1").is_some());
        assert!(runtime.find("import-step-2").is_none());

        // Step 1 needs a source.
        let (x, y) = runtime.find("import-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("import-step-1").is_some(), "still on step 1");
        assert!(runtime.find("import-error").is_some());

        runtime.state.import_source = rune::TextEdit::new("dump.csv");
        let (x, y) = runtime.find("import-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("import-step-2").is_some(), "step 2 arrived");
        assert!(runtime.find("import-step-1").is_none());

        // Step 2 needs a target table: no target, no next step.
        let (x, y) = runtime.find("import-finish").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("import-step-2").is_some(), "still on step 2");
        assert!(runtime.find("import-ready").is_none());
        assert!(runtime.find("import-error").is_some(), "the refusal is visible");

        runtime.state.import_target = rune::TextEdit::new("customers");
        let (x, y) = runtime.find("import-finish").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("import-ready").is_some(), "both steps done");
    }

    #[test]
    fn a_backup_page_stops_and_shows_stopped() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("nav-settings").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("backup-section").is_some());

        let (x, y) = runtime.find("backup-start").expect("start button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let snapshot = runtime.state.backup_snapshot.clone().unwrap();
        assert_eq!(snapshot.status, "running");
        assert!(runtime.find("backup-progress").is_some());
        assert!(runtime.find("backup-log-1").is_some());

        let (x, y) = runtime.find("backup-stop").expect("stop button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let snapshot = runtime.state.backup_snapshot.clone().unwrap();
        assert_eq!(snapshot.status, "stopped", "after stopping the status is stopped");
        assert!(snapshot.log.last() == Some(&"stopped by the user".to_owned()));
        assert!(
            runtime.find(&format!("backup-log-{}", snapshot.log.len() - 1)).is_some(),
            "the stop line is drawn"
        );

        // A second stop is refused and visible.
        let (x, y) = runtime.find("backup-stop").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("backup-error").is_some());
    }

    #[test]
    fn the_sync_wizard_refuses_the_same_endpoint_twice() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-tools").expect("tools navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("tools-page").is_some());
        assert!(runtime.find("sync-tool").is_some());
        assert!(runtime.find("tool-compare").is_some());
        assert!(runtime.find("tool-transfer").is_some());

        // Cycle the target onto the source: both ends local.
        let (x, y) = runtime.find("sync-target").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_target, "local", "one connection away");

        let (x, y) = runtime.find("sync-endpoints-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_step, 0, "the same end twice does not advance");
        assert!(runtime.find("sync-error").is_some(), "the refusal is visible");

        // Back to distinct endpoints: the step advances.
        let (x, y) = runtime.find("sync-target").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_target, "replica");
        let (x, y) = runtime.find("sync-endpoints-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_step, 1);
        assert!(runtime.find("sync-error").is_none());
    }

    #[test]
    fn the_sync_table_step_selects_all_and_clears() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        runtime.state.sync_next_endpoints();
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("sync-step-2").is_some());
        assert!(runtime.state.sync_tables.is_empty());

        let (x, y) = runtime.find("sync-select-all").expect("select all");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_tables.len(), 3, "the local source sees three tables");

        let (x, y) = runtime.find("sync-clear").expect("clear");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.sync_tables.is_empty(), "clear empties the selection");

        // A single chip toggles one table; Next needs at least one.
        let (x, y) = runtime.find("sync-table-customers").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.sync_tables.contains("customers"));
        let (x, y) = runtime.find("sync-tables-next").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_step, 2);

        runtime.state.sync_clear_tables();
        runtime.state.sync_step = 1;
        runtime.state.sync_next_tables();
        assert_eq!(runtime.state.sync_step, 1, "no selection, no next step");
        assert!(runtime.state.sync_error.is_some());
    }

    #[test]
    fn the_sync_options_default_and_round_trip() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.sync_next_endpoints();
        runtime.state.sync_select_all();
        runtime.state.sync_next_tables();
        runtime.settle_all();
        runtime.frame(0.0);

        assert!(runtime.find("sync-step-3").is_some());
        assert_eq!(
            runtime.state.sync_options,
            [false, false, true],
            "defaults: no create, no drop, copy data on"
        );

        let (x, y) = runtime.find("sync-option-0").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_options, [true, false, true]);

        let (x, y) = runtime.find("sync-option-2").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_options, [true, false, false]);

        // Toggling back restores the defaults.
        let (x, y) = runtime.find("sync-option-0").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let (x, y) = runtime.find("sync-option-2").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.sync_options, [false, false, true], "the round trip restores the defaults");
    }

    #[test]
    fn the_sync_preview_is_a_read_only_grid() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.sync_next_endpoints();
        runtime.state.sync_tables.insert("customers".into());
        runtime.state.sync_next_tables();
        runtime.settle_all();
        runtime.frame(1.0);

        let (x, y) = runtime.find("sync-preview-next").expect("preview button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.settle_all();
        runtime.frame(0.0);

        let grid = runtime.state.sync_preview_grid.as_mut().expect("the preview grid");
        assert_eq!(grid.cell(0, 1), "customer-0");
        assert!(!grid.is_editable(), "the preview page cannot be edited");
        grid.begin_edit();
        assert!(!grid.is_editing(), "the editor refuses the preview");
        assert!(runtime.find("result-grid").is_some(), "drawn through the B8 sheet");
        assert!(runtime.find("result-cell-0-0").is_some());
    }

    #[test]
    fn the_sync_run_needs_tables_and_finishes_through_snapshots() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.sync_next_endpoints();
        runtime.state.sync_tables.insert("customers".into());
        runtime.state.sync_next_tables();
        runtime.state.sync_show_preview();
        runtime.settle_all();
        runtime.frame(1.0);

        // No selection, no run: clear the tables behind the wizard's back.
        runtime.state.sync_tables.clear();
        let (x, y) = runtime.find("sync-run").expect("run button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.sync_job.is_none(), "no tables selected, no job");
        assert!(runtime.find("sync-error").is_some());

        runtime.state.sync_tables.insert("customers".into());
        let (x, y) = runtime.find("sync-run").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.state.sync_job.is_some());
        assert_eq!(runtime.state.sync_snapshot.as_ref().unwrap().status, "running");

        let (x, y) = runtime.find("sync-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let (x, y) = runtime.find("sync-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.sync_snapshot.as_ref().unwrap().status,
            "completed",
            "the sync job finishes through snapshots"
        );
        assert!(runtime.find("sync-progress").is_some());
    }

    #[test]
    fn the_compare_step_marks_same_name_objects() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("tool-compare").expect("compare tab");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("compare-tool").is_some());
        assert!(runtime.find("compare-object-customers").is_some());
        assert!(runtime.find("compare-match-customers").is_some(), "same name on both ends");
        assert!(runtime.find("compare-match-orders").is_some());
        assert!(runtime.find("compare-missing-catalog").is_some(), "the replica lacks analytics");
    }

    #[test]
    fn an_empty_compare_plan_has_its_own_empty_state() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("tool-compare").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        assert!(runtime.find("compare-plan").is_some(), "local vs replica has a plan");
        assert!(runtime.find("compare-plan-empty").is_none());

        // The same endpoint twice: the plan is empty.
        let (x, y) = runtime.find("compare-target").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.compare_target, "local");
        assert!(runtime.find("compare-plan").is_none(), "an empty plan draws no lines");
        assert!(runtime.find("compare-plan-empty").is_some(), "the empty state shows");
    }

    #[test]
    fn a_failed_plan_apply_keeps_the_plan_visible() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("tool-compare").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("compare-plan").is_some());

        let (x, y) = runtime.find("compare-apply").expect("apply button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.state.compare_job.is_some());
        assert_eq!(runtime.state.compare_snapshot.as_ref().unwrap().status, "running");

        let (x, y) = runtime.find("compare-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        runtime.frame(1.0);
        let (x, y) = runtime.find("compare-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        let snapshot = runtime.state.compare_snapshot.clone().unwrap();
        assert_eq!(snapshot.status, "failed");
        assert_eq!(
            snapshot.log.last().map(String::as_str),
            Some("the replica cannot create analytics tables")
        );
        assert!(
            runtime.find("compare-plan").is_some(),
            "the failed apply did not clear the plan"
        );
        assert!(runtime.find("compare-progress").is_some());

        // An empty plan (same endpoints) refuses with an error and no job.
        let (x, y) = runtime.find("compare-target").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let (x, y) = runtime.find("compare-apply").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.compare_job.is_none());
        assert!(runtime.find("compare-error").is_some(), "nothing to apply is visible");
    }

    #[test]
    fn a_column_already_in_a_pair_cannot_pair_again() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("tool-transfer").expect("transfer tab");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("transfer-tool").is_some());

        // Pair id -> id.
        let (x, y) = runtime.find("transfer-pair").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("transfer-pair-row-0").is_some());
        assert!(!runtime.state.transfer_unmapped(true).contains(&"id".to_owned()));

        // A duplicate name cannot pair again: id is already used on both ends.
        let (x, y) = runtime.find("transfer-pair").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("transfer-pair-row-1").is_none(), "the duplicate pairing was refused");
        assert!(runtime.find("transfer-error").is_some(), "the refusal is visible");

        // A fresh pair still works and both unmapped lists shrink.
        runtime.state.transfer_pair_left = "name".into();
        runtime.state.transfer_pair_right = "name".into();
        let (x, y) = runtime.find("transfer-pair").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("transfer-pair-row-1").is_some());
        assert!(!runtime.state.transfer_unmapped(true).contains(&"name".to_owned()));
        assert!(runtime.find("transfer-unmapped-left").is_some());
        assert!(runtime.find("transfer-unmapped-right").is_some());
    }

    #[test]
    fn cancelling_the_transfer_confirmation_leaves_no_job() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-tools").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("tool-transfer").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        // No pairs: the confirmation refuses with no job.
        let (x, y) = runtime.find("transfer-start").expect("transfer button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("transfer-confirm").is_some());
        let (x, y) = runtime.find("transfer-yes").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.transfer_job.is_none(), "no pairs, no job");
        assert!(runtime.find("transfer-error").is_some());

        // With a pair: cancelling the confirmation leaves no job.
        runtime.frame(1.0);
        runtime.state.pair_transfer_columns();
        let (x, y) = runtime.find("transfer-start").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("transfer-no").expect("cancel button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.transfer_job.is_none(), "a cancelled confirmation gets no job");
        assert!(runtime.find("transfer-job").is_none());

        // Confirming hands out a job and the progress follows snapshots.
        runtime.frame(1.0);
        let (x, y) = runtime.find("transfer-start").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("transfer-yes").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.transfer_job.is_some(), "the port handed out a job id");
        assert_eq!(runtime.state.transfer_snapshot.as_ref().unwrap().status, "running");
        runtime.frame(1.0);
        let (x, y) = runtime.find("transfer-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let (x, y) = runtime.find("transfer-refresh").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.transfer_snapshot.as_ref().unwrap().status, "completed");
        assert!(runtime.find("transfer-progress").is_some());
    }

    #[test]
    fn an_empty_dashboard_has_its_own_empty_state() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-dashboards").expect("dashboards navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("dashboards-page").is_some());
        assert!(runtime.find("dashboard-empty").is_some(), "the empty list has its own state");
        assert!(runtime.find("widget-1").is_none());
    }

    #[test]
    fn a_bar_chart_shows_the_zero_and_the_maximum() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.frame(0.0);
        runtime
            .state
            .workspace
            .save_dashboard_widget("Weekly", "select 1")
            .unwrap();
        let (x, y) = runtime.find("nav-dashboards").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.settle_all();
        runtime.frame(1.0);

        assert!(runtime.find("widget-1").is_some());
        assert!(runtime.find("widget-chart-1").is_some());
        let zero = runtime.presented("widget-block-1-mon").expect("the zero bar is drawn");
        let max = runtime.presented("widget-block-1-thu").expect("the maximum bar is drawn");
        assert!(zero.0[3] >= 1.0, "the zero bar keeps a sliver: {:?}", zero);
        assert!(max.0[3] >= 80.0, "the maximum bar is tall: {:?}", max);
        assert!(max.0[3] > zero.0[3] + 60.0, "the chart is proportional");
    }

    #[test]
    fn a_widget_without_a_title_cannot_be_saved() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-dashboards").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("widget-form").is_some());

        let (x, y) = runtime.find("widget-save").expect("save button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("dashboard-empty").is_some(), "nothing was saved");
        assert!(runtime.find("widget-form-error").is_some(), "the refusal is visible");

        runtime.state.widget_title = rune::TextEdit::new("Weekly");
        runtime.state.widget_query = rune::TextEdit::new("select 1");
        let (x, y) = runtime.find("widget-save").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("widget-1").is_some(), "the widget joins the dashboard");
        assert!(runtime.find("widget-chart-1").is_some(), "its chart renders");
        assert!(runtime.find("widget-form-error").is_none());
    }

    #[test]
    fn the_workflow_list_is_sorted_by_name() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-workflows").expect("workflows navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("workflows-page").is_some());
        let audit = runtime.presented("workflow-audit").expect("audit is listed").0;
        let nightly = runtime.presented("workflow-nightly-backup").expect("nightly-backup is listed").0;
        let weekly = runtime.presented("workflow-weekly-report").expect("weekly-report is listed").0;
        assert!(
            audit[1] < nightly[1] && nightly[1] < weekly[1],
            "the list reads alphabetically: audit {audit:?} nightly {nightly:?} weekly {weekly:?}"
        );
    }

    #[test]
    fn the_first_workflow_step_cannot_move_up() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-workflows").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("workflow-audit").expect("a workflow row");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("workflow-form").is_some());
        assert!(runtime.find("step-row-0").is_some());
        assert!(runtime.find("step-row-1").is_some());

        // The first step cannot move up.
        let (x, y) = runtime.find("step-up-0").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.workflow_editing.as_ref().unwrap().1,
            vec!["compare structures".to_owned(), "apply plan".to_owned()],
            "the first step stayed put"
        );

        // Down moves it below the second; up brings it back.
        let (x, y) = runtime.find("step-down-0").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.workflow_editing.as_ref().unwrap().1,
            vec!["apply plan".to_owned(), "compare structures".to_owned()]
        );
        let (x, y) = runtime.find("step-up-1").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.workflow_editing.as_ref().unwrap().1,
            vec!["compare structures".to_owned(), "apply plan".to_owned()]
        );

        // The last step cannot move down either.
        let (x, y) = runtime.find("step-down-1").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.workflow_editing.as_ref().unwrap().1.len(), 2);

        // Save persists the order through the port.
        runtime.state.move_workflow_step(0, false);
        let (x, y) = runtime.find("workflow-save").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let audit = runtime
            .state
            .workspace
            .workflows()
            .into_iter()
            .find(|flow| flow.name == "audit")
            .unwrap();
        assert_eq!(audit.steps, vec!["apply plan".to_owned(), "compare structures".to_owned()]);
    }

    #[test]
    fn the_workflow_yaml_edits_and_undoes_through_the_document_model() {
        use rune::{Key, KeyEvent, Mods};
        use super::app_key;

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-workflows").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("workflow-audit").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        assert!(runtime.find("yaml-editor").is_some(), "the YAML surface is the editor model");
        let doc = runtime.state.workflow_yaml.doc().to_owned();
        assert!(doc.starts_with("name: audit\nsteps:\n"), "{doc}");
        assert!(doc.contains("  - compare structures\n"));
        // The plain panel shares the editor's line renderer (sql-line-* keys),
        // just without the SQL spans.
        assert!(runtime.find("sql-line-2").is_some(), "plain lines render");

        runtime.state.workflow_yaml.focus();
        runtime.key(KeyEvent { key: Key::Text("X".into()), mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.workflow_yaml.doc().starts_with('X'), "typing goes in");
        assert_ne!(runtime.state.workflow_yaml.doc(), doc);

        runtime.key(KeyEvent { key: Key::Command('z'), mods: Mods::default() });
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.workflow_yaml.doc(),
            doc,
            "Command+Z still undoes on the YAML document"
        );
    }

    #[test]
    fn workflow_run_results_separate_failures_from_successes() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-workflows").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        let (x, y) = runtime.find("workflow-audit").unwrap();
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("workflow-run").expect("run button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.find("run-results").is_some());
        assert!(runtime.find("run-result-0").is_some());
        assert!(runtime.find("run-ok-0").is_some(), "the first step succeeded");
        assert!(runtime.find("run-result-1").is_some());
        assert!(runtime.find("run-fail-1").is_some(), "the applying step failed");
        assert!(runtime.find("run-ok-1").is_none(), "a step is one or the other");
        let results = &runtime.state.workflow_results;
        assert_eq!(results[0].summary, "ok: 12 rows");
        assert_eq!(results[1].summary, "the replica refused the plan");
    }

    #[test]
    fn tab_strip_selects_and_closes_table_tabs() {
        let mut runtime = runtime();
        let first = runtime.state.open_table_tab("customers").unwrap();
        let second = runtime.state.open_table_tab("orders").unwrap();
        runtime.frame(0.0);

        let (x, y) = runtime.find(&format!("tab-{first}")).expect("first tab");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.tabs.selected().unwrap().id, first);

        let (x, y) = runtime.find(&format!("tab-close-{second}")).expect("close second");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.tabs.tabs().iter().all(|tab| tab.id != second));
        assert_eq!(runtime.state.tabs.selected().unwrap().id, first);
    }

    #[test]
    fn clicking_a_session_table_opens_a_table_tab() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        assert!(runtime.find("object-tree").is_some());
        assert!(runtime.find("tree-table-customers").is_none());

        let (x, y) = runtime.find("connection-local").expect("connection");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("tree-table-customers").expect("table in the tree");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.tabs.tabs().len(), 1);
        assert_eq!(runtime.state.tabs.selected().unwrap().title, "customers");
        assert_eq!(runtime.state.tabs.selected().unwrap().kind, datazen_ui_api::TabKind::Table);
    }

    #[test]
    fn the_open_table_tab_is_the_table_in_the_grid() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("connection-local").expect("connection");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        let (x, y) = runtime.find("tree-table-orders").expect("orders");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let orders = runtime.state.tabs.selected().unwrap().id;
        assert_eq!(runtime.state.grid.cell(0, 1), "order-0");

        let (x, y) = runtime.find("tree-table-customers").expect("customers");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        let customers = runtime.state.tabs.selected().unwrap().id;
        assert_eq!(runtime.state.grid.cell(0, 1), "customer-0");

        runtime.frame(1.0);
        let (x, y) = runtime.find(&format!("tab-{orders}")).expect("orders tab");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(runtime.state.tabs.selected().unwrap().id, orders);
        assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
        assert_ne!(customers, orders);
    }

    #[test]
    fn closing_the_selected_table_tab_follows_the_remaining_table() {
        use datazen_ui_api::WorkspaceApi;
        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        let orders = runtime.state.open_table_tab("orders").unwrap();
        let customers = runtime.state.open_table_tab("customers").unwrap();
        assert!(runtime.state.close_tab(customers));
        assert_eq!(runtime.state.tabs.selected().unwrap().id, orders);
        assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
    }

    #[test]
    fn a_padded_table_title_loads_the_trimmed_table() {
        use datazen_ui_api::WorkspaceApi;
        let mut runtime = runtime();
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.state.open_table_tab(" orders ").unwrap();
        assert_eq!(runtime.state.tabs.selected().unwrap().title, "orders");
        assert_eq!(runtime.state.grid.cell(0, 1), "order-0");
    }

    #[test]
    fn a_query_tab_keeps_the_editor_above_an_empty_result() {
        use rune::{Key, KeyEvent, Mods};
        use super::app_key;

        let mut runtime = runtime();
        runtime.on_key = Some(app_key);
        runtime.frame(0.0);
        let (x, y) = runtime.find("open-query").expect("new query");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);

        assert!(runtime.find("query-tab").is_some());
        assert!(runtime.find("sql-editor-field").is_some());
        assert!(runtime.find("query-results").is_some());
        assert_eq!(runtime.state.tabs.selected().unwrap().kind, datazen_ui_api::TabKind::Query);

        runtime.state.sql.focus();
        runtime.key(KeyEvent { key: Key::Text("select 1".into()), mods: Mods::default() });
        runtime.frame(0.0);
        assert!(runtime.state.sql.doc().starts_with("select 1"), "{}", runtime.state.sql.doc());
        assert!(runtime.find("query-results").is_some());
    }

    #[test]
    fn a_query_tab_stays_in_the_workspace_and_off_the_connections_page() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("open-query").expect("new query");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        assert!(runtime.find("sql-editor-field").is_some());
        assert!(runtime.find("tab-strip").is_some());

        let (x, y) = runtime.find("nav-connections").expect("connections navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Connections);
        assert!(runtime.find("connection-local").is_some());
        assert!(runtime.find("sql-editor-field").is_none(), "the editor belongs to the workspace");
        assert!(runtime.find("query-results").is_none(), "the results area belongs to the workspace");
        assert!(runtime.find("tab-strip").is_none(), "tabs are a workspace control");
        assert_eq!(runtime.state.tabs.tabs().len(), 1, "the query tab itself is kept");
    }

    #[test]
    fn the_unwired_desktop_adapter_shows_its_fixed_error_and_keeps_the_demo_list() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime.find("nav-connections").expect("connections navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        // The adapter is not wired yet: its one fixed sentence is on the page.
        let message = runtime
            .elements()
            .into_iter()
            .find(|el| el.key.as_deref() == Some("desktop-adapter-message"))
            .and_then(|el| el.text)
            .expect("the adapter's message is displayed");
        assert_eq!(message, datazen_ui_api::DESKTOP_NOT_IMPLEMENTED);

        // The memory port's demo list stays behind it.
        assert!(runtime.find("connection-local").is_some(), "the port's list is the fallback");
        assert!(runtime.state.desktop_connections.is_none(), "no adapter list replaced the port's");
        assert_eq!(
            runtime.state.desktop_error.as_deref(),
            Some(datazen_ui_api::DESKTOP_NOT_IMPLEMENTED)
        );
    }

    #[test]
    fn the_desktop_adapter_message_leaves_the_connections_page_with_the_section() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime.find("nav-connections").expect("connections navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.find("desktop-adapter-message").is_some());

        let (x, y) = runtime.find("nav-settings").expect("settings navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(
            runtime.find("desktop-adapter-message").is_none(),
            "the message belongs to the connections page"
        );
    }

    #[test]
    fn a_wired_adapter_replaces_the_demo_list_with_the_use_case_answer() {
        use datazen_ui_api::{ConnectionProfile, UseCaseAdapter};

        let mut runtime = runtime();
        // H3: the desktop process lists connections through the adapter; the
        // port's demo list is only the fallback.
        runtime.state.desktop = Box::new(UseCaseAdapter::new(vec![ConnectionProfile {
            id: "warehouse".into(),
            name: "Warehouse".into(),
            engine: "postgres".into(),
        }]));
        runtime.frame(0.0);

        let (x, y) = runtime.find("nav-connections").expect("connections navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(
            runtime.find("connection-warehouse").is_some(),
            "the use case's connection is on the page"
        );
        assert!(
            runtime.find("connection-local").is_none(),
            "the adapter's list replaced the port's demo list"
        );
        assert!(
            runtime.find("desktop-adapter-message").is_none(),
            "no fallback message: the adapter answered"
        );
        assert!(runtime.state.desktop_connections.is_some());
    }

    /// An adapter whose read use case serves one distinctive row, to see it
    /// win over the port's page in the wired path.
    struct AdapterServingOneRow;

    impl datazen_ui_api::DesktopAdapter for AdapterServingOneRow {
        fn list_connections(
            &mut self,
        ) -> Result<Vec<datazen_ui_api::ConnectionProfile>, String> {
            Err(datazen_ui_api::DESKTOP_NOT_IMPLEMENTED.into())
        }

        fn read_table_page(
            &mut self,
            table: &str,
            page: u32,
            _page_size: u32,
            _sort: Option<&datazen_ui_api::ColumnSort>,
            _filter: Option<&datazen_ui_api::ColumnFilter>,
        ) -> Result<datazen_ui_api::TablePage, String> {
            Ok(datazen_ui_api::TablePage {
                table: table.into(),
                columns: vec![String::from("adapter")],
                rows: vec![vec![String::from("from-adapter")]],
                page,
                page_size: 1,
                total_rows: 1,
                sort: None,
                filter: None,
                read_only: None,
            })
        }

        fn commit_cells(
            &mut self,
            _table: &str,
            _edits: &[(u32, String, String)],
        ) -> Result<u32, String> {
            Err(datazen_ui_api::DESKTOP_NOT_IMPLEMENTED.into())
        }
    }

    #[test]
    fn the_wired_adapter_serves_the_table_page_before_the_port() {
        let mut runtime = runtime();
        runtime.state.desktop = Box::new(AdapterServingOneRow);
        runtime.frame(0.0);

        let (x, y) = runtime.find("connection-local").expect("demo connection");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.state.grid.has_page(), "a page loaded");
        assert_eq!(
            runtime.state.grid.total_rows(),
            1,
            "the adapter's one-row page won, not the port's forty"
        );
        assert_eq!(runtime.state.active, Section::Workspace);
    }

    #[test]
    fn a_read_that_fails_everywhere_keeps_the_previous_grid_page() {
        let mut runtime = runtime();
        runtime.frame(0.0);

        let (x, y) = runtime.find("connection-local").expect("demo connection");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert!(runtime.state.grid.has_page());
        let (page, total) = (runtime.state.grid.page(), runtime.state.grid.total_rows());
        assert_eq!(total, 40, "the port answered the first read");

        // A table no use case knows: the adapter fails (it is unwired in the
        // tests), the port fails, and the grid keeps the page it had.
        assert!(
            !runtime.state.show_table("ghost"),
            "no read use case knows this table"
        );
        runtime.frame(0.0);
        assert!(
            runtime.state.grid.has_page(),
            "the failed read did not clear the grid"
        );
        assert_eq!(runtime.state.grid.page(), page, "the page number stays");
        assert_eq!(
            runtime.state.grid.total_rows(),
            total,
            "the previous page stays behind the failure"
        );
    }

    /// An adapter that serves a writable page but rejects every commit with a
    /// real error, to see the dirty marks survive the failure.
    struct AdapterFailingCommits;

    impl datazen_ui_api::DesktopAdapter for AdapterFailingCommits {
        fn list_connections(
            &mut self,
        ) -> Result<Vec<datazen_ui_api::ConnectionProfile>, String> {
            Err(datazen_ui_api::DESKTOP_NOT_IMPLEMENTED.into())
        }

        fn read_table_page(
            &mut self,
            table: &str,
            page: u32,
            _page_size: u32,
            _sort: Option<&datazen_ui_api::ColumnSort>,
            _filter: Option<&datazen_ui_api::ColumnFilter>,
        ) -> Result<datazen_ui_api::TablePage, String> {
            Ok(datazen_ui_api::TablePage {
                table: table.into(),
                columns: vec![String::from("id"), String::from("name")],
                rows: vec![
                    vec![String::from("0"), String::from("customer-0")],
                    vec![String::from("1"), String::from("customer-1")],
                ],
                page,
                page_size: 2,
                total_rows: 2,
                sort: None,
                filter: None,
                read_only: None,
            })
        }

        fn commit_cells(
            &mut self,
            _table: &str,
            _edits: &[(u32, String, String)],
        ) -> Result<u32, String> {
            Err("the desktop rejected the commit".into())
        }
    }

    #[test]
    fn a_failed_commit_keeps_the_dirty_marks() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.state.desktop = Box::new(AdapterFailingCommits);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);

        // One edited cell: the adapter's page is writable, the edit stages.
        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor = rune::TextEdit::new(String::new());
        runtime.state.grid.editor.insert("renamed");
        runtime.state.commit_grid_edit();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.cell(0, 1), "renamed");
        assert_eq!(runtime.state.grid.dirty_cells(), 1);

        // The commit fails with a real error: the dirty mark and the edit
        // stay exactly as they were.
        let (x, y) = runtime.find("commit-staged").expect("commit button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.grid.dirty_cells(),
            1,
            "the failure keeps the dirty mark"
        );
        assert_eq!(
            runtime.state.grid.cell(0, 1),
            "renamed",
            "the edit is still staged in the grid"
        );
    }

    #[test]
    fn a_wired_commit_lands_in_the_adapters_use_case_world() {
        use datazen_ui_api::{UseCaseAdapter, WorkspaceApi};

        let mut runtime = runtime();
        runtime.state.desktop = Box::new(UseCaseAdapter::with_demo_connections());
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.total_rows(), 40, "the adapter's world serves the page");

        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor = rune::TextEdit::new(String::new());
        runtime.state.grid.editor.insert("renamed");
        runtime.state.commit_grid_edit();
        runtime.frame(0.0);
        assert_eq!(runtime.state.grid.dirty_cells(), 1);

        let (x, y) = runtime.find("commit-staged").expect("commit button");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.grid.dirty_cells(),
            0,
            "the reload after the commit clears the marks"
        );
        assert_eq!(
            runtime.state.grid.cell(0, 1),
            "renamed",
            "the committed value reads back through the adapter's world"
        );
    }

    #[test]
    fn the_settings_page_is_its_own_page_not_the_old_preview_card() {
        let mut runtime = runtime();
        runtime.frame(0.0);
        let (x, y) = runtime.find("nav-settings").expect("settings navigation");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert_eq!(runtime.state.active, Section::Settings);
        assert!(runtime.find("settings-page").is_some(), "the settings page is real since P2");
        assert!(runtime.find("datazen-preview-card").is_none(), "the placeholder card is gone");
        assert!(runtime.find("connection-local").is_none(), "the connection list is its own page");
        assert!(runtime.find("workspace-empty").is_none(), "the workspace empty state is not here");
    }

    #[test]
    fn disconnect_clears_the_tree_and_blocks_editing() {
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.frame(0.0);
        assert!(runtime.find("disconnect").is_none());
        let (x, y) = runtime.find("connection-local").expect("connection");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(1.0);
        runtime.state.open_table_tab("customers").unwrap();
        assert!(runtime.state.grid.is_editable());

        runtime.frame(1.0);
        let (x, y) = runtime.find("disconnect").expect("disconnect");
        runtime.pointer_down(x, y);
        runtime.pointer_up(x, y);
        runtime.frame(0.0);

        assert!(runtime.state.workspace.session().is_none());
        assert!(runtime.state.tabs.tabs().is_empty());
        assert!(runtime.find("tree-table-customers").is_none());
        assert!(!runtime.state.grid.is_editable());
        runtime.state.grid.begin_edit();
        assert!(!runtime.state.grid.is_editing());
    }

    /// N1's test host: it takes every job and answers each on the poll after
    /// the next one — one frame of work, then the delivery. The canned pages
    /// carry the requesting generation, so one read's answer is tellable
    /// from another's.
    use datazen_ui_api::{
        AdapterDelivery, AdapterJob, ColumnFilter, ColumnSort, ConnectionProfile,
        DESKTOP_NOT_IMPLEMENTED, DesktopAdapter, TablePage,
    };

    struct JobAdapter {
        accepted: Vec<AdapterJob>,
        polls: u32,
        fail_commit: bool,
    }

    impl JobAdapter {
        fn new() -> Self {
            Self { accepted: Vec::new(), polls: 0, fail_commit: false }
        }

        fn failing_commit() -> Self {
            Self { fail_commit: true, ..Self::new() }
        }
    }

    impl DesktopAdapter for JobAdapter {
        fn list_connections(&mut self) -> Result<Vec<ConnectionProfile>, String> {
            Ok(Vec::new())
        }

        fn read_table_page(
            &mut self,
            _: &str,
            _: u32,
            _: u32,
            _: Option<&ColumnSort>,
            _: Option<&ColumnFilter>,
        ) -> Result<TablePage, String> {
            // A host that takes jobs must not be served synchronously.
            Err(DESKTOP_NOT_IMPLEMENTED.into())
        }

        fn commit_cells(&mut self, _: &str, _: &[(u32, String, String)]) -> Result<u32, String> {
            Err(DESKTOP_NOT_IMPLEMENTED.into())
        }

        fn begin_job(&mut self, job: AdapterJob) -> bool {
            self.accepted.push(job);
            true
        }

        fn poll_jobs(&mut self) -> Vec<AdapterDelivery> {
            self.polls += 1;
            if self.polls < 2 {
                return Vec::new();
            }
            self.polls = 0;
            let fail_commit = self.fail_commit;
            std::mem::take(&mut self.accepted)
                .into_iter()
                .map(|job| match job {
                    AdapterJob::ReadTable { generation, table, page, page_size, .. } => {
                        AdapterDelivery::TablePage {
                            generation,
                            page: Ok(TablePage {
                                table,
                                columns: vec!["job".into()],
                                rows: vec![vec![format!("read-{generation}")]],
                                page,
                                page_size,
                                total_rows: 1,
                                sort: None,
                                filter: None,
                                read_only: None,
                            }),
                        }
                    }
                    AdapterJob::CommitCells { generation, .. } => AdapterDelivery::CommitCount {
                        generation,
                        count: if fail_commit {
                            Err("rejected by the host".into())
                        } else {
                            Ok(1)
                        },
                    },
                    AdapterJob::RunQuery { generation, .. } => AdapterDelivery::QueryPage {
                        generation,
                        page: Ok(TablePage {
                            table: "result".into(),
                            columns: vec!["job".into()],
                            rows: vec![vec![format!("query-{generation}")]],
                            page: 0,
                            page_size: 1,
                            total_rows: 1,
                            sort: None,
                            filter: None,
                            read_only: None,
                        }),
                    },
                })
                .collect()
        }
    }

    #[test]
    fn an_accepted_read_job_keeps_the_previous_page_until_the_delivery_lands() {
        use super::app_poll;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        let before = runtime.state.grid.cell(0, 0).to_owned();
        assert!(runtime.state.grid.pages() > 1, "the demo table pages");

        // The host takes the page turn as a job: the marker shows, the page
        // the grid already loaded stays exactly as it was.
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.next_table_page();
        assert!(runtime.state.table_loading.is_some(), "the read job is in flight");
        assert_eq!(
            runtime.state.grid.cell(0, 0), before,
            "the previous page stays while the job runs"
        );
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("table-loading").is_some(), "the footer says the read is in flight");
        assert_eq!(runtime.state.grid.cell(0, 0), before, "still the previous page");

        // The delivery lands on the poll after the next one.
        runtime.frame(0.0);
        assert!(runtime.state.table_loading.is_none(), "the marker leaves with the delivery");
        assert_eq!(
            runtime.state.grid.cell(0, 0),
            format!("read-{}", runtime.state.read_generation),
            "the delivered page replaces the kept one"
        );
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("table-loading").is_none(), "the marker leaves the footer");
    }

    #[test]
    fn a_stale_read_delivery_is_dropped() {
        use super::app_poll;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);

        // Two reads in a row: the second supersedes the first, and both
        // answers arrive together — the stale one first.
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.reload_table_page();
        let stale = runtime.state.read_generation;
        runtime.state.reload_table_page();
        let current = runtime.state.read_generation;
        assert_ne!(stale, current, "the second read started its own generation");

        runtime.frame(0.0);
        assert!(runtime.state.table_loading.is_some(), "the newer read is in flight");
        runtime.frame(0.0);
        assert_eq!(
            runtime.state.grid.cell(0, 0),
            format!("read-{current}"),
            "only the current generation's page lands"
        );
        assert_ne!(
            runtime.state.grid.cell(0, 0),
            format!("read-{stale}"),
            "the superseded read does not overwrite it"
        );
        assert!(runtime.state.table_loading.is_none());
    }

    #[test]
    fn an_accepted_query_job_lands_in_the_result_grid_when_it_delivers() {
        use super::app_poll;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_query_tab("Query").unwrap();
        runtime.state.sql = sql_editor::SqlEditor::from_doc("select 1");
        runtime.settle_all();
        runtime.frame(0.0);

        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.run_query();
        let generation = runtime.state.query_generation;
        assert!(runtime.state.query_loading.is_some(), "the query job is in flight");
        assert!(runtime.state.result_grid.is_none(), "no result before the delivery");
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("query-loading").is_some(), "the result area says the query runs");

        runtime.frame(0.0);
        assert!(runtime.state.query_loading.is_none(), "the marker leaves with the delivery");
        let grid = runtime.state.result_grid.as_ref().expect("the delivery landed");
        assert_eq!(grid.cell(0, 0), format!("query-{generation}"));
    }

    #[test]
    fn an_accepted_commit_job_keeps_the_dirty_marks_until_the_delivery() {
        use super::app_poll;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor.insert("-edited");
        runtime.state.commit_grid_edit();
        assert_eq!(runtime.state.grid.dirty_cells(), 1, "one staged edit");

        // The host takes the batch as a job: the dirty marks stay and the
        // footer says the save is in flight.
        runtime.state.desktop = Box::new(JobAdapter::new());
        runtime.state.commit_staged_edits();
        assert!(runtime.state.commit_loading.is_some(), "the commit job is in flight");
        assert_eq!(runtime.state.grid.dirty_cells(), 1, "the dirty marks stay while it runs");
        runtime.settle_all();
        runtime.frame(0.0);
        assert!(runtime.find("commit-loading").is_some(), "the footer says the save is in flight");

        // The delivery lands: the marks clear through the reload the commit
        // asked for (itself a job on this host, delivered two polls later).
        runtime.frame(0.0);
        assert!(runtime.state.commit_loading.is_none(), "the commit marker leaves");
        assert!(runtime.state.table_loading.is_some(), "the reload after the commit is a read job");
        runtime.frame(0.0);
        runtime.frame(0.0);
        assert!(runtime.state.table_loading.is_none());
        assert_eq!(runtime.state.grid.dirty_cells(), 0, "the reloaded page carries no staged edits");
    }

    #[test]
    fn a_failed_commit_delivery_keeps_the_dirty_marks() {
        use super::app_poll;
        use datazen_ui_api::WorkspaceApi;

        let mut runtime = runtime();
        runtime.poll = Some(app_poll);
        runtime.state.workspace.connect("local").unwrap();
        runtime.state.open_table_tab("customers").unwrap();
        runtime.settle_all();
        runtime.frame(0.0);
        runtime.state.grid.select(0, 1);
        runtime.state.grid.begin_edit();
        runtime.state.grid.editor.insert("-edited");
        runtime.state.commit_grid_edit();
        let before = runtime.state.grid.cell(0, 1).to_owned();

        runtime.state.desktop = Box::new(JobAdapter::failing_commit());
        runtime.state.commit_staged_edits();
        runtime.frame(0.0);
        runtime.frame(0.0);

        assert!(runtime.state.commit_loading.is_none(), "the marker leaves with the failure");
        assert_eq!(runtime.state.grid.dirty_cells(), 1, "a real failure keeps the dirty marks");
        assert_eq!(
            runtime.state.grid.cell(0, 1), before,
            "no reload happened, the staged value is untouched"
        );
        assert!(runtime.state.table_loading.is_none(), "a failed commit does not reload");
    }
}
