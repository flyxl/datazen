//! Application state definitions.

use crate::components::ai::{AiChatPanelState, AiPendingDraft, AiWriteProposalHostState};
use crate::data_grid::DataGrid;
use crate::sql_editor::SqlEditor;
use crate::types::{QuerySidebarPanel, ResultView, Section, TableView, Tool};
use crate::views::workflow_page::{
    WorkflowConfirmRequest, WorkflowContextMenuRequest, WorkflowEditorMode, WorkflowListItem,
    WorkflowPanel, WorkflowSideTab, WorkflowStepOrder,
};
use crate::views::ConnectionHomeSlotVm;
use crate::views::HistoryListItem;
use crate::views::WorkflowExecutionResult;
use crate::views::{
    InstallWappStep, RunningExecutionCloseRequest, WappFilter, WappPackagePreview,
    WappRemoveRequest, WappSummaryVm,
};
use crate::views::{
    WorkflowCommandDefinition, WorkflowConnectionOption, WorkflowDraft, WorkflowFormSelect,
};
use std::cell::Cell;

/// A3: one query result tab. Multiple executions produce multiple tabs; pinned
/// tabs stay across later runs.
#[derive(Debug)]
pub struct QueryResultTab {
    pub id: u32,
    pub label: String,
    pub generation: Option<u32>,
    pub execution_stats: Option<datazen_ui_api::QueryExecutionStats>,
    pub grid: Option<DataGrid>,
    pub pinned: bool,
}
use crate::{icons, splitter};
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
    pub active: Section,
    pub sql: SqlEditor,
    pub grid: DataGrid,
    pub workspace: MemoryWorkspace,
    /// The desktop host's seam. H3 wired list_connections to the connection
    /// use case; the remaining methods answer the fixed error until H4/H5.
    pub desktop: Box<dyn DesktopAdapter>,
    /// The adapter's connection list once it answers (H3); None while the
    /// page falls back to the memory port's list — never answered, or
    /// unwired. A failed re-fetch keeps the previous list (the zustand
    /// store's `connections` survives a failed fetch).
    pub desktop_connections: Option<Vec<ConnectionProfile>>,
    /// The adapter's last error, displayed on the connections page.
    pub desktop_error: Option<String>,
    /// T020: the connection store's list-fetch error — React
    /// `useConnectionStore`'s `error`, read by the main window's load-failure
    /// gate (`MainPage.tsx:112`). Only the list fetch (the adapter sync)
    /// writes it; the shared adapter-notice slot [`Self::desktop_error`]
    /// keeps collecting every failure the page shows as a banner, including
    /// open-job sentences, which never gate the window.
    pub connections_load_error: Option<String>,
    /// The value-presentation registry (issue-32): filled by the shared
    /// bootstrap chain (`start_shared_app_state`) with the bundled
    /// contributions; the settings/snapshot layer reads it.
    pub value_presentation_registry:
        crate::support::value_presentation::registry::ValuePresentationRegistry,
    pub tabs: Tabs,
    /// The page the workspace last opened for the selected table tab.
    pub table_page: u32,
    /// The sort the table page is opened with.
    pub sort: Option<ColumnSort>,
    /// The filter the table page is opened with.
    pub filter: Option<ColumnFilter>,
    /// The column the filter bar targets, as an index into the loaded page.
    pub filter_column: u32,
    pub filter_editor: TextEdit,
    /// Navigator/object-tree filter text (A1: value search/filter).
    pub navigator_filter: TextEdit,
    /// Session-only recently opened connections (most recent first). UI state
    /// for A1 tree hierarchy; not persisted and not part of any contract.
    pub recent_connection_ids: Vec<String>,
    /// The read-only query result grid of the selected query tab (mirrors the
    /// selected result sub-tab for compatibility with existing view/focus paths).
    pub result_grid: Option<DataGrid>,
    /// T1: the result grid holds the keyboard after a result cell is pressed;
    /// typing, Escape or pressing the editor surface hands it back.
    pub result_focused: bool,
    /// A3: result sub-tabs for query executions (including pinned tabs).
    pub query_result_tabs: Vec<QueryResultTab>,
    pub selected_query_result: Option<u32>,
    pub next_query_result_id: u32,
    /// A3: favorites/history/snippets sidebar inside the query workspace.
    pub query_sidebar_panel: QuerySidebarPanel,
    pub(crate) query_history: crate::query_history_state::QueryHistoryState,
    /// A3: last honest notice from a toolbar action without a host capability.
    pub query_notice: Option<String>,
    /// A5: table/chart presentation of the query result workspace.
    pub result_view: ResultView,
    /// A5: the result find bar's search text.
    pub result_find: TextEdit,
    /// The query generation: bumped by every run and every cancel. A result page
    /// from a generation that is not the current one is dropped.
    pub query_generation: u32,
    /// The table read generation (N1): bumped by every read request. A job
    /// delivery from a generation that is not the current one is dropped.
    pub read_generation: u32,
    /// A read job the host accepted and has not answered yet (N1). While it
    /// is in flight the grid keeps its previous page and the footer says so.
    pub table_loading: Option<u32>,
    pub(crate) table_read_error: Option<String>,
    /// A commit job the host accepted and has not answered yet (N1). The
    /// dirty marks stay until the delivery lands.
    pub commit_loading: Option<u32>,
    pub(crate) edit_preview: crate::native_edit_preview::NativeEditPreview,
    /// A query job the host accepted and has not answered yet (N1).
    pub query_loading: Option<u32>,
    pub(crate) native_query: Option<crate::native_query_close::NativeQuery>,
    /// An open job the host accepted and has not answered yet (N3): the
    /// connections page says so until the catalog or the failure lands.
    pub connect_loading: Option<u32>,
    /// The live host's catalog once its open job answered one (N3); None
    /// means the memory port's catalog is the truth (the demo path).
    pub desktop_catalog: Option<datazen_ui_api::Catalog>,
    pub(crate) native_kv: Option<crate::views::native_kv::KvWorkbench>,
    pub(crate) native_kv_unused_input: rune::TextEdit,
    /// The live session's connection id (N3): the refresh path re-opens it.
    pub desktop_connection: Option<String>,
    /// Catalog nodes the tree keeps collapsed, by path: "db" or "db/schema".
    pub collapsed: std::collections::BTreeSet<String>,
    /// Which side of the selected table tab the workspace shows.
    pub table_view: TableView,
    /// The ER diagram's open schema, as (database, schema).
    pub er_schema: Option<(String, String)>,
    pub er_selected: Option<String>,
    pub er_search: TextEdit,
    pub er_zoom: f32,
    /// ER box positions per table, canvas points. UI state only, never the port.
    pub er_positions: std::collections::BTreeMap<String, (f32, f32)>,
    /// The box being dragged: its table and where the gesture started.
    pub er_drag: Option<(String, (f32, f32))>,
    /// Column-change drafts per table, kept until applied. UI state.
    pub column_drafts: std::collections::BTreeMap<String, Vec<ColumnDraft>>,
    pub draft_name: TextEdit,
    pub draft_type: TextEdit,
    /// The settings page's inputs and its last rejected write.
    pub page_size_input: TextEdit,
    pub max_rows_input: TextEdit,
    pub settings_error: Option<String>,
    /// Selected settings section (A6): grouped navigation + deep-link within
    /// the settings page. Addressable by key so a specific section can be opened.
    pub(crate) settings_content: crate::views::settings_content_state::SettingsContentState,
    pub(crate) appearance_select: Option<usize>,
    /// A1: the connections home's filter input (name/engine/id search).
    pub connection_search: TextEdit,
    /// A1: the connections card list's expanded state (show all / show less).
    pub connections_show_all: bool,
    /// T144: which home row owns the copy marker, and its real runtime deadline
    /// (seconds). Cleared by `home_copy_tick` in the app poll; not a boolean.
    pub home_copied_id: Option<String>,
    pub home_copied_until: Option<f64>,
    /// T144: a driver-contributed connection home (React `KvConnectionHomeBinding`).
    /// The host sets it only for a live session; removal stage: real driver slot.
    pub home_slot: Option<ConnectionHomeSlotVm>,
    /// The connection form dialog is open over the home.
    pub connection_dialog_open: bool,
    /// The generation of a connection dialog open (`Dialog::open_epoch`):
    /// bumped by `open_connection_dialog`, so a reopened dialog's body starts
    /// at the top instead of at the offset it had before it closed.
    pub connection_dialog_epoch: u64,
    /// The import dialog (the React share dialog's import mode) is open.
    pub import_dialog_open: bool,
    /// The generation of an import dialog open; see
    /// [`Self::connection_dialog_epoch`].
    pub import_dialog_epoch: u64,
    /// The export file the import dialog picked (None until chosen).
    pub import_file: Option<String>,
    /// T022: the install-wapp dialog (React `InstallWappDialog.tsx`) is open
    /// over the window; the root modal layer (`views/modals.rs`) mounts it.
    pub install_wapp_open: bool,
    /// The generation of an install-wapp dialog open (`Dialog::open_epoch`);
    /// see [`Self::connection_dialog_epoch`].
    pub install_wapp_epoch: u64,
    /// T022: the dialog's step (React's `InstallStep`): the pick step, then
    /// the review of the inspected package. The host inspect seam that
    /// advances it is the wapps management page wiring (T072).
    pub install_wapp_step: InstallWappStep,
    /// T022: the picked package's preview — the pick token, the package label
    /// and the inspected manifest (React's three `useState`s, set and cleared
    /// together). The inspect use case is a host seam (T072).
    pub install_wapp_preview: Option<WappPackagePreview>,
    /// T022: the pick→inspect pass is in flight (React's `inspecting`).
    pub install_wapp_inspecting: bool,
    /// T022: the confirm→install pass is in flight (React's `installing`).
    pub install_wapp_installing: bool,
    /// T022: the install flow's last error (the dialog's error branch).
    pub install_wapp_error: Option<String>,
    /// T072: the wapp store's installed list (React `wappStore.wapps`),
    /// filled by the `wapps_loaded` seam transition (`wapps_state.rs`); the
    /// fetch's own failure branch is the honest answer until the adapter
    /// carries the wapp list use case.
    pub wapps: Vec<WappSummaryVm>,
    /// T072: the store's `loaded` flag — the page's mount-time fetch gate
    /// (React: `if (!loaded) void fetch()`).
    pub wapps_loaded: bool,
    /// T072: the store's `error` slot (a failed fetch, or the store-side
    /// error of a failed toggle/uninstall command — React's store writes
    /// only the error on failure and keeps the list).
    pub wapps_error: Option<String>,
    /// T072: the page's search box (React's `search`, restored from the
    /// workspace panel snapshot on mount — the state simply persists, so
    /// the snapshot round-trip is a no-op of the same observable behaviour).
    pub wapp_search: TextEdit,
    /// T072: the page's filter chip (React's `filter`, defaulting to
    /// Workspace the way the snapshot fallback does).
    pub wapp_filter: WappFilter,
    /// T072: the page's action error (React's `actionError` — a failed
    /// toggle or uninstall the page caught).
    pub wapp_action_error: Option<String>,
    /// T072: the uninstall confirmation's pending ask (React's
    /// `useConfirmDialog` `{options, resolve}` state; the root modal layer
    /// mounts the dialog while one is pending).
    pub wapp_remove_request: Option<WappRemoveRequest>,
    /// T072: the generation of a remove-confirm open (`Dialog::open_epoch`);
    /// see [`Self::connection_dialog_epoch`].
    pub wapp_remove_epoch: u64,
    pub connection_name: TextEdit,
    pub connection_engine: TextEdit,
    pub(crate) connection_form_snapshots: std::collections::BTreeMap<String, crate::connections_io_state::ConnectionFormSnapshot>,
    pub(crate) redis_form_options: datazen_ui_api::native_kv::RedisConnectionOptions,
    pub(crate) redis_form_master: TextEdit,
    pub(crate) redis_form_nodes: TextEdit,
    pub(crate) redis_form_ca: TextEdit,
    pub(crate) redis_form_cert: TextEdit,
    pub(crate) redis_form_key: TextEdit,
    pub connection_address: TextEdit,
    /// E5: the remote fields the live host maps onto its config; the
    /// memory port ignores them.
    pub connection_username: TextEdit,
    pub connection_password: TextEdit,
    pub connection_database: TextEdit,
    pub connection_error: Option<String>,
    pub(crate) connection_probe_id: u64,
    pub(crate) connection_probe_pending: bool,
    pub(crate) connection_probe_result: Option<Result<(), datazen_ui_api::ConnectionProbeError>>,
    pub(crate) redis_probe_pending: bool,
    pub(crate) redis_probe_result: Option<Result<(), String>>,
    /// UI-only SSH tunnel fields for A2 dialog alignment (not persisted; save uses draft only).
    pub connection_ssh_enabled: bool,
    pub connection_ssh_host: TextEdit,
    pub connection_ssh_port: TextEdit,
    pub connection_ssh_username: TextEdit,
    /// The snippet form's fields and its last rejected write.
    pub snippet_name: TextEdit,
    pub snippet_sql: TextEdit,
    pub snippet_error: Option<String>,
    /// T6: the favorites form's title input and its last rejected write. The
    /// statement comes from the current query editor — "save current SQL".
    pub favorite_title: TextEdit,
    pub favorite_error: Option<String>,
    /// T7: the parsed connection import waiting on its confirm, and the
    /// import path's outcome notice. The password serves the encrypted
    /// export formats; the plain ones ignore it.
    pub import_password: TextEdit,
    pub import_preview: Option<datazen_ui_api::ImportPreview>,
    pub import_notice: Option<String>,
    /// T4: the value search's form and state. The value and its mode ride
    /// the job; the hits list is the scan's answer.
    pub value_search_value: TextEdit,
    pub value_search_mode: u32,
    pub value_search_generation: u32,
    pub value_search_loading: Option<u32>,
    pub value_search_error: Option<String>,
    pub value_search_hits: Vec<datazen_ui_api::ValueSearchHit>,
    /// The value the loaded hits answered, so a locate filters by what was
    /// searched, not by whatever the input holds since.
    pub value_search_query: String,
    /// T5: the loaded table's inbound references (None until asked for).
    pub inbound_refs: Option<Vec<datazen_ui_api::InboundRef>>,
    pub fk_error: Option<String>,
    /// A4: FK navigation breadcrumb trail (followed foreign-key jumps).
    pub fk_breadcrumbs: Vec<FkBreadcrumb>,
    /// E3: the inbound generation — bumped by every ask. A delivery from a
    /// scan the state has moved past is dropped.
    pub inbound_generation: u32,
    /// E3: an inbound job the host accepted and has not answered yet.
    pub inbound_loading: Option<u32>,
    /// E1: the structure generation — bumped by every structure request (a
    /// table open, a view switch, an ER open). A structure delivery from a
    /// generation the table's entry has moved past is dropped.
    pub structure_generation: u32,
    /// E1: the loaded structures, per table — the four-part answer the
    /// structure views and the outbound FK navigation read. Render
    /// functions read this map only, never the adapter.
    pub structures: std::collections::BTreeMap<String, StructureState>,
    /// E2: the widget-bars generation — bumped by every bars request (a
    /// dashboards open, a widget save, a manual refresh).
    pub widget_bars_generation: u32,
    /// E2: the loaded bars, per widget id — the bar chart's only read.
    /// Render never queries the widget's data live.
    pub widget_bars: std::collections::BTreeMap<u64, WidgetBarsState>,
    /// The export dialog's format, range and its job.
    pub export_format_index: u32,
    pub export_range: TextEdit,
    pub export_job: Option<u64>,
    pub export_error: Option<String>,
    /// The export job's last snapshot. Refreshed only on demand.
    pub export_snapshot: Option<ProgressSnapshot>,
    /// The import wizard: 0 source step, 1 target step, 2 ready.
    pub import_step: u32,
    pub import_source: TextEdit,
    pub import_target: TextEdit,
    pub import_error: Option<String>,
    pub(crate) native_csv: crate::native_csv_state::NativeCsvState,
    /// The backup job and its last snapshot.
    pub backup_job: Option<u64>,
    pub backup_snapshot: Option<ProgressSnapshot>,
    pub backup_error: Option<String>,
    /// The tools page's active tool.
    pub tool: Tool,
    /// The sync wizard: endpoints, tables, options, preview, run.
    pub sync_step: u32,
    pub sync_source: String,
    pub sync_target: String,
    pub sync_tables: std::collections::BTreeSet<String>,
    pub sync_options: [bool; 3],
    pub sync_error: Option<String>,
    /// The sync preview grid (read-only, the B8 sheet).
    pub sync_preview_grid: Option<DataGrid>,
    /// The sync run job and its last snapshot.
    pub sync_job: Option<u64>,
    pub sync_snapshot: Option<ProgressSnapshot>,
    /// The compare tool's endpoints.
    pub compare_source: String,
    pub compare_target: String,
    /// The compare plan's apply job and its last snapshot.
    pub compare_job: Option<u64>,
    pub compare_snapshot: Option<ProgressSnapshot>,
    pub compare_error: Option<String>,
    /// The transfer tool's tables, pairing chips and the pairs themselves.
    pub(crate) native_transfer: crate::native_transfer_state::NativeTransferState,
    pub transfer_left: String,
    pub transfer_right: String,
    pub transfer_pair_left: String,
    pub transfer_pair_right: String,
    pub transfer_pairs: Vec<(String, String)>,
    pub transfer_error: Option<String>,
    /// The transfer's confirmation step and its job.
    pub transfer_confirming: bool,
    pub transfer_job: Option<u64>,
    pub transfer_snapshot: Option<ProgressSnapshot>,
    /// The widget form's fields and its last rejected save.
    pub widget_title: TextEdit,
    pub widget_query: TextEdit,
    pub widget_error: Option<String>,
    /// The workflow page's YAML document, edited through the SQL editor's
    /// document model (T145: the edit panel's yaml mode and the lite editor
    /// before it — the surface the workspace-tabs tests pin).
    pub workflow_yaml: SqlEditor,
    /// The last workflow run's results.
    pub workflow_results: Vec<WorkflowStepResult>,
    /// T019: the same run as the ported execution-result panel's view model
    /// (the honest projection of `workflow_results`; see
    /// `WorkflowExecutionResult::from_step_results`).
    pub workflow_execution: Option<WorkflowExecutionResult>,
    /// T019: the panel's per-step interaction state — step ids whose rows are
    /// expanded, and step ids currently showing the chart view.
    pub workflow_step_expanded: std::collections::BTreeSet<String>,
    pub workflow_step_chart: std::collections::BTreeSet<String>,
    /// T8: the workflow run's generation and its in-flight marker. The host's
    /// engine runs as a job; a delivery from a generation the state has moved
    /// past is dropped.
    pub workflow_generation: u32,
    pub workflow_loading: Option<u32>,
    /// T070: the visual workflow form's draft (React `WorkflowForm`'s
    /// `draft` prop), driven by the page mount's edit pane (T145) and the
    /// form's tests through `workflow_form_slots()`.
    pub workflow_draft: WorkflowDraft,
    /// T070: the form's open listbox (React's `ui/Select` keeps its own open
    /// state internally; immediate mode puts the one open select in `S`).
    pub workflow_form_open: Option<WorkflowFormSelect>,
    /// T070: the command-input text areas' editing state (React's
    /// `CommandInputEditor` locals), keyed `step<i>.<field>` / `step<i>.raw`.
    pub workflow_form_input_edits: std::collections::BTreeMap<String, TextEdit>,
    /// T070: the connections the form offers (React's `connections` prop —
    /// the page's `savedConnections`), filled by the page's
    /// `reload_workflow_connections` (T145) and the form's probes.
    pub workflow_form_connections: Vec<WorkflowConnectionOption>,
    /// T070: the loaded command definitions per connection (React's
    /// `commandsByConnection` — the loader itself is a host-side seam).
    pub workflow_form_commands: std::collections::BTreeMap<String, Vec<WorkflowCommandDefinition>>,
    /// T070: the connections whose command list is in flight (React's
    /// `loadingCommands`).
    pub workflow_form_commands_loading: std::collections::BTreeSet<String>,
    /// T070: the loaded database lists per multi-db connection (React's
    /// `databasesByConn`; the `listDatabasesDedicated` loader is T467's).
    pub workflow_form_databases: std::collections::BTreeMap<String, Vec<String>>,
    /// T070: the connections whose database list is in flight (React's
    /// `loadingDatabases`).
    pub workflow_form_databases_loading: std::collections::BTreeSet<String>,
    /// T145: the page's workflow list (React `aiStore.workflows`) — the host
    /// registry's lite projection, refilled by `fetch_workflow_list`
    /// (`workflow_page_state.rs`).
    pub workflow_list: Vec<WorkflowListItem>,
    /// T145: the list's loading flag (React `workflowsLoading`).
    pub workflow_list_loading: bool,
    /// T145: the store's error slot (React `workflowError`) — the pane branch
    /// the page renders while it is `Some`.
    pub workflow_store_error: Option<String>,
    /// T145: the open panels (React `panels`) — run, edit, history and
    /// ai-create, in open order.
    pub workflow_panels: Vec<WorkflowPanel>,
    /// T145: the active panel id (React `activePanelId`).
    pub workflow_active_panel: Option<String>,
    /// T145: the active step index (React `activeStepIndex`) — the step the
    /// detail view shows within the active panel's result.
    pub workflow_active_step: Option<usize>,
    /// T145: the page's operation error (React `operationError`) — the strip
    /// banner the sidebar renders.
    pub workflow_operation_error: Option<String>,
    /// T145: the page's feedback line (React `feedback`).
    pub workflow_feedback: String,
    /// T145: the sidebar's side tab (React `sideTab`).
    pub workflow_side_tab: WorkflowSideTab,
    /// T145: the history items (React `historyItems`) — the list use case's
    /// seam keeps them empty; the wiring's transition
    /// (`workflow_history_loaded`) fills them.
    pub workflow_history_items: Vec<HistoryListItem>,
    /// T145: the run's variable inputs (React `variables`) — the field values
    /// the run panel's inputs edit.
    pub workflow_variables: std::collections::BTreeMap<String, TextEdit>,
    /// T145: the connection variable's select — the open dropdown's
    /// variable name (the workflow form's select slots pattern).
    pub workflow_variable_select: Option<String>,
    /// T145: the workflows dir (React `workflowsDir`) — the get-dir command
    /// seam keeps it `None`, so React's own conditional hides the open-dir
    /// button.
    pub workflow_dir: Option<String>,
    /// T145: the edit panel's editor mode (React `editorMode`).
    pub workflow_editor_mode: WorkflowEditorMode,
    /// T145: the step-result order (React
    /// `appSettings.workflowStepResultOrder ?? 'desc'`; the settings store's
    /// persistence is the seam).
    pub workflow_step_order: WorkflowStepOrder,
    /// T145: the confirm dialog's pending ask (React `useConfirmDialog`'s
    /// `{options, resolve}`; the root modal layer mounts the dialog while
    /// one is pending).
    pub workflow_confirm_request: Option<WorkflowConfirmRequest>,
    /// T145: the generation of a confirm-dialog open (`Dialog::open_epoch`).
    pub workflow_confirm_epoch: u64,
    /// T145: the add-to-dashboard dialog is open — the pending mount the
    /// modal layer shows (T018's dialog port swaps it).
    pub workflow_add_dashboard_open: bool,
    /// T145: the list row's secondary press — the context-menu seam's trigger
    /// (the workflow id and the window point; the entries builder and the
    /// native host are T646's).
    pub workflow_context_menu: Option<WorkflowContextMenuRequest>,
    /// T145: the page's chart config slot per step id (ChartView's
    /// `savedConfig`).
    pub workflow_chart_configs:
        std::collections::BTreeMap<String, crate::components::chart::chart_canvas::ChartConfig>,
    /// T145: the page's per-step chart flags — the expanded overlay, the split
    /// view and the export dropdown (ChartView's own state, S-owned).
    pub workflow_chart_expanded: std::collections::BTreeSet<String>,
    pub workflow_chart_split: std::collections::BTreeSet<String>,
    pub workflow_chart_export_open: std::collections::BTreeSet<String>,
    /// T145: the panel id counter (React's module-scope `panelCounter`).
    pub workflow_panel_counter: u32,
    /// Live size for the splitter being dragged (L2): (region key, points).
    /// While `Some`, `splitter_size` returns this instead of the persisted
    /// value; a cancelled or finished drag clears it.
    pub splitter_preview: Option<(String, u32)>,
    /// T071 (segment-2's T070): the running-execution close host's pending
    /// ask — React `useConfirmDialog`'s single pending request, installed by
    /// `RunningExecutionCloseHost` (the prompt behind "close a tab whose
    /// query is still running"). `Some` while the confirm dialog is on
    /// screen; `ask_running_execution_close` opens it.
    pub running_close_request: Option<RunningExecutionCloseRequest>,
    /// T071: the prompt's open generation; each ask bumps it so a reopened
    /// dialog starts at the top (`Dialog::open_epoch`).
    pub running_close_epoch: u64,
    /// T071: the last settled ask — `(request, allowed)`. The host's answer
    /// routing (React `handleClose(result)` + the promise's resolve);
    /// written by `answer_running_execution_close`, read by the close guard
    /// when it ports (T396's `stores/runningExecutionClose.ts`), so no
    /// production reader yet.
    #[allow(dead_code)]
    pub running_close_outcome: Option<(RunningExecutionCloseRequest, bool)>,
    /// T071: whether the prompt's copy control has copied the SQL (the
    /// dialog's copied mark; the next ask resets it).
    pub running_close_sql_copied: bool,
    /// T009-S7: the AI assistant aside's open state. React's
    /// `ContentView` holds `aiChatOpen` in `useState(false)`; the Rune
    /// copy lives here so the toggle, the mount and the host sync all
    /// read one flag. It survives section switches (unlike React, where
    /// leaving the connection view unmounts `ContentView` and drops it)
    /// — the panel re-mounts when the workspace returns, and the
    /// unmount cancel still fires on the transition (see
    /// `ai_chat_mounted`).
    pub ai_chat_open: bool,
    /// T009-S7: the assistant panel's owned state (S6's
    /// `AiChatPanelState`). Constructed with an empty db session id:
    /// the session key falls back to `'default'` exactly as React's
    /// `computeSessionKey` does when `connectionId` and `dbSessionId`
    /// are both empty, and the live connection id re-keys it through
    /// `ensure_session` once a connection opens.
    pub ai_chat_panel: AiChatPanelState,
    /// T009-S7: the host-owned stand-in for
    /// `settingsConfig.memory.enabled` (React `lib/aiMemorySettings.ts`:
    /// memory is opt-in, `defaultAiMemorySettings.enabled: false`, so an
    /// unset config sends `'off'`). The host sync seeds the panel from
    /// this field on every poll — never the panel VM's own `true`
    /// default, which would read `'auto'` with no settings behind it
    /// (tester S6 non-blocking #3). T088 replaces this field with the
    /// real settings config.
    pub ai_memory_enabled: bool,
    /// T009-S7: the pending AI draft request the host holds (React
    /// `ContentView`'s `pendingDraftRequest`); cleared when the panel's
    /// consumed-draft outbox acks it.
    pub ai_pending_draft: Option<AiPendingDraft>,
    /// T009-S7: how many consumed-draft acks the host has applied, so
    /// the outbox (which only grows) is read incrementally.
    pub ai_draft_acks_applied: usize,
    /// T009-S7: the last draft the host issued to the panel, as
    /// (request id, input-was-empty, was-configured). The host
    /// re-issues `handle_draft_request` when the id changes, when the
    /// input's emptiness flips, or when the configured flag flips —
    /// mirroring React's draft effect, which re-runs on
    /// `[draftRequest, isConfigured, isStreaming, input]` (streaming is
    /// excluded: a re-issue after a stream ends is a no-op in the VM —
    /// the processed mark blocks it — exactly as React's does).
    pub ai_draft_last_issue: Option<(String, bool, bool)>,
    /// T009-S7: whether the panel was mounted on the last poll, so the
    /// unmount cancel (`cancel_in_flight`, React's unmount effect)
    /// fires once on the mounted → unmounted transition instead of on
    /// every poll while the panel stays hidden.
    pub ai_chat_mounted: bool,
    /// T012: the write-proposal host's owned state (React
    /// `AiWriteProposalHost`'s `queue` / `remainingMs` / `responding` /
    /// `error` / `receipt` / `auditError`, mounted at the app root in
    /// `App.tsx:104`, next to the AI panel). The cross-window event edge
    /// and the `ai_apply_write` / `ai_deny_write` drain have no Rune
    /// source yet — see the component module's docs — so this state only
    /// moves through its own methods and the poll tick below.
    pub ai_write_proposal: AiWriteProposalHostState,
    /// The system appearance preference, updated by the root view so helpers
    /// like `dark_theme()` can fall back consistently when no theme is saved.
    pub system_dark: Cell<bool>,
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
            connections_load_error: None,
            value_presentation_registry: Default::default(),
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
            query_history: Default::default(),
            query_notice: None,
            result_view: ResultView::Table,
            result_find: TextEdit::new(String::new()),
            query_generation: 0,
            read_generation: 0,
            table_loading: None,
            table_read_error: None,
            commit_loading: None,
            edit_preview: Default::default(),
            query_loading: None,
            native_query: None,
            connect_loading: None,
            desktop_catalog: None,
            native_kv: None,
            native_kv_unused_input: rune::TextEdit::new(String::new()),
            desktop_connection: None,
            collapsed: std::collections::BTreeSet::new(),
            table_view: TableView::Data,
            er_schema: None,
            er_selected: None,
            er_search: TextEdit::default(),
            er_zoom: 1.0,
            er_positions: std::collections::BTreeMap::new(),
            er_drag: None,
            column_drafts: std::collections::BTreeMap::new(),
            draft_name: TextEdit::new(String::new()),
            draft_type: TextEdit::new(String::new()),
            page_size_input: TextEdit::new(String::new()),
            max_rows_input: TextEdit::new(String::new()),
            settings_error: None,
            settings_content: Default::default(),
            appearance_select: None,
            connection_search: TextEdit::new(String::new()),
            connections_show_all: false,
            home_copied_id: None,
            home_copied_until: None,
            home_slot: None,
            connection_dialog_open: false,
            connection_dialog_epoch: 0,
            import_dialog_open: false,
            import_dialog_epoch: 0,
            import_file: None,
            install_wapp_open: false,
            install_wapp_epoch: 0,
            install_wapp_step: InstallWappStep::Select,
            install_wapp_preview: None,
            install_wapp_inspecting: false,
            install_wapp_installing: false,
            install_wapp_error: None,
            wapps: Vec::new(),
            wapps_loaded: false,
            wapps_error: None,
            wapp_search: TextEdit::new(String::new()),
            wapp_filter: WappFilter::Workspace,
            wapp_action_error: None,
            wapp_remove_request: None,
            wapp_remove_epoch: 0,
            connection_name: TextEdit::new(String::new()),
            connection_engine: TextEdit::new(String::new()),
            connection_form_snapshots: Default::default(),
            redis_form_options: Default::default(),
            redis_form_master: TextEdit::new(""),
            redis_form_nodes: TextEdit::new(""),
            redis_form_ca: TextEdit::new(""),
            redis_form_cert: TextEdit::new(""),
            redis_form_key: TextEdit::new(""),
            connection_address: TextEdit::new(String::new()),
            connection_username: TextEdit::new(String::new()),
            connection_password: TextEdit::new(String::new()),
            connection_database: TextEdit::new(String::new()),
            connection_error: None,
            connection_probe_id: 0,
            connection_probe_pending: false,
            connection_probe_result: None,
            redis_probe_pending: false,
            redis_probe_result: None,
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
            native_csv: Default::default(),
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
            native_transfer: Default::default(),
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
            workflow_yaml: SqlEditor::new(),
            workflow_results: Vec::new(),
            workflow_execution: None,
            workflow_step_expanded: std::collections::BTreeSet::new(),
            workflow_step_chart: std::collections::BTreeSet::new(),
            workflow_generation: 0,
            workflow_loading: None,
            workflow_draft: WorkflowDraft::empty(),
            workflow_form_open: None,
            workflow_form_input_edits: std::collections::BTreeMap::new(),
            workflow_form_connections: Vec::new(),
            workflow_form_commands: std::collections::BTreeMap::new(),
            workflow_form_commands_loading: std::collections::BTreeSet::new(),
            workflow_form_databases: std::collections::BTreeMap::new(),
            workflow_form_databases_loading: std::collections::BTreeSet::new(),
            workflow_list: Vec::new(),
            workflow_list_loading: false,
            workflow_store_error: None,
            workflow_panels: Vec::new(),
            workflow_active_panel: None,
            workflow_active_step: None,
            workflow_operation_error: None,
            workflow_feedback: String::new(),
            workflow_side_tab: WorkflowSideTab::Workflows,
            workflow_history_items: Vec::new(),
            workflow_variables: std::collections::BTreeMap::new(),
            workflow_variable_select: None,
            workflow_dir: None,
            workflow_editor_mode: WorkflowEditorMode::Visual,
            workflow_step_order: WorkflowStepOrder::Desc,
            workflow_confirm_request: None,
            workflow_confirm_epoch: 0,
            workflow_add_dashboard_open: false,
            workflow_context_menu: None,
            workflow_chart_configs: std::collections::BTreeMap::new(),
            workflow_chart_expanded: std::collections::BTreeSet::new(),
            workflow_chart_split: std::collections::BTreeSet::new(),
            workflow_chart_export_open: std::collections::BTreeSet::new(),
            workflow_panel_counter: 0,
            splitter_preview: None,
            running_close_request: None,
            running_close_epoch: 0,
            running_close_outcome: None,
            running_close_sql_copied: false,
            ai_chat_open: false,
            ai_chat_panel: AiChatPanelState::new(String::new()),
            ai_memory_enabled: false,
            ai_pending_draft: None,
            ai_draft_acks_applied: 0,
            ai_draft_last_issue: None,
            ai_chat_mounted: false,
            ai_write_proposal: AiWriteProposalHostState::new(),
            system_dark: Cell::new(true),
        }
    }
}

/// One table's structure read state (E1): the generation the entry waits
/// on, its loading and error marks, and the four-part answer once it
/// lands. Render functions read the data; the honest states say
/// themselves instead of drawing a phantom empty table.
pub struct StructureState {
    pub generation: u32,
    pub loading: bool,
    pub error: Option<String>,
    pub data: Option<datazen_ui_api::TableStructure>,
}

/// One widget's bars read state (E2): the generation the entry waits on,
/// its loading and error marks, and the chart's answer once it lands.
pub struct WidgetBarsState {
    pub generation: u32,
    pub loading: bool,
    pub error: Option<String>,
    pub bars: Option<Vec<(String, u32)>>,
}

/// A4: one step in the FK navigation breadcrumb trail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FkBreadcrumb {
    pub label: String,
    pub table: String,
    pub page: u32,
    pub sort: Option<datazen_ui_api::ColumnSort>,
    pub filter: Option<datazen_ui_api::ColumnFilter>,
}
