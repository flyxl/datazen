//! Application state definitions.

use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use crate::types::{QuerySidebarPanel, ResultView, Section, TableView, Tool};

/// A3: one query result tab. Multiple executions produce multiple tabs; pinned
/// tabs stay across later runs.
#[derive(Debug)]
pub(crate) struct QueryResultTab {
    pub id: u32,
    pub label: String,
    pub generation: Option<u32>,
    pub grid: Option<DataGrid>,
    pub pinned: bool,
}
use crate::{icons, splitter};
#[cfg(not(target_arch = "wasm32"))]
use datazen_ui_api::UseCaseAdapter;
use datazen_ui_api::{
    AdapterDelivery, AdapterJob, Catalog, ColumnDraft, ColumnFilter, ColumnSort, ConnectionProfile,
    DashboardWidget, DesktopAdapter, MemoryWorkspace, ProgressSnapshot, QueryApi, Setting,
    SettingsApi, TabKind, TablePage, Tabs, UnimplementedAdapter, Workflow, WorkflowStepResult,
    WorkspaceApi, DESKTOP_NOT_IMPLEMENTED,
};
use rune::tok::{self, type_};
use rune::{
    col, oklch, row, text, text_input, App, AppSpec, Color, Cx, El, FileDialogOptions,
    FileDialogResult, Ime, Key, KeyEvent, TextEdit, Titlebar, Tx,
};

