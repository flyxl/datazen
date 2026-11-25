//! Port of `packages/frontend/src/windows/dashboard/DashboardPanel.tsx`
//! (T112).
//!
//! The dashboards page body: the board tab strip (with the add / import /
//! docs actions), the per-board toolbar (the rename field, the monitor
//! pause / refresh-all / export / delete buttons), the widget grid — one
//! T069 [`ChartWidgetTile`] per widget plus the dashed add-widget card —
//! the T175 [`RunHistoryDrawer`] mount, and the two delete confirmations.
//! Four early-return branches sit in front: the bootstrap spinner, the
//! empty-boards hero, and the missing-id line.
//!
//! ## What maps where
//!
//! - `useLocaleDomains(['dashboard'])` (the `localesReady` gate and its
//!   `LocaleDomainLoading` stand-in) is elided, the T018 ruling: the zh-CN
//!   table is compiled in by `build.rs`, so the panel never renders
//!   half-translated keys.
//! - `useDashboardStore` is a host concern, not Rune's: the entry
//!   projections (`current` / `runs` / `busyWidgets` / `loading` / `error` /
//!   `list` / `listLoading`) land in [`DashboardPanelState`] through the
//!   `deliver_*` methods, and every store action (`fetchDashboards`,
//!   `mountDashboard`, `loadDashboard`, `saveDashboard`, `deleteDashboard`,
//!   `refreshWidget`, `refreshAllWidgets`, `releaseDashboard`) is a host
//!   callback the panel fires. The refcounted mount (`mountDashboard` /
//!   `releaseDashboard` around the active id) is entirely the host's — the
//!   panel only asks for the board it displays.
//! - `openWidgetEditor`'s `aiCommands.workflowGet` / `workflowList` pair is
//!   the [`DashboardPanelState::editor_fetch_due`] poll: the host serves the
//!   armed workflow id and delivers [`EditorWorkflowVm`] (both answers at
//!   once — React awaits `workflowGet` then, in the visible branch,
//!   `workflowList`); the visibility branch and the `queryStep` find are
//!   ported verbatim in [`DashboardPanelState::deliver_editor_workflow`].
//! - The `dashboardCommands` calls (`exportWithDialog`,
//!   `importWithDialog`, `setDashboardRefreshPaused`,
//!   `createWidgetFromSql`, `updateHiddenWidgetSql`) and
//!   `openDocsWindow('opsDashboard')` / `openWorkflowWindow()` are host
//!   callbacks; `handleExport`'s filename sanitizer — `name.replace(/
//!   [^\w.-]+/g, '_') || 'dashboard'` — is ported verbatim as
//!   [`sanitize_export_stem`].
//! - `WidgetEditorDrawer` is **T201, not landed**: the panel keeps every
//!   piece of the drawer's state React's panel owns (`editorOpen`,
//!   `editingWidget`, `isNewWidget`, `editorHiddenSql`, `userWorkflows` —
//!   the same fields the workspace snapshot persists) and renders the
//!   host-built drawer through the [`DashboardPanel::editor_drawer`] slot,
//!   the T069 axis-configurator convention. The T201 host builds the drawer
//!   (mounting the T146 monitor-schedule authorization card and the T018
//!   add-to-dashboard dialog where React's does) and wires its `onSave` /
//!   `onClose` to [`DashboardPanelState::editor_saved`] /
//!   [`DashboardPanelState::close_editor`], plus the refresh dispatch
//!   (`refreshWidget` unless the widget was just created from hidden SQL).
//!   `onOpenWorkflowEditor` forwards into that slot; React's
//!   `openWorkflowWindow()` fallback is the host's.
//! - `createEmptyDashboard`'s `crypto.randomUUID()` and `new Date().
//!   toISOString()` are host-supplied ([`create_empty_dashboard`] takes
//!   both); `nextWidgetLayout` is ported verbatim
//!   ([`next_widget_layout`]).
//! - The workspace-panel snapshot (`viewStateRef` + the unmount
//!   `saveDashboardSnapshot`) is [`DashboardPanelState::snapshot`] /
//!   [`DashboardPanelState::mount`]; `ConnectionPage`'s
//!   `clearDashboardSnapshot` is the host passing `None`.
//! - The mount effect's bootstrap machine is
//!   [`DashboardPanelState::apply_mount_effect`] +
//!   [`DashboardPanelState::deliver_list`]; the board switch's
//!   `onDashboardChange` fires from [`DashboardPanelState::switch_dashboard`]
//!   and the rename-sync effect lives behind
//!   [`DashboardPanelState::title_sync_due`] / `mark_title_synced` (the
//!   host polls after each delivery).
//! - State mutation reaches the owning state `S` through the
//!   [`PanelStateFn`] projection (the T023-S2 `EditorFn` convention); the
//!   IPC-shaped actions are the builder's named slots. Without either, the
//!   panel renders but its presses no-op — the not-yet-wired host's case,
//!   the tile precedent.
//! - The per-tile `chartData` memo (the T030/T182 transform seam, host-fed)
//!   arrives through [`DashboardPanel::tile_chart_data`]; the run-history
//!   drawer's through [`DashboardPanel::history_chart_data`]. While the
//!   overlay's local config equals the widget's, the tile reuses the tile
//!   data (T069's documented fallback), which is what React's
//!   recomputation yields.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; derived keys follow the
//! port's prefix convention — the owning testid is the prefix, one
//! consistent family per subtree (a board tab merges its testid with its
//! `data-dashboard-id`, the T069 `dashboard-tile-{id}` rule):
//!
//! | part | key |
//! |------|-----|
//! | panel root (all four branches, testid) | `dashboard-panel` |
//! | bootstrapping block (testid) | `dashboard-bootstrapping` (+ `-spinner`, `-text`) |
//! | empty-boards block (testid) | `dashboard-empty-boards` (+ `-icon`, `-text`) |
//! | create-first button (testid) | `dashboard-create-first` (+ `-icon`, `-text`) |
//! | missing-id line (derived) | `dashboard-panel-missing-id` (+ `-text`) |
//! | tab-bar outer row / actions (derived) | `dashboard-tab-bar-row` / `dashboard-tab-bar-actions` |
//! | tab strip (testid) | `dashboard-tab-bar` |
//! | a board tab (testid + `data-dashboard-id`) | `dashboard-tab-{board.id}` (+ `-content`, `-name`, `-underline`) |
//! | tab add / import / docs (testids) | `dashboard-tab-add` / `dashboard-import` / `dashboard-docs-help` |
//! | toolbar row / title half / actions (derived) | `dashboard-panel-toolbar` / `-title` / `-actions` |
//! | rename input / trigger (testids) | `dashboard-rename-input` / `dashboard-rename` (+ `-name`, `-icon`) |
//! | pause / refresh-all / export / delete (testids) | `dashboard-pause-toggle` / `dashboard-refresh-all` (+ `-spinner`) / `dashboard-export` / `dashboard-delete-panel` |
//! | main content (testid) | `dashboard-main` |
//! | loading block / error banner (derived) | `dashboard-main-loading` (+ `-spinner`, `-text`) / `dashboard-main-error` (+ `-text`, from `ui/banner.rs`) |
//! | grid (testid) | `dashboard-grid` |
//! | a tile cell (derived) | `dashboard-grid-cell-{widget.id}` (then T069's `dashboard-tile-{id}` family) |
//! | add-widget button (testid) | `dashboard-add-widget` (+ `-icon`, `-text`) |
//! | delete-panel dialog (derived) | `dashboard-panel-delete-dialog` (+ `-text`, `-cancel`, shell keys from `ui/dialog.rs`), confirm `dashboard-delete-confirm` (testid) |
//! | widget-delete dialog (derived) | `dashboard-panel-widget-delete-dialog` (+ `-text`, `-cancel`), confirm `widget-delete-confirm` (testid — React's literal has no `dashboard-` prefix) |
//! | run-history drawer / editor seam | T175's `run-history-drawer` family / the host slot's own keys |
//!
//! Known deltas (documented, not silent): the grid's `repeat(auto-fill,
//! minmax(420px, 1fr))` is a wrapping row of 420pt cells — the `1fr`
//! stretch is the `wapp_card` precedent's simplification; the add-widget
//! card's `border-dashed` renders solid (Rune has no dash); its
//! `hover:border-accent/40` keeps the static border with the `bg-accent/5`
//! hover fill and the accent ink swap; the tab underline is an
//! always-present 2pt in-flow row (transparent when inactive, the T174
//! ruling — a tab stands 2pt taller than React's overlaid bar); the
//! toolbar's icon-only `Button`s are hand-rolled 28pt ghost rows (the
//! T069 `plain_button` shape at React's `h-7 w-7`, glyphs 14pt); the
//! rename input's `onBlur` commit has no Rune counterpart (Enter commits,
//! Escape cancels, both through the field's key handler); the two
//! dialogs use the kit `Dialog` defaults (`fit`, the T018 mapping) rather
//! than Tailwind's unclassed `Dialog` shell; `transition-colors` /
//! `transition-opacity` have no counterpart (the standard hover overlay
//! cross-fades on its own).

