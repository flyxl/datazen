//! The state half of the T112 `DashboardPanel` port: every `useState` of the
//! React component, the `useDashboardStore` entry projections the panel reads
//! (`current` / `runs` / `busyWidgets` / `loading` / `error` / `list` /
//! `listLoading`), the nested T069 tile states and the nested T175 run-history
//! drawer state, plus the mount effect's bootstrap machine. Every transition
//! is a method; the host calls the deliveries and polls the `*_due` /
//! `*_sync` accessors exactly where React's effects fired.

use std::collections::{HashMap, HashSet};

use rune::TextEdit;

use crate::components::chart::chart_canvas::{ChartConfig, ChartType};
use crate::i18n::t;
use crate::views::dashboard::chart_widget_tile::{
    ChartWidgetTileState, TileViewMode, WidgetRunVm,
};
use crate::views::dashboard::run_history_drawer::RunHistoryDrawerState;

use super::{
    BoardListItemVm, DashboardSnapshotVm, DashboardVm, EditorWorkflowVm, PanelWidgetVm,
    RefreshModeVm, RefreshPolicyVm, WidgetLayoutVm, create_empty_dashboard, next_widget_layout,
};

/// The mount effect's two arms (React lines 152–177): the explicit-open early
/// return, or the async bootstrap that re-points the active board at the
/// list's first entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MountArm {
    /// `initialDashboardId && !savedSnapshot`: honor the prop, no bootstrap.
    ExplicitOpen,
    /// The async arm: `setBootstrapping(true)`, fetch, pick `boards[0]`.
    Bootstrap,
}

/// The `openWidgetEditor` flow's two shapes: open now, or fetch the workflow
/// first (the drawer opens only after the fetch settles, exactly as React's
/// `await` ordering does).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EditorOpenFlow {
    /// `setEditorOpen(true)` already ran (the as-new and no-workflow arms).
    Open,
    /// The `workflowGet` the host must serve; deliver through
    /// [`DashboardPanelState::deliver_editor_workflow`] (or its error twin),
    /// which opens the drawer.
    FetchWorkflow(String),
}

/// Every `useState` of the React component plus the store projections and the
/// two nested component states, kept in the owning state `S`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug)]
pub struct DashboardPanelState {
    /// React's `activeDashboardId` (`''` = none).
    pub active_dashboard_id: String,
    /// React's `bootstrapping`.
    pub bootstrapping: bool,
    /// The store's `list`.
    pub list: Vec<BoardListItemVm>,
    /// The store's `listLoading`.
    pub list_loading: bool,
    /// The store entry's `dashboard` (`dashboardsById[active].dashboard`).
    pub current: Option<DashboardVm>,
    /// The store entry's `runs` (missing key = never loaded; `None` value =
    /// loaded and empty — React's `undefined` vs `null` distinction, which the
    /// on-open refresh gate reads).
    pub runs: HashMap<String, Option<WidgetRunVm>>,
    /// The store entry's `busyWidgets`.
    pub busy_widgets: HashSet<String>,
    /// The store entry's `loading`.
    pub loading: bool,
    /// The store entry's `error`.
    pub error: Option<String>,
    /// React's `editorOpen` (the T201 seam's `open` prop).
    pub editor_open: bool,
    /// React's `editingWidget`.
    pub editing_widget: Option<PanelWidgetVm>,
    /// React's `isNewWidget`.
    pub is_new_widget: bool,
    /// React's `editorHiddenSql`.
    pub editor_hidden_sql: Option<super::HiddenSqlVm>,
    /// React's `userWorkflows`.
    pub user_workflows: Vec<super::WorkflowListItemVm>,
    /// The `workflowGet` `openWidgetEditor` awaits (host-polled; React has no
    /// cancel token here, so a close does **not** clear it — the late
    /// delivery still opens the drawer, as React's does).
    pub editor_fetch: Option<String>,
    /// React's `historyOpen`.
    pub history_open: bool,
    /// React's `historyWidget`.
    pub history_widget: Option<PanelWidgetVm>,
    /// React's `renaming`.
    pub renaming: bool,
    /// React's `nameDraft` as the field's `TextEdit` (the snapshot's
    /// `nameDraft` is its text).
    pub rename_edit: TextEdit,
    /// React's `refreshingAll`.
    pub refreshing_all: bool,
    /// React's `monitorPaused`.
    pub monitor_paused: bool,
    /// React's `deleteConfirmOpen`.
    pub delete_confirm_open: bool,
    /// React's `widgetToDelete`.
    pub widget_to_delete: Option<String>,
    /// React's `lastSyncedBoardRef` (`"${id}:${name}"`).
    pub last_synced_board: String,
    /// The T175 drawer the panel mounts (its `open` tracks
    /// [`Self::history_open`]).
    pub history_drawer: RunHistoryDrawerState,
    /// The T069 per-widget tile states (`expanded` + the overlay's
    /// `localConfig`).
    pub tile_states: HashMap<String, ChartWidgetTileState>,
}