pub struct AppState {
    pub(crate) active: Section,
    pub(crate) sql: SqlEditor,
    pub(crate) grid: DataGrid,
    pub(crate) workspace: MemoryWorkspace,
    /// The desktop host's seam. H3 wired list_connections to the connection
    /// use case; the remaining methods answer the fixed error until H4/H5.
    pub(crate) desktop: Box<dyn DesktopAdapter>,
    /// The adapter's connection list once it answers (H3); None while the
    /// page falls back to the memory port's list.
    pub(crate) desktop_connections: Option<Vec<ConnectionProfile>>,
    /// The adapter's last error, displayed on the connections page.
    pub(crate) desktop_error: Option<String>,
    pub(crate) tabs: Tabs,
    /// The page the workspace last opened for the selected table tab.
    pub(crate) table_page: u32,
    /// The sort the table page is opened with.
    pub(crate) sort: Option<ColumnSort>,
    /// The filter the table page is opened with.
    pub(crate) filter: Option<ColumnFilter>,
    /// The column the filter bar targets, as an index into the loaded page.
    pub(crate) filter_column: u32,
    pub(crate) filter_editor: TextEdit,
    /// Navigator/object-tree filter text (A1: value search/filter).
    pub(crate) navigator_filter: TextEdit,
    /// Session-only recently opened connections (most recent first). UI state
    /// for A1 tree hierarchy; not persisted and not part of any contract.
    pub(crate) recent_connection_ids: Vec<String>,
    /// The read-only query result grid of the selected query tab (mirrors the
    /// selected result sub-tab for compatibility with existing view/focus paths).
    pub(crate) result_grid: Option<DataGrid>,
    /// T1: the result grid holds the keyboard after a result cell is pressed;
    /// typing, Escape or pressing the editor surface hands it back.
    pub(crate) result_focused: bool,
    /// A3: result sub-tabs for query executions (including pinned tabs).
    pub(crate) query_result_tabs: Vec<QueryResultTab>,
    pub(crate) selected_query_result: Option<u32>,
    pub(crate) next_query_result_id: u32,
    /// A3: favorites/history/snippets sidebar inside the query workspace.
    pub(crate) query_sidebar_panel: QuerySidebarPanel,
    /// A3: last honest notice from a toolbar action without a host capability.
    pub(crate) query_notice: Option<String>,
    /// A5: table/chart presentation of the query result workspace.
    pub(crate) result_view: ResultView,
    /// A5: the result find bar's search text.
    pub(crate) result_find: TextEdit,
    /// The query generation: bumped by every run and every cancel. A result page
    /// from a generation that is not the current one is dropped.
    pub(crate) query_generation: u32,
    /// The table read generation (N1): bumped by every read request. A job
    /// delivery from a generation that is not the current one is dropped.
    pub(crate) read_generation: u32,
    /// A read job the host accepted and has not answered yet (N1). While it
    /// is in flight the grid keeps its previous page and the footer says so.
    pub(crate) table_loading: Option<u32>,
    /// A commit job the host accepted and has not answered yet (N1). The
    /// dirty marks stay until the delivery lands.
    pub(crate) commit_loading: Option<u32>,
    /// A query job the host accepted and has not answered yet (N1).
    pub(crate) query_loading: Option<u32>,
    /// An open job the host accepted and has not answered yet (N3): the
    /// connections page says so until the catalog or the failure lands.
    pub(crate) connect_loading: Option<u32>,
    /// The live host's catalog once its open job answered one (N3); None
    /// means the memory port's catalog is the truth (the demo path).
    pub(crate) desktop_catalog: Option<datazen_ui_api::Catalog>,
    /// The live session's connection id (N3): the refresh path re-opens it.
    pub(crate) desktop_connection: Option<String>,
    /// Catalog nodes the tree keeps collapsed, by path: "db" or "db/schema".
    pub(crate) collapsed: std::collections::BTreeSet<String>,
    /// Which side of the selected table tab the workspace shows.
    pub(crate) table_view: TableView,
    /// The ER diagram's open schema, as (database, schema).
    pub(crate) er_schema: Option<(String, String)>,
    /// ER box positions per table, canvas points. UI state only, never the port.
    pub(crate) er_positions: std::collections::BTreeMap<String, (f32, f32)>,
    /// The box being dragged: its table and where the gesture started.
    pub(crate) er_drag: Option<(String, (f32, f32))>,
    /// Column-change drafts per table, kept until applied. UI state.
    pub(crate) column_drafts: std::collections::BTreeMap<String, Vec<ColumnDraft>>,
    pub(crate) draft_name: TextEdit,
    pub(crate) draft_type: TextEdit,
    /// The settings page's inputs and its last rejected write.
    pub(crate) page_size_input: TextEdit,
    pub(crate) max_rows_input: TextEdit,
    pub(crate) settings_error: Option<String>,
    /// Selected settings section (A6): grouped navigation + deep-link within
    /// the settings page. Addressable by key so a specific section can be opened.
    pub(crate) settings_section: String,
    /// The connection form's fields and its last rejected save.
    pub(crate) connection_dialog_open: bool,
    pub(crate) connection_name: TextEdit,
    pub(crate) connection_engine: TextEdit,
    pub(crate) connection_address: TextEdit,
    /// E5: the remote fields the live host maps onto its config; the
    /// memory port ignores them.
    pub(crate) connection_username: TextEdit,
    pub(crate) connection_password: TextEdit,
    pub(crate) connection_database: TextEdit,
    pub(crate) connection_error: Option<String>,
    /// UI-only SSH tunnel fields for A2 dialog alignment (not persisted; save uses draft only).
    pub(crate) connection_ssh_enabled: bool,
    pub(crate) connection_ssh_host: TextEdit,
    pub(crate) connection_ssh_port: TextEdit,
    pub(crate) connection_ssh_username: TextEdit,
    /// The snippet form's fields and its last rejected write.
    pub(crate) snippet_name: TextEdit,
    pub(crate) snippet_sql: TextEdit,
    pub(crate) snippet_error: Option<String>,
    /// T6: the favorites form's title input and its last rejected write. The
    /// statement comes from the current query editor — "save current SQL".
    pub(crate) favorite_title: TextEdit,
    pub(crate) favorite_error: Option<String>,
    /// T7: the parsed connection import waiting on its confirm, and the
    /// import path's outcome notice. The password serves the encrypted
    /// export formats; the plain ones ignore it.
    pub(crate) import_password: TextEdit,
    pub(crate) import_preview: Option<datazen_ui_api::ImportPreview>,
    pub(crate) import_notice: Option<String>,
    /// T4: the value search's form and state. The value and its mode ride
    /// the job; the hits list is the scan's answer.
    pub(crate) value_search_value: TextEdit,
    pub(crate) value_search_mode: u32,
    pub(crate) value_search_generation: u32,
    pub(crate) value_search_loading: Option<u32>,
    pub(crate) value_search_error: Option<String>,
    pub(crate) value_search_hits: Vec<datazen_ui_api::ValueSearchHit>,
    /// The value the loaded hits answered, so a locate filters by what was
    /// searched, not by whatever the input holds since.
    pub(crate) value_search_query: String,
    /// T5: the loaded table's inbound references (None until asked for).
    pub(crate) inbound_refs: Option<Vec<datazen_ui_api::InboundRef>>,
    pub(crate) fk_error: Option<String>,
    /// A4: FK navigation breadcrumb trail (followed foreign-key jumps).
    pub(crate) fk_breadcrumbs: Vec<FkBreadcrumb>,
    /// E3: the inbound generation — bumped by every ask. A delivery from a
    /// scan the state has moved past is dropped.
    pub(crate) inbound_generation: u32,
    /// E3: an inbound job the host accepted and has not answered yet.
    pub(crate) inbound_loading: Option<u32>,
    /// E1: the structure generation — bumped by every structure request (a
    /// table open, a view switch, an ER open). A structure delivery from a
    /// generation the table's entry has moved past is dropped.
    pub(crate) structure_generation: u32,
    /// E1: the loaded structures, per table — the four-part answer the
    /// structure views and the outbound FK navigation read. Render
    /// functions read this map only, never the adapter.
    pub(crate) structures: std::collections::BTreeMap<String, StructureState>,
    /// E2: the widget-bars generation — bumped by every bars request (a
    /// dashboards open, a widget save, a manual refresh).
    pub(crate) widget_bars_generation: u32,
    /// E2: the loaded bars, per widget id — the bar chart's only read.
    /// Render never queries the widget's data live.
    pub(crate) widget_bars: std::collections::BTreeMap<u64, WidgetBarsState>,
    /// The export dialog's format, range and its job.
    pub(crate) export_format_index: u32,
    pub(crate) export_range: TextEdit,
    pub(crate) export_job: Option<u64>,
    pub(crate) export_error: Option<String>,
    /// The export job's last snapshot. Refreshed only on demand.
    pub(crate) export_snapshot: Option<ProgressSnapshot>,
    /// The import wizard: 0 source step, 1 target step, 2 ready.
    pub(crate) import_step: u32,
    pub(crate) import_source: TextEdit,
    pub(crate) import_target: TextEdit,
    pub(crate) import_error: Option<String>,
    /// The backup job and its last snapshot.
    pub(crate) backup_job: Option<u64>,
    pub(crate) backup_snapshot: Option<ProgressSnapshot>,
    pub(crate) backup_error: Option<String>,
    /// The tools page's active tool.
    pub(crate) tool: Tool,
    /// The sync wizard: endpoints, tables, options, preview, run.
    pub(crate) sync_step: u32,
    pub(crate) sync_source: String,
    pub(crate) sync_target: String,
    pub(crate) sync_tables: std::collections::BTreeSet<String>,
    pub(crate) sync_options: [bool; 3],
    pub(crate) sync_error: Option<String>,
    /// The sync preview grid (read-only, the B8 sheet).
    pub(crate) sync_preview_grid: Option<DataGrid>,
    /// The sync run job and its last snapshot.
    pub(crate) sync_job: Option<u64>,
    pub(crate) sync_snapshot: Option<ProgressSnapshot>,
    /// The compare tool's endpoints.
    pub(crate) compare_source: String,
    pub(crate) compare_target: String,
    /// The compare plan's apply job and its last snapshot.
    pub(crate) compare_job: Option<u64>,
    pub(crate) compare_snapshot: Option<ProgressSnapshot>,
    pub(crate) compare_error: Option<String>,
    /// The transfer tool's tables, pairing chips and the pairs themselves.
    pub(crate) transfer_left: String,
    pub(crate) transfer_right: String,
    pub(crate) transfer_pair_left: String,
    pub(crate) transfer_pair_right: String,
    pub(crate) transfer_pairs: Vec<(String, String)>,
    pub(crate) transfer_error: Option<String>,
    /// The transfer's confirmation step and its job.
    pub(crate) transfer_confirming: bool,
    pub(crate) transfer_job: Option<u64>,
    pub(crate) transfer_snapshot: Option<ProgressSnapshot>,
    /// The widget form's fields and its last rejected save.
    pub(crate) widget_title: TextEdit,
    pub(crate) widget_query: TextEdit,
    pub(crate) widget_error: Option<String>,
    /// The workflow being edited: its name and the step list being reordered.
    pub(crate) workflow_editing: Option<(String, Vec<String>)>,
    /// The workflow's YAML, edited through the SQL editor's document model.
    pub(crate) workflow_yaml: SqlEditor,
    /// The last workflow run's results.
    pub(crate) workflow_results: Vec<WorkflowStepResult>,
    /// T8: the workflow run's generation and its in-flight marker. The host's
    /// engine runs as a job; a delivery from a generation the state has moved
    /// past is dropped.
    pub(crate) workflow_generation: u32,
    pub(crate) workflow_loading: Option<u32>,
    /// Live size for the splitter being dragged (L2): (region key, points).
    /// While `Some`, `splitter_size` returns this instead of the persisted
    /// value; a cancelled or finished drag clears it.
    pub(crate) splitter_preview: Option<(String, u32)>,
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
            navigator_filter: TextEdit::new(String::new()),
            recent_connection_ids: Vec::new(),
            result_grid: None,
            result_focused: false,
            query_result_tabs: Vec::new(),
            selected_query_result: None,
            next_query_result_id: 1,
            query_sidebar_panel: QuerySidebarPanel::Favorites,
            query_notice: None,
            result_view: ResultView::Table,
            result_find: TextEdit::new(String::new()),
            query_generation: 0,
            read_generation: 0,
            table_loading: None,
            commit_loading: None,
            query_loading: None,
            connect_loading: None,
            desktop_catalog: None,
            desktop_connection: None,
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
            settings_section: "general".into(),
            connection_dialog_open: false,
            connection_name: TextEdit::new(String::new()),
            connection_engine: TextEdit::new(String::new()),
            connection_address: TextEdit::new(String::new()),
            connection_username: TextEdit::new(String::new()),
            connection_password: TextEdit::new(String::new()),
            connection_database: TextEdit::new(String::new()),
            connection_error: None,
            connection_ssh_enabled: false,
            connection_ssh_host: TextEdit::new(String::new()),
            connection_ssh_port: TextEdit::new(String::from("22")),
            connection_ssh_username: TextEdit::new(String::new()),
            snippet_name: TextEdit::new(String::new()),
            snippet_sql: TextEdit::new(String::new()),
            snippet_error: None,
            favorite_title: TextEdit::new(String::new()),
            favorite_error: None,
            import_password: TextEdit::new(String::new()),
            import_preview: None,
            import_notice: None,
            value_search_value: TextEdit::new(String::new()),
            value_search_mode: 0,
            value_search_generation: 0,
            value_search_loading: None,
            value_search_error: None,
            value_search_hits: Vec::new(),
            value_search_query: String::new(),
            inbound_refs: None,
            fk_error: None,
            fk_breadcrumbs: Vec::new(),
            inbound_generation: 0,
            inbound_loading: None,
            structure_generation: 0,
            structures: std::collections::BTreeMap::new(),
            widget_bars_generation: 0,
            widget_bars: std::collections::BTreeMap::new(),
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
            workflow_generation: 0,
            workflow_loading: None,
            splitter_preview: None,
        }
    }
}

/// One table's structure read state (E1): the generation the entry waits
/// on, its loading and error marks, and the four-part answer once it
/// lands. Render functions read the data; the honest states say
/// themselves instead of drawing a phantom empty table.
pub(crate) struct StructureState {
    pub(crate) generation: u32,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) data: Option<datazen_ui_api::TableStructure>,
}

/// One widget's bars read state (E2): the generation the entry waits on,
/// its loading and error marks, and the chart's answer once it lands.
pub(crate) struct WidgetBarsState {
    pub(crate) generation: u32,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) bars: Option<Vec<(String, u32)>>,
}

/// A4: one step in the FK navigation breadcrumb trail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct FkBreadcrumb {
    pub(crate) label: String,
    pub(crate) table: String,
    pub(crate) page: u32,
    pub(crate) sort: Option<datazen_ui_api::ColumnSort>,
    pub(crate) filter: Option<datazen_ui_api::ColumnFilter>,
}