mod chrome;
mod state;

#[allow(unused_imports)] // the audit and the future host consume the flow enums
pub use state::{DashboardPanelState, EditorOpenFlow, MountArm};

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, icon, row, text, El, ScrollSpec, TextStyle};
use rune_kit::Kit;

use crate::components::chart::chart_canvas::{ChartConfig, ChartDataPoint, ChartType};
use crate::i18n::t;
use crate::icons::{ICON_DASHBOARDS, ICON_PAUSE, ICON_PLAY, ICON_PLUS};
use crate::ui::banner::{Banner, BannerVariant};
use crate::ui::field::TextField;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize};
use crate::views::dashboard::chart_widget_tile::{
    ChartWidgetTile, ChartWidgetTileState, ChartWidgetVm, TileViewMode,
};
use crate::views::dashboard::run_history_drawer::RunHistoryDrawer;

// ---- Geometry (Tailwind, verbatim) ------------------------------------------

/// React grid `gap-3` (and the empty / add cards' `gap-3`).
const GRID_GAP: f32 = 12.0;
/// React `gridTemplateColumns: repeat(auto-fill, minmax(420px, 1fr))` — the
/// column floor (the wrap-row basis).
const GRID_MIN_COL: f32 = 420.0;
/// React main content `p-4`.
const MAIN_PAD: f32 = 16.0;
/// React add-widget card `min-h-[320px]` and its `Plus h-8 w-8`.
const ADD_CARD_MIN_H: f32 = 320.0;
const ADD_ICON: f32 = 32.0;
/// React empty-boards `Gauge h-10 w-10 opacity-40` and the block's `gap-3`.
const EMPTY_ICON: f32 = 40.0;
const EMPTY_ICON_ALPHA: f32 = 0.4;
const EMPTY_GAP: f32 = 12.0;
/// React spinner rows `mr-2`.
const SPINNER_GAP: f32 = 8.0;
/// React error banner `mb-3`.
const ERROR_MB: f32 = 12.0;
/// React create-first button `h-7 gap-1 px-2`, icon `h-3.5 w-3.5`.
const CREATE_FIRST_H: f32 = 28.0;
const CREATE_FIRST_GAP: f32 = 4.0;
const CREATE_FIRST_PX: f32 = 8.0;
const CREATE_FIRST_ICON: f32 = 14.0;