impl Default for DashboardPanelState {
    /// React's `useState` initializers with no prop and no snapshot: no
    /// board, bootstrapping. [`Self::mount`] is the real constructor.
    fn default() -> Self {
        Self::mount(None, None)
    }
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl DashboardPanelState {
    /// React's `useState` initializer block (lines 70–115): the prop wins
    /// over the snapshot for the active board; the snapshot restores the
    /// recreatable view state; `bootstrapping` starts true only with neither
    /// prop nor snapshot board.
    pub fn mount(initial: Option<&str>, saved: Option<&DashboardSnapshotVm>) -> Self {
        let initial = initial.filter(|id| !id.is_empty());
        let active = initial
            .map(str::to_owned)
            .or_else(|| saved.map(|s| s.active_dashboard_id.clone()))
            .unwrap_or_default();
        let bootstrapping =
            initial.is_none() && saved.is_none_or(|s| s.active_dashboard_id.is_empty());
        Self {
            active_dashboard_id: active,
            bootstrapping,
            list: Vec::new(),
            list_loading: false,
            current: None,
            runs: HashMap::new(),
            busy_widgets: HashSet::new(),
            loading: false,
            error: None,
            editor_open: saved.is_some_and(|s| s.editor_open),
            editing_widget: saved.and_then(|s| s.editing_widget.clone()),
            is_new_widget: saved.is_some_and(|s| s.is_new_widget),
            editor_hidden_sql: saved.and_then(|s| s.editor_hidden_sql.clone()),
            user_workflows: saved.map_or_else(Vec::new, |s| s.user_workflows.clone()),
            editor_fetch: None,
            history_open: saved.is_some_and(|s| s.history_open),
            history_widget: saved.and_then(|s| s.history_widget.clone()),
            renaming: saved.is_some_and(|s| s.renaming),
            rename_edit: TextEdit::new(saved.map_or_else(String::new, |s| s.name_draft.clone())),
            refreshing_all: false,
            monitor_paused: false,
            delete_confirm_open: false,
            widget_to_delete: None,
            last_synced_board: String::new(),
            history_drawer: RunHistoryDrawerState::default(),
            tile_states: HashMap::new(),
        }
    }

    /// The mount effect (lines 152–177). The host calls `fetchDashboards` in
    /// both arms; this returns which arm ran so the host knows whether the
    /// landing goes through [`Self::deliver_list`]'s bootstrap half.
    pub fn apply_mount_effect(
        &mut self,
        initial: Option<&str>,
        snapshot_present: bool,
    ) -> MountArm {
        let initial = initial.filter(|id| !id.is_empty());
        if initial.is_some() && !snapshot_present {
            self.active_dashboard_id = initial.unwrap_or_default().to_owned();
            self.bootstrapping = false;
            return MountArm::ExplicitOpen;
        }
        self.bootstrapping = true;
        MountArm::Bootstrap
    }

    /// `fetchDashboards`'s opening `set({ listLoading: true })`.
    pub fn begin_list_fetch(&mut self) {
        self.list_loading = true;
    }

    /// `fetchDashboards`'s landing: the list replaces, the flag clears. In
    /// the bootstrap arm the first board becomes active — React's async arm
    /// does this **unconditionally** (`if (boards.length > 0)
    /// setActiveDashboardId(boards[0]!.id)`), so a snapshot-restored board
    /// survives only until the bootstrap fetch lands; that is React's
    /// behavior, ported as written.
    pub fn deliver_list(&mut self, list: Vec<BoardListItemVm>) {
        self.list_loading = false;
        if self.bootstrapping {
            self.bootstrapping = false;
            if let Some(first) = list.first() {
                self.active_dashboard_id = first.id.clone();
            }
        }
        self.list = list;
    }

    /// `switchDashboard` (line 188): the active id moves; the `(id, name)`
    /// pair is the `onDashboardChange` the host fires when the board is in
    /// the list (React's `board` lookup).
    pub fn switch_dashboard(&mut self, id: &str) -> Option<(String, String)> {
        self.active_dashboard_id = id.to_owned();
        self.list
            .iter()
            .find(|board| board.id == id)
            .map(|board| (board.id.clone(), board.name.clone()))
    }

    /// `loadDashboard`'s opening `set({ loading: true, error: null })`.
    pub fn begin_dashboard_load(&mut self) {
        self.loading = true;
        self.error = None;
    }

    /// `loadDashboard`'s landing: the board and its whole runs map land
    /// atomically (React's single `set` after `Promise.all`), and the two
    /// `useEffect`s that watch `current` fire — `setNameDraft(current.name)`
    /// and `setMonitorPaused(!!current.refreshPaused)`. The store's patch
    /// leaves a previous `error` untouched, and so does this.
    pub fn deliver_dashboard(
        &mut self,
        dashboard: DashboardVm,
        runs: HashMap<String, Option<WidgetRunVm>>,
    ) {
        self.rename_edit.replace_text(dashboard.name.clone());
        self.monitor_paused = dashboard.refresh_paused;
        self.runs = runs;
        self.current = Some(dashboard);
        self.loading = false;
    }

    /// `loadDashboard`'s catch: the error lands, the stale board survives
    /// (the store's patch never clears `dashboard`).
    pub fn deliver_dashboard_error(&mut self, error: String) {
        self.loading = false;
        self.error = Some(error);
    }

    /// `refreshWidget`'s busy half (`busyWidgets[widgetId] = true` / the
    /// delete on settle) — the host applies both ends.
    pub fn set_widget_busy(&mut self, widget_id: &str, busy: bool) {
        if busy {
            self.busy_widgets.insert(widget_id.to_owned());
        } else {
            self.busy_widgets.remove(widget_id);
        }
    }

    /// `setRun(dashboardId, widgetId, run)` — the cross-window bus's delivery
    /// (a `null` run is React's `setRun(..., null)`).
    pub fn set_run(&mut self, widget_id: &str, run: Option<WidgetRunVm>) {
        self.runs.insert(widget_id.to_owned(), run);
    }

    /// The on-open refresh effect (lines 245–252): the enabled widgets whose
    /// refresh mode is `onOpen` and whose run entry is missing or null. The
    /// host serves the list after every `current`/`runs` change (React's dep
    /// array); the misses a failed refresh leaves do **not** re-fire on their
    /// own — only the next `current` change re-asks, exactly as the effect's
    /// deps do.
    pub fn on_open_refreshes_due(&self) -> Vec<String> {
        let Some(current) = &self.current else {
            return Vec::new();
        };
        current
            .widgets
            .iter()
            .filter(|widget| {
                widget.enabled
                    && matches!(widget.refresh.mode, RefreshModeVm::OnOpen)
                    && self.runs.get(&widget.id).is_none_or(|run| run.is_none())
            })
            .map(|widget| widget.id.clone())
            .collect()
    }

    /// The shell-title sync effect (lines 233–238): the `(id, name)` pair the
    /// host should hand `onDashboardChange` — `None` when the board is
    /// missing or the `${id}:${name}` stamp is unchanged.
    pub fn title_sync_due(&self) -> Option<(String, String)> {
        let current = self.current.as_ref()?;
        let stamp = format!("{}:{}", current.id, current.name);
        if stamp == self.last_synced_board {
            return None;
        }
        Some((current.id.clone(), current.name.clone()))
    }

    /// The effect's `lastSyncedBoardRef.current = …` line.
    pub fn mark_title_synced(&mut self) {
        if let Some(current) = &self.current {
            self.last_synced_board = format!("{}:{}", current.id, current.name);
        }
    }

    /// `handleRefreshAll`'s `setRefreshingAll(true)` / `finally` false pair.
    pub fn begin_refresh_all(&mut self) {
        self.refreshing_all = true;
    }

    pub fn end_refresh_all(&mut self) {
        self.refreshing_all = false;
    }

    /// `handleToggleMonitorPause`'s `next = !monitorPaused`.
    pub fn next_monitor_pause(&self) -> bool {
        !self.monitor_paused
    }

    /// The same handler's `setMonitorPaused(next)` — the host applies it once
    /// its `setDashboardRefreshPaused` call resolves (React awaits first).
    pub fn set_monitor_paused(&mut self, paused: bool) {
        self.monitor_paused = paused;
    }

    /// The rename button's `setRenaming(true)` / the input's Escape arm.
    pub fn begin_rename(&mut self) {
        self.renaming = true;
    }

    pub fn cancel_rename(&mut self) {
        self.renaming = false;
    }

    /// React's `nameDraft` (the snapshot field).
    pub fn name_draft(&self) -> String {
        self.rename_edit.text.clone()
    }

    /// `handleRename` (lines 387–391): the trimmed name to save, or `None`
    /// when there is no board or the draft is blank (React returns early and
    /// keeps the input open).
    pub fn commit_rename(&mut self) -> Option<String> {
        let name = self.rename_edit.text.trim().to_owned();
        if self.current.is_none() || name.is_empty() {
            return None;
        }
        self.renaming = false;
        Some(name)
    }

    /// The panel-delete dialog's open / close / confirm-first-line trio.
    pub fn open_delete_confirm(&mut self) {
        self.delete_confirm_open = true;
    }

    pub fn close_delete_confirm(&mut self) {
        self.delete_confirm_open = false;
    }

    /// `handleDeletePanel`'s `setDeleteConfirmOpen(false)` — the host then
    /// runs `deleteDashboard` and delivers the outcome.
    pub fn begin_delete_panel(&mut self) {
        self.delete_confirm_open = false;
    }

    /// `handleDeletePanel`'s tail (lines 209–214) over the store's
    /// post-delete list: no boards left clears the active id, otherwise the
    /// first remaining board activates (returning its `onDashboardChange`
    /// pair). The store's `deleteDashboard` set also drops the entry and
    /// filters the list, so the remaining list lands here with it.
    pub fn panel_deleted(&mut self, remaining: Vec<BoardListItemVm>) -> Option<(String, String)> {
        self.list = remaining;
        if self.list.is_empty() {
            self.active_dashboard_id = String::new();
            None
        } else {
            let first = self.list[0].id.clone();
            self.switch_dashboard(&first)
        }
    }

    /// The tile's delete button (`setWidgetToDelete(id)`) and the dialog's
    /// close (`setWidgetToDelete(null)`).
    pub fn request_delete_widget(&mut self, widget_id: &str) {
        self.widget_to_delete = Some(widget_id.to_owned());
    }

    pub fn cancel_delete_widget(&mut self) {
        self.widget_to_delete = None;
    }

    /// `handleDeleteWidget`'s `setWidgetToDelete(null)` — the taken id is the
    /// widget the host's save flow filters out.
    pub fn begin_delete_widget(&mut self) -> Option<String> {
        self.widget_to_delete.take()
    }

    /// `handleDeleteWidget`'s filtered board (the save payload): the current
    /// board without the widget, or `None` when there is no board.
    pub fn board_without_widget(&self, widget_id: &str) -> Option<DashboardVm> {
        let mut board = self.current.clone()?;
        board.widgets.retain(|widget| widget.id != widget_id);
        Some(board)
    }

    /// The tile's history button (lines 460–463): the widget lands and the
    /// T175 drawer's open effect runs (`begin_resync` arms the index fetch).
    pub fn open_history(&mut self, widget: &PanelWidgetVm) {
        self.history_widget = Some(widget.clone());
        self.history_open = true;
        self.history_drawer
            .begin_resync(&self.active_dashboard_id, &widget.tile_vm());
    }

    /// The drawer's `onClose` (lines 703–706): both flags clear and the
    /// drawer's in-flight fetches cancel.
    pub fn close_history(&mut self) {
        self.history_open = false;
        self.history_widget = None;
        self.history_drawer.close();
    }

    /// `handleAddWidget` (lines 291–304): the draft widget —
    /// `t('dashboard.newWidget')`, `DEFAULT_CHART_CONFIG` with `yAxes: ['v']`,
    /// [`next_widget_layout`], `DEFAULT_REFRESH`, enabled — then the as-new
    /// arm of `openWidgetEditor`. The host supplies React's
    /// `crypto.randomUUID()`. Returns false when there is no board (React's
    /// early return).
    pub fn begin_add_widget(&mut self, new_id: &str) -> bool {
        let Some(current) = &self.current else {
            return false;
        };
        let draft = PanelWidgetVm {
            id: new_id.to_owned(),
            title: t("dashboard.newWidget").to_owned(),
            workflow_id: String::new(),
            view_mode: TileViewMode::Chart,
            chart_config: Some(ChartConfig {
                y_axes: vec!["v".to_owned()],
                ..Default::default()
            }),
            layout: next_widget_layout(&current.widgets),
            refresh: RefreshPolicyVm::default(),
            enabled: true,
        };
        self.begin_edit_widget(draft, true);
        true
    }

    /// `openWidgetEditor` (lines 264–289): the synchronous half — the widget
    /// lands, the hidden-sql and workflow list reset — then either the
    /// `workflowGet` the host serves (the drawer opens on delivery) or the
    /// immediate open (the as-new hidden sql, or nothing when editing a
    /// workflow-less widget).
    pub fn begin_edit_widget(&mut self, widget: PanelWidgetVm, as_new: bool) -> EditorOpenFlow {
        let workflow_id = widget.workflow_id.clone();
        self.editing_widget = Some(widget);
        self.is_new_widget = as_new;
        self.editor_hidden_sql = None;
        self.user_workflows = Vec::new();
        if !as_new && !workflow_id.is_empty() {
            self.editor_fetch = Some(workflow_id.clone());
            EditorOpenFlow::FetchWorkflow(workflow_id)
        } else {
            if as_new {
                self.editor_hidden_sql = Some(super::HiddenSqlVm {
                    connection_id: String::new(),
                    sql: "SELECT 1 AS v".to_owned(),
                });
            }
            self.editor_open = true;
            EditorOpenFlow::Open
        }
    }

    /// The `workflowGet` the state has armed; the host serves it once and
    /// delivers through the two methods below.
    pub fn editor_fetch_due(&self) -> Option<&str> {
        self.editor_fetch.as_deref()
    }

    /// `openWidgetEditor`'s awaited branch (lines 271–281): a
    /// `dashboardHidden` workflow yields the hidden sql —
    /// `wf.connection ?? queryStep?.connection ?? ''` (empty string is not
    /// nullish, so an explicit empty connection wins) and
    /// `queryStep?.sql ?? 'SELECT 1 AS v'` — anything else yields the
    /// `workflowList` answer. Either way the drawer then opens
    /// (`setEditorOpen(true)` after the `await`).
    pub fn deliver_editor_workflow(&mut self, workflow: &EditorWorkflowVm) {
        self.editor_fetch = None;
        if workflow.visibility == "dashboardHidden" {
            let query = workflow.steps.iter().find(|step| step.kind == "query");
            self.editor_hidden_sql = Some(super::HiddenSqlVm {
                connection_id: workflow
                    .connection
                    .clone()
                    .or_else(|| query.and_then(|step| step.connection.clone()))
                    .unwrap_or_default(),
                sql: query
                    .and_then(|step| step.sql.clone())
                    .unwrap_or_else(|| "SELECT 1 AS v".to_owned()),
            });
        } else {
            self.user_workflows = workflow.user_workflows.clone();
        }
        self.editor_open = true;
    }

    /// The same branch's `catch` (`/* keep read-only workflow id */`): the
    /// drawer still opens.
    pub fn deliver_editor_workflow_error(&mut self) {
        self.editor_fetch = None;
        self.editor_open = true;
    }

    /// The editor drawer's `onClose` (lines 689–695). The in-flight
    /// `workflowGet` is **not** cancelled — React has no `cancelled` flag in
    /// `openWidgetEditor`, so a late delivery still opens the drawer again,
    /// and neither does this.
    pub fn close_editor(&mut self) {
        self.editor_open = false;
        self.editing_widget = None;
        self.is_new_widget = false;
        self.editor_hidden_sql = None;
        self.user_workflows = Vec::new();
    }

    /// `handleSaveWidget`'s tail (lines 334–341): the drawer closes and the
    /// new-widget flag clears. The hidden sql and workflow list survive
    /// (React's onClose is the only clearer). The refresh dispatch — skip it
    /// only for a new widget created from hidden SQL — is the host's, driven
    /// by the `is_new_widget` value this method returns first.
    pub fn editor_saved(&mut self) -> bool {
        let was_new = self.is_new_widget;
        self.editor_open = false;
        self.editing_widget = None;
        self.is_new_widget = false;
        was_new
    }

    /// The board the panel would save for a view-mode / chart-type /
    /// chart-config change (`handleViewModeChange`, `handleChartTypeChange`,
    /// `handleChartConfigChange`, lines 346–375): the current board with the
    /// one widget patched. The local `current` is **not** touched — React's
    /// store patch lands only when `saveDashboard` resolves, and the host
    /// delivers that through [`Self::deliver_dashboard`].
    pub fn board_with_view_mode(
        &self,
        widget_id: &str,
        mode: TileViewMode,
    ) -> Option<DashboardVm> {
        let mut board = self.current.clone()?;
        for widget in &mut board.widgets {
            if widget.id == widget_id {
                widget.view_mode = mode;
            }
        }
        Some(board)
    }

    pub fn board_with_chart_type(
        &self,
        widget_id: &str,
        chart_type: ChartType,
    ) -> Option<DashboardVm> {
        let mut board = self.current.clone()?;
        for widget in &mut board.widgets {
            if widget.id == widget_id && widget.chart_config.is_some() {
                if let Some(config) = widget.chart_config.as_mut() {
                    config.chart_type = chart_type;
                }
            }
        }
        Some(board)
    }

    pub fn board_with_chart_config(
        &self,
        widget_id: &str,
        config: ChartConfig,
    ) -> Option<DashboardVm> {
        let mut board = self.current.clone()?;
        for widget in &mut board.widgets {
            if widget.id == widget_id {
                widget.chart_config = Some(config.clone());
            }
        }
        Some(board)
    }

    /// The unmount snapshot (lines 120–150): the recreatable view state the
    /// mode-switch remount restores through [`Self::mount`].
    pub fn snapshot(&self) -> DashboardSnapshotVm {
        DashboardSnapshotVm {
            active_dashboard_id: self.active_dashboard_id.clone(),
            editor_open: self.editor_open,
            editing_widget: self.editing_widget.clone(),
            is_new_widget: self.is_new_widget,
            editor_hidden_sql: self.editor_hidden_sql.clone(),
            user_workflows: self.user_workflows.clone(),
            history_open: self.history_open,
            history_widget: self.history_widget.clone(),
            renaming: self.renaming,
            name_draft: self.rename_edit.text.clone(),
        }
    }

    /// The T069 tile state for a widget (the default until a press expands
    /// it) — the host's `begin_expand` / `collapse` / `set_overlay_config`
    /// route.
    pub fn tile_state_mut(&mut self, widget_id: &str) -> &mut ChartWidgetTileState {
        self.tile_states.entry(widget_id.to_owned()).or_default()
    }

    /// A fresh `createEmptyDashboard(name)` (lines 36–47) with the host's
    /// uuid and clock — `layout: { cols: 12, rowHeight: 80 }`, no widgets,
    /// enabled.
    pub fn empty_dashboard(name: &str, id: &str, now_iso: &str) -> DashboardVm {
        create_empty_dashboard(id, name, now_iso)
    }

    /// The export filename stem's widget-layout twin — exposed for the host's
    /// `handleExport` (the sanitizer itself is the parent module's
    /// [`super::sanitize_export_stem`]).
    pub fn widget_layout_floor() -> WidgetLayoutVm {
        WidgetLayoutVm { x: 0, y: 0, w: 6, h: 4 }
    }
}