/// React `text-xs` (12px / 400).
fn type_xs() -> TextStyle {
    TextStyle::new(12.0, 400.0)
}

/// React `text-sm` (14px / 400).
fn type_sm() -> TextStyle {
    TextStyle::new(14.0, 400.0)
}

// ---- View models ------------------------------------------------------------

/// The slice of React's `Dashboard` the tab strip reads (the store's `list`
/// entries; the host converts the wire type, the T018 `DashboardOptionVm`
/// rule).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardListItemVm {
    pub id: String,
    pub name: String,
}

/// React `WidgetLayout` (`{ x, y, w, h }`).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetLayoutVm {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

/// React `RefreshMode` (`'manual' | 'onOpen' | 'interval'`).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RefreshModeVm {
    #[default]
    Manual,
    OnOpen,
    Interval,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl RefreshModeVm {
    /// Parse the wire spelling (`onOpen`, the camel-case literal).
    pub fn from_str(value: &str) -> Self {
        match value {
            "onOpen" => Self::OnOpen,
            "interval" => Self::Interval,
            _ => Self::Manual,
        }
    }
}

/// React `RefreshPolicy`; the default is React's `DEFAULT_REFRESH`
/// (`{ mode: 'manual' }`).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RefreshPolicyVm {
    pub mode: RefreshModeVm,
    pub refresh_sec: Option<u32>,
}

/// React `DashboardWidget`, the panel-flavoured slice: the T069 tile's
/// fields plus the panel's own business (`workflowId`, `layout`, `refresh`).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq)]
pub struct PanelWidgetVm {
    pub id: String,
    pub title: String,
    pub workflow_id: String,
    pub view_mode: TileViewMode,
    pub chart_config: Option<ChartConfig>,
    pub layout: WidgetLayoutVm,
    pub refresh: RefreshPolicyVm,
    pub enabled: bool,
}

impl PanelWidgetVm {
    /// The T069 tile's projection of this widget (id / title / view mode /
    /// chart config / enabled — every field the tile reads).
    pub fn tile_vm(&self) -> ChartWidgetVm {
        ChartWidgetVm {
            id: self.id.clone(),
            title: self.title.clone(),
            view_mode: self.view_mode,
            chart_config: self.chart_config.clone(),
            enabled: self.enabled,
        }
    }
}

/// React `Dashboard` (the panel-flavoured slice the panel and its saves
/// need; the host converts the wire type).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq)]
pub struct DashboardVm {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub layout_cols: u32,
    pub layout_row_height: u32,
    pub widgets: Vec<PanelWidgetVm>,
    pub enabled: bool,
    pub refresh_paused: bool,
}

/// React `editorHiddenSql` (`{ connectionId, sql }`) — also the T201 seam's
/// `hiddenSql` prop.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HiddenSqlVm {
    pub connection_id: String,
    pub sql: String,
}

/// The slice of React's `WorkflowListItem` the panel stores for the editor
/// drawer (T201 extends it when it lands).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorkflowListItemVm {
    pub id: String,
    pub name: String,
}

/// One step of the `workflowGet` answer the panel's find reads.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EditorWorkflowStepVm {
    /// React's `s.type` (the find matches `'query'`).
    pub kind: String,
    pub connection: Option<String>,
    pub sql: Option<String>,
}

/// The `workflowGet` + `workflowList` answers `openWidgetEditor` awaits,
/// delivered together (the host runs both fetches before delivering).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq)]
pub struct EditorWorkflowVm {
    /// The workflow's visibility (`'dashboardHidden'` or anything else).
    pub visibility: String,
    /// `wf.connection` (`None` is React's `undefined`).
    pub connection: Option<String>,
    /// `wf.steps` — the `type === 'query'` find runs on it.
    pub steps: Vec<EditorWorkflowStepVm>,
    /// The `workflowList` answer (the visible-workflow branch's list).
    pub user_workflows: Vec<WorkflowListItemVm>,
}

/// The `DashboardWorkspaceSnapshot` (`workspacePanelStateStore.ts`, lines
/// 27–38): the recreatable view state the mode-switch remount restores.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq)]
pub struct DashboardSnapshotVm {
    pub active_dashboard_id: String,
    pub editor_open: bool,
    pub editing_widget: Option<PanelWidgetVm>,
    pub is_new_widget: bool,
    pub editor_hidden_sql: Option<HiddenSqlVm>,
    pub user_workflows: Vec<WorkflowListItemVm>,
    pub history_open: bool,
    pub history_widget: Option<PanelWidgetVm>,
    pub renaming: bool,
    pub name_draft: String,
}

// ---- Pure helpers -----------------------------------------------------------

/// React's `createEmptyDashboard(name)` (lines 36–47): the host supplies the
/// `crypto.randomUUID()` id and the `new Date().toISOString()` stamp — both
/// are host concerns, the `on_begin_create` ruling of the AI settings port.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub fn create_empty_dashboard(id: &str, name: &str, now_iso: &str) -> DashboardVm {
    DashboardVm {
        id: id.to_owned(),
        name: name.to_owned(),
        created_at: now_iso.to_owned(),
        updated_at: now_iso.to_owned(),
        layout_cols: 12,
        layout_row_height: 80,
        widgets: Vec::new(),
        enabled: true,
        refresh_paused: false,
    }
}

/// React's `nextWidgetLayout` (lines 49–52), verbatim: below the deepest
/// widget's bottom edge, half the row width, four rows tall.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub fn next_widget_layout(widgets: &[PanelWidgetVm]) -> WidgetLayoutVm {
    let max_y = widgets
        .iter()
        .map(|widget| widget.layout.y + widget.layout.h)
        .max()
        .unwrap_or(0);
    WidgetLayoutVm {
        x: 0,
        y: max_y,
        w: 6,
        h: 4,
    }
}

/// React's `handleExport` filename stem (line 395):
/// `name.replace(/[^\w.-]+/g, '_') || 'dashboard'` — every maximal run of
/// characters outside `\w` (`[A-Za-z0-9_]`), `.` and `-` collapses into one
/// `_`; only an empty result (the empty name — any non-word name yields at
/// least `_`) falls back to `dashboard`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub fn sanitize_export_stem(name: &str) -> String {
    let mut out = String::new();
    let mut in_bad_run = false;
    for ch in name.chars() {
        let keep = ch.is_ascii_alphanumeric() || ch == '_' || ch == '.' || ch == '-';
        if keep {
            out.push(ch);
            in_bad_run = false;
        } else if !in_bad_run {
            out.push('_');
            in_bad_run = true;
        }
    }
    if out.is_empty() {
        "dashboard".to_owned()
    } else {
        out
    }
}

// ---- Builder ----------------------------------------------------------------

/// The `S` → panel-state projection (the T023-S2 `EditorFn` convention): the
/// panel's own handlers mutate the state through it, so the host wires one
/// closure instead of one per control.
pub type PanelStateFn<S> = Rc<dyn Fn(&mut S) -> &mut DashboardPanelState>;

type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type WidgetIdFn<S> = Rc<dyn Fn(&mut S, String) + 'static>;
type BoardFn<S> = Rc<dyn Fn(&mut S, DashboardVm) + 'static>;
type ChangeFn<S> = Rc<dyn Fn(&mut S, &str, &str) + 'static>;
type PauseFn<S> = Rc<dyn Fn(&mut S, bool) + 'static>;

/// Builder for the panel, generic over the app state `S` that owns the
/// [`DashboardPanelState`] slice (the ported-dialog pattern of T018/T069/
/// T146/T175, composed: the grid instantiates one T069 tile per widget and
/// the layer carries the T175 drawer).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub struct DashboardPanel<'a, S: 'static> {
    state: &'a DashboardPanelState,
    /// The state projection every view-initiated transition flows through.
    state_fn: Option<PanelStateFn<S>>,
    /// The T030/T182 transform seam, per widget (the tile's `chartData`
    /// memo output for `widget.chartConfig`).
    tile_chart_data: Option<&'a HashMap<String, Vec<ChartDataPoint>>>,
    /// The drawer's transform seam (the memo output for `(loadedRun,
    /// historyWidget.chartConfig)`).
    history_chart_data: Option<&'a [ChartDataPoint]>,
    /// The T201 seam: a host-built element standing in for the
    /// `WidgetEditorDrawer` (pushed on the layer, React's `fixed inset-0
    /// z-50 flex justify-end` drawer shape; the host's port nulls itself
    /// while closed, as React's does).
    editor_drawer: Option<El>,
    /// The rename field's `on_edit` projection (the T018 `name_edit`
    /// convention — the field needs the `TextEdit` accessor).
    rename_edit: Option<Box<dyn FnOnce(TextField<'a>) -> TextField<'a>>>,
    /// React `onDashboardChange` — fired by the board switch and the
    /// delete-restore (the rename-sync effect's fires are the host's
    /// `title_sync_due` poll).
    on_dashboard_change: Option<ChangeFn<S>>,
    /// `handleCreatePanel` (the host generates the uuid, saves, re-fetches
    /// and switches).
    on_create_panel: Option<ActionFn<S>>,
    /// `handleCreateFirstBoard` (the same flow with the default name).
    on_create_first_board: Option<ActionFn<S>>,
    /// `handleImport` (the host runs `importWithDialog` and either reloads
    /// the current board or switches to the imported one).
    on_import: Option<ActionFn<S>>,
    /// The docs button's `openDocsWindow('opsDashboard')`.
    on_open_docs: Option<ActionFn<S>>,
    /// `handleToggleMonitorPause(next)` — the host persists, applies
    /// `set_monitor_paused` and reloads (React awaits the IPC first).
    on_toggle_monitor_pause: Option<PauseFn<S>>,
    /// `handleRefreshAll`'s IPC half (the state half is the panel's
    /// `begin_refresh_all`; the host delivers `end_refresh_all`).
    on_refresh_all: Option<ActionFn<S>>,
    /// `handleExport` (the host sanitizes via [`sanitize_export_stem`] and
    /// runs `exportWithDialog`).
    on_export: Option<ActionFn<S>>,
    /// The rename commit's save half (the host reads `commit_rename`'s
    /// name and saves).
    on_rename_commit: Option<ActionFn<S>>,
    /// `handleAddWidget`'s uuid half (the host generates the id and applies
    /// `begin_add_widget`).
    on_add_widget: Option<ActionFn<S>>,
    /// The tile's `onRefresh` — `refreshWidget(current.id, widget.id)`.
    on_refresh_widget: Option<WidgetIdFn<S>>,
    /// The view-mode / chart-type / chart-config saves
    /// (`saveDashboard` with the one widget patched).
    on_save_board: Option<BoardFn<S>>,
    /// `handleDeletePanel`'s IPC half (the host deletes and delivers the
    /// remaining list through `panel_deleted`).
    on_delete_panel: Option<ActionFn<S>>,
    /// `handleDeleteWidget`'s save half (the host saves
    /// `board_without_widget`).
    on_delete_widget: Option<WidgetIdFn<S>>,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl<'a, S: 'static> DashboardPanel<'a, S> {
    /// The panel over its state slice; every slot below is optional (the
    /// not-yet-wired host renders but nothing fires).
    pub fn new(state: &'a DashboardPanelState) -> Self {
        Self {
            state,
            state_fn: None,
            tile_chart_data: None,
            history_chart_data: None,
            editor_drawer: None,
            rename_edit: None,
            on_dashboard_change: None,
            on_create_panel: None,
            on_create_first_board: None,
            on_import: None,
            on_open_docs: None,
            on_toggle_monitor_pause: None,
            on_refresh_all: None,
            on_export: None,
            on_rename_commit: None,
            on_add_widget: None,
            on_refresh_widget: None,
            on_save_board: None,
            on_delete_panel: None,
            on_delete_widget: None,
        }
    }

    /// The state projection every view-initiated transition flows through.
    pub fn state_fn(mut self, f: PanelStateFn<S>) -> Self {
        self.state_fn = Some(f);
        self
    }

    /// The per-widget transform seam (the tile's `chartData` memo output).
    pub fn tile_chart_data(
        mut self,
        data: Option<&'a HashMap<String, Vec<ChartDataPoint>>>,
    ) -> Self {
        self.tile_chart_data = data;
        self
    }

    /// The run-history drawer's transform seam.
    pub fn history_chart_data(mut self, data: Option<&'a [ChartDataPoint]>) -> Self {
        self.history_chart_data = data;
        self
    }

    /// The T201 `WidgetEditorDrawer` seam (host-built, layer-pushed).
    pub fn editor_drawer(mut self, slot: El) -> Self {
        self.editor_drawer = Some(slot);
        self
    }

    /// The rename field's `on_edit` projection (the host hands the field its
    /// `TextEdit` accessor, the T018 convention).
    pub fn rename_edit(
        mut self,
        f: impl FnOnce(TextField<'a>) -> TextField<'a> + 'static,
    ) -> Self {
        self.rename_edit = Some(Box::new(f));
        self
    }

    /// React `onDashboardChange(id, name)`.
    pub fn on_dashboard_change(mut self, f: impl Fn(&mut S, &str, &str) + 'static) -> Self {
        self.on_dashboard_change = Some(Rc::new(f));
        self
    }

    /// `handleCreatePanel`.
    pub fn on_create_panel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_create_panel = Some(Rc::new(f));
        self
    }

    /// `handleCreateFirstBoard`.
    pub fn on_create_first_board(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_create_first_board = Some(Rc::new(f));
        self
    }

    /// `handleImport`.
    pub fn on_import(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_import = Some(Rc::new(f));
        self
    }

    /// The docs-help press (`openDocsWindow('opsDashboard')`).
    pub fn on_open_docs(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_open_docs = Some(Rc::new(f));
        self
    }

    /// `handleToggleMonitorPause(next)` — the host receives the computed
    /// `next` (React's `!monitorPaused`).
    pub fn on_toggle_monitor_pause(mut self, f: impl Fn(&mut S, bool) + 'static) -> Self {
        self.on_toggle_monitor_pause = Some(Rc::new(f));
        self
    }

    /// `handleRefreshAll`'s IPC half.
    pub fn on_refresh_all(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_refresh_all = Some(Rc::new(f));
        self
    }

    /// `handleExport`.
    pub fn on_export(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_export = Some(Rc::new(f));
        self
    }

    /// The rename commit's save half (Enter; the blur arm has no Rune
    /// counterpart — a documented delta).
    pub fn on_rename_commit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_rename_commit = Some(Rc::new(f));
        self
    }

    /// `handleAddWidget`'s uuid half.
    pub fn on_add_widget(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_add_widget = Some(Rc::new(f));
        self
    }

    /// The tile refresh press — `refreshWidget(current.id, widget.id)`.
    pub fn on_refresh_widget(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_refresh_widget = Some(Rc::new(f));
        self
    }

    /// The board save for a view-mode / chart-type / chart-config change.
    pub fn on_save_board(mut self, f: impl Fn(&mut S, DashboardVm) + 'static) -> Self {
        self.on_save_board = Some(Rc::new(f));
        self
    }

    /// `handleDeletePanel`'s IPC half.
    pub fn on_delete_panel(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_delete_panel = Some(Rc::new(f));
        self
    }

    /// `handleDeleteWidget`'s save half.
    pub fn on_delete_widget(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_delete_widget = Some(Rc::new(f));
        self
    }

    /// Builds the panel root; the drawers and dialogs ride `layer` (the
    /// `createPortal` convention), in React's DOM order — editor seam, run
    /// history, then the two dialogs. The three early-return branches build
    /// no layer children at all (React returns before the mounts).
    pub fn build(mut self, dark: bool, layer: &mut Vec<El>) -> El {
        let state = self.state;
        // React's early returns, in order (the locale gate is elided — the
        // T018 ruling).
        if state.bootstrapping
            || (state.list_loading
                && state.active_dashboard_id.is_empty()
                && state.list.is_empty())
        {
            return self.panel_root().child(self.bootstrapping_block(dark));
        }
        if state.active_dashboard_id.is_empty() && state.list.is_empty() {
            return self.panel_root().child(self.empty_boards_block(dark));
        }
        if state.active_dashboard_id.is_empty() {
            return self.panel_root().child(self.missing_id_line());
        }
        let k = kit(dark);
        let rename_edit = self.rename_edit.take();
        let mut root = self.panel_root();
        if !state.list.is_empty() {
            root = root.child(self.tab_bar(&k));
        }
        if state.current.is_some() {
            root = root.child(self.toolbar(&k, dark, rename_edit));
        }
        root = root.child(self.main_content(&k, dark, layer));
        // React's DOM order: the editor drawer, the run-history drawer, then
        // the two confirm dialogs (later siblings paint on top).
        if let Some(editor) = self.editor_drawer.take() {
            layer.push(editor);
        }
        self.history_drawer_mount(dark, layer);
        self.delete_dialogs(&k, layer);
        root
    }

    /// React's root (`flex h-full min-h-0 flex-col`, the testid — the three
    /// early-return branches share it with `flex h-full flex-col`, one
    /// column for both).
    fn panel_root(&self) -> El {
        col().key("dashboard-panel").w_full().h_full().min_h(0.0)
    }

    /// The bootstrap branch (lines 485–497): the full-height centered
    /// spinner line.
    fn bootstrapping_block(&self, dark: bool) -> El {
        row()
            .key("dashboard-bootstrapping")
            .grow(1.0)
            .items_center()
            .justify_center()
            .gap(SPINNER_GAP) // React mr-2
            .child(
                Spinner::new("dashboard-bootstrapping-spinner")
                    .size(SpinnerSize::Lg) // React size="lg"
                    .build(dark),
            )
            .child(
                text(t("common.loading"))
                    .key("dashboard-bootstrapping-text")
                    .font(type_sm()) // text-sm
                    .color(kit(dark).c.text_muted), // text-fg-muted
            )
    }

    /// The empty-boards branch (lines 499–519): the gauge glyph, the line,
    /// the create-first button.
    fn empty_boards_block(&self, dark: bool) -> El {
        let k = kit(dark);
        let mut create = row()
            .key("dashboard-create-first")
            .role("button")
            .label(t("dashboard.createFirstBoard"))
            .items_center()
            .gap(CREATE_FIRST_GAP) // React gap-1
            .px(CREATE_FIRST_PX) // px-2
            .h(CREATE_FIRST_H) // h-7
            .radius(k.radius.md) // the Button's rounded-md
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus)
            .child(
                icon(&ICON_PLUS, CREATE_FIRST_ICON) // h-3.5 w-3.5
                    .key("dashboard-create-first-icon")
                    .color(k.c.text_base),
            )
            .child(
                text(t("dashboard.createFirstBoard"))
                    .key("dashboard-create-first-text")
                    .font(type_xs()) // text-xs
                    .color(k.c.text_base),
            );
        if let Some(create_first) = &self.on_create_first_board {
            let create_first = Rc::clone(create_first);
            create = create.on_press(move |s: &mut S| create_first(s));
        }
        col()
            .key("dashboard-empty-boards")
            .grow(1.0)
            .items_center()
            .justify_center()
            .gap(EMPTY_GAP) // gap-3
            .child(
                icon(&ICON_DASHBOARDS, EMPTY_ICON) // React Gauge h-10 w-10
                    .key("dashboard-empty-boards-icon")
                    .color(k.c.text_muted)
                    .opacity(EMPTY_ICON_ALPHA), // opacity-40
            )
            .child(
                text(t("dashboard.emptyBoards"))
                    .key("dashboard-empty-boards-text")
                    .font(type_sm()) // text-sm
                    .color(k.c.text_muted), // text-fg-muted
            )
            .child(create)
    }

    /// The missing-id branch (lines 521–529): the centered line.
    fn missing_id_line(&self) -> El {
        row()
            .key("dashboard-panel-missing-id")
            .grow(1.0)
            .items_center()
            .justify_center()
            .child(
                text(t("dashboard.missingId"))
                    .key("dashboard-panel-missing-id-text")
                    .font(type_sm()) // text-sm
                    .color(kit(true).c.text_muted), // text-fg-muted
            )
    }

    /// The main content (lines 434–483): the scroll column with the loading
    /// line, the error banner and the grid. `layer` carries the tiles'
    /// expand overlays (the `createPortal` convention — a scroll-column
    /// child would cull a pinned element).
    fn main_content(&self, k: &Kit, dark: bool, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let mut main = col()
            .key("dashboard-main")
            .w_full()
            .grow(1.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false }) // overflow-auto
            .px(MAIN_PAD) // p-4
            .py(MAIN_PAD);
        if state.loading && state.current.is_none() && !state.active_dashboard_id.is_empty() {
            main = main.child(
                row()
                    .key("dashboard-main-loading")
                    .h_full()
                    .items_center()
                    .justify_center()
                    .gap(SPINNER_GAP) // React mr-2
                    .child(
                        Spinner::new("dashboard-main-loading-spinner")
                            .size(SpinnerSize::Lg) // React size="lg"
                            .build(dark),
                    )
                    .child(
                        text(t("common.loading"))
                            .key("dashboard-main-loading-text")
                            .font(type_sm()) // text-sm
                            .color(k.c.text_muted), // text-fg-muted
                    ),
            );
        }
        if let Some(error) = &state.error {
            // React's `mb-3 rounded-md border border-red-500/30 bg-red-500/10
            // px-3 py-2 text-sm text-red-400` — the boxed danger banner (the
            // T175 index-error mapping; the banner's uniform 8pt padding
            // stands in for `px-3 py-2`).
            if let Some(banner) = Banner::new("dashboard-main-error", error.clone())
                .variant(BannerVariant::Boxed)
                .build(k)
            {
                main = main.child(banner.mb(ERROR_MB));
            }
        }
        if let Some(current) = &state.current {
            main = main.child(self.grid(k, dark, current, layer));
        }
        main
    }

    /// The widget grid (lines 446–481): one tile per widget plus the
    /// add-widget card, in a wrapping row of 420pt cells.
    fn grid(&self, k: &Kit, dark: bool, current: &DashboardVm, layer: &mut Vec<El>) -> El {
        let mut grid = row()
            .key("dashboard-grid")
            .w_full()
            .flex_wrap() // repeat(auto-fill, minmax(420px, 1fr))
            .gap(GRID_GAP); // gap-3
        for widget in &current.widgets {
            grid = grid.child(self.tile_cell(dark, widget, layer));
        }
        grid.child(self.add_widget_card(k))
    }

    /// One grid cell: the T069 tile at the column floor. The tile's expand
    /// overlay (when due) rides `layer`, the tile builder's own convention.
    fn tile_cell(&self, dark: bool, widget: &PanelWidgetVm, layer: &mut Vec<El>) -> El {
        let state = self.state;
        let default_tile = ChartWidgetTileState::default();
        let tile_state = state.tile_states.get(&widget.id).unwrap_or(&default_tile);
        let tile_vm = widget.tile_vm();
        let mut tile = ChartWidgetTile::<S>::new(&tile_vm, tile_state)
            .run(state.runs.get(&widget.id).and_then(|run| run.as_ref()))
            .busy(state.busy_widgets.contains(&widget.id))
            .chart_data(
                self.tile_chart_data
                    .and_then(|data| data.get(&widget.id))
                    .map(Vec::as_slice),
            );
        if let Some(panel_state) = &self.state_fn {
            // onEdit — openWidgetEditor(widget, false); the workflowGet the
            // flow awaits becomes the editor_fetch_due poll.
            let state_fn = Rc::clone(panel_state);
            let edit_widget = widget.clone();
            tile = tile.on_edit(move |s: &mut S| {
                state_fn(s).begin_edit_widget(edit_widget.clone(), false);
            });
            // onDelete — the widget-delete dialog opens.
            let state_fn = Rc::clone(panel_state);
            let id = widget.id.clone();
            tile = tile.on_delete(move |s: &mut S| state_fn(s).request_delete_widget(&id));
            // onHistory — the run-history drawer opens (its index fetch
            // arms inside).
            let state_fn = Rc::clone(panel_state);
            let history_widget = widget.clone();
            tile = tile.on_history(move |s: &mut S| state_fn(s).open_history(&history_widget));
            // onExpand / onCollapse — React's internal overlay state.
            let state_fn = Rc::clone(panel_state);
            let expand_widget = widget.clone();
            tile = tile.on_expand(move |s: &mut S| {
                state_fn(s)
                    .tile_state_mut(&expand_widget.id)
                    .begin_expand(&expand_widget.tile_vm());
            });
            let state_fn = Rc::clone(panel_state);
            let id = widget.id.clone();
            tile = tile.on_collapse(move |s: &mut S| state_fn(s).tile_state_mut(&id).collapse());
            // The three board patches: the save payload is built here, the
            // local current stays untouched until the host's save lands
            // (React's store patches on resolve).
            if let Some(save_slot) = &self.on_save_board {
                let state_fn = Rc::clone(panel_state);
                let id = widget.id.clone();
                let save_board = Rc::clone(save_slot);
                tile = tile.on_view_mode_change(move |s: &mut S, mode: TileViewMode| {
                    if let Some(board) = state_fn(s).board_with_view_mode(&id, mode) {
                        save_board(s, board);
                    }
                });
                let state_fn = Rc::clone(panel_state);
                let id = widget.id.clone();
                let save_board = Rc::clone(save_slot);
                tile = tile.on_chart_type_change(move |s: &mut S, chart_type: ChartType| {
                    if let Some(board) = state_fn(s).board_with_chart_type(&id, chart_type) {
                        save_board(s, board);
                    }
                });
                let state_fn = Rc::clone(panel_state);
                let id = widget.id.clone();
                let save_board = Rc::clone(save_slot);
                tile = tile.on_chart_config_change(move |s: &mut S, config: ChartConfig| {
                    if let Some(board) = state_fn(s).board_with_chart_config(&id, config) {
                        save_board(s, board);
                    }
                });
            }
        }
        if let Some(refresh_slot) = &self.on_refresh_widget {
            let refresh_widget = Rc::clone(refresh_slot);
            let id = widget.id.clone();
            tile = tile.on_refresh(move |s: &mut S| refresh_widget(s, id.clone()));
        }
        let tile_el = tile.build(dark, layer);
        col()
            .key(format!("dashboard-grid-cell-{}", widget.id))
            .w(GRID_MIN_COL) // the minmax(420px, 1fr) floor
            .child(tile_el)
    }

    /// The dashed add-widget card (lines 470–479).
    fn add_widget_card(&self, k: &Kit) -> El {
        let disabled = self.state.loading;
        let mut card = col()
            .key("dashboard-add-widget")
            .w(GRID_MIN_COL) // the grid stretch (the 1fr simplification)
            .min_h(ADD_CARD_MIN_H) // min-h-[320px]
            .items_center()
            .justify_center()
            .gap(GRID_GAP) // gap-3
            .radius(k.radius.lg) // rounded-lg
            .border(2.0, k.alpha(k.c.border_base, 0.6)) // border-2 border-edge/60
            .child(
                icon(&ICON_PLUS, ADD_ICON) // h-8 w-8
                    .key("dashboard-add-widget-icon")
                    .color(k.c.text_muted)
                    .opacity(0.6), // React opacity-60
            )
            .child(
                text(t("dashboard.addWidget"))
                    .key("dashboard-add-widget-text")
                    .font(type_sm()) // text-sm
                    .color(k.c.text_muted) // text-fg-muted
                    .group_color(k.c.text_accent), // hover:text-accent
            );
        if disabled {
            // React `disabled={loading}`: the control is inert.
            return card;
        }
        card = card
            .role("button")
            .label(t("dashboard.addWidget"))
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus)
            .hover_group()
            .hover(k.alpha(k.c.bg_accent, 0.05)); // hover:bg-accent/5
        if let Some(add_widget) = &self.on_add_widget {
            let add_widget = Rc::clone(add_widget);
            card = card.on_press(move |s: &mut S| add_widget(s));
        }
        card
    }

    /// The T175 drawer mount (lines 699–707): the panel's history widget
    /// feeds the drawer builder, its state the drawer state.
    fn history_drawer_mount(&self, dark: bool, layer: &mut Vec<El>) {
        let state = self.state;
        let Some(widget) = &state.history_widget else {
            return;
        };
        let tile_vm = widget.tile_vm();
        let mut drawer = RunHistoryDrawer::<S>::new(&state.history_drawer, &tile_vm)
            .chart_data(self.history_chart_data);
        if let Some(panel_state) = &self.state_fn {
            let state_fn = Rc::clone(panel_state);
            drawer = drawer.on_close(move |s: &mut S| state_fn(s).close_history());
            let state_fn = Rc::clone(panel_state);
            drawer = drawer.on_select(move |s: &mut S, id: String| {
                state_fn(s).history_drawer.select(&id);
            });
            let state_fn = Rc::clone(panel_state);
            drawer = drawer.on_view_mode_change(move |s: &mut S, mode: TileViewMode| {
                state_fn(s).history_drawer.set_view_mode(mode);
            });
        }
        drawer.build(dark, layer);
    }
}

/// The play / pause glyph pair the toolbar's monitor toggle swaps (React
/// lines 640–645) — shared with the chrome half and the audit.
pub fn monitor_toggle_glyph(paused: bool) -> &'static rune::Icon {
    if paused {
        &ICON_PLAY
    } else {
        &ICON_PAUSE
    }
}
