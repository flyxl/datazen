//! Independent audit for T112 (the DashboardPanel Rune port).
//!
//! Tester-written: the coder's module is a black box and every assertion
//! below re-reads `packages/frontend/src/windows/dashboard/DashboardPanel.tsx`.
//! The React tree has no behavioral `DashboardPanel.test.tsx` — its only
//! suite is `__tests__/DashboardPanel.firstFrameI18n.test.tsx`, the
//! locale-gate false-positive guard whose `dashboard-locale-loading`
//! stand-in the port elides per the T018 ruling (the zh-CN table is compiled
//! in, so the guarded body renders on frame one). This audit mirrors that
//! suite's surviving journey — a hot store renders `dashboard-main` with the
//! tab strip, toolbar and grid — and then walks the panel's own React
//! branches and handlers: the four early returns, the board tab strip, the
//! toolbar's rename + four icon actions, the widget grid of composed T069
//! tiles plus the add-widget card, the two delete confirmations, and the
//! state machine the host drives (the bootstrap epochs, board switching,
//! the rename commit, the editor flows, the run-history mount, the
//! snapshot round-trip, the pure helpers).
//!
//! The T201 `WidgetEditorDrawer` is a documented seam: the audit's composite
//! host plays it exactly as the port's module doc prescribes — the
//! [`DashboardPanel::editor_drawer`] slot is supplied only while
//! `editor_open` (React's drawer renders null while closed), its root
//! carries React's T201 testid `widget-editor-drawer`, it mounts the T146
//! [`MonitorScheduleAuthorization`] card under React WidgetEditorDrawer's own
//! condition (lines 313–323: a dashboard id, a widget, `!isNew`, refresh
//! mode `interval`), and the add-widget journey opens the T018
//! [`AddToDashboardDialog`] on the panel's layer (the port's T201
//! composition). The T175 run-history drawer mounts through the panel's own
//! `open_history` wiring; its entry keys are the drawer code's spelling
//! (`run-history-drawer-entry-{id}`, the accepted T175 audit's form).
//!
//! Element evidence comes from `Runtime::elements()` (runtime layout),
//! never from a value the component reports about itself; presses go
//! through real pointer events (move + frame before down/up), typing
//! through Cmd+A + paste, and every state change through `rt.update`.

use std::collections::HashMap;
use std::rc::Rc;

use rune::{col, row, Cx, El, Env, KeyEvent, Key, Mods, Runtime, TextEdit};

use crate::components::chart::chart_canvas::{
    ChartConfig, ChartDataPoint, ChartType, ChartValue as CanvasValue,
};
use crate::i18n::t;
use crate::icons::{ICON_PAUSE, ICON_PLAY};
use crate::support::chart_field_inference::ChartValue;
use crate::views::dashboard::add_to_dashboard_dialog::{
    AddToDashboardDialog, AddToDashboardState, DashboardOptionVm,
};
use crate::views::dashboard::chart_widget_tile::{
    tile_root_key, TileViewMode, WidgetRunStatus, WidgetRunVm,
};
use crate::views::dashboard::dashboard_panel::{
    create_empty_dashboard, monitor_toggle_glyph, next_widget_layout, sanitize_export_stem,
    BoardListItemVm, DashboardPanel, DashboardPanelState, DashboardVm, EditorOpenFlow,
    EditorWorkflowStepVm, EditorWorkflowVm, HiddenSqlVm, MountArm, PanelWidgetVm, RefreshModeVm,
    RefreshPolicyVm, WidgetLayoutVm, WorkflowListItemVm,
};
use crate::views::dashboard::monitor_schedule_authorization::{
    MonitorScheduleAuthorization, MonitorScheduleAuthorizationState,
};
use crate::views::dashboard::run_history_drawer::RunIndexEntryVm;

/// `Date.now()` for the T146 card's builds (the T146 audit's own clock).
const NOW_MS: i64 = 1_700_000_000_000;

// ---- Fixtures ---------------------------------------------------------------

fn chart_config() -> ChartConfig {
    ChartConfig {
        chart_type: ChartType::Bar,
        x_axis: Some("category".to_owned()),
        y_axes: vec!["amount".to_owned()],
        ..Default::default()
    }
}

fn widget(
    id: &str,
    title: &str,
    workflow_id: &str,
    mode: RefreshModeVm,
    y: u32,
    h: u32,
) -> PanelWidgetVm {
    PanelWidgetVm {
        id: id.to_owned(),
        title: title.to_owned(),
        workflow_id: workflow_id.to_owned(),
        view_mode: TileViewMode::Chart,
        chart_config: Some(chart_config()),
        layout: WidgetLayoutVm { x: 0, y, w: 6, h },
        refresh: RefreshPolicyVm { mode, refresh_sec: None },
        enabled: true,
    }
}

/// `d1` "Ops": w1 manual (workflow wf-1, bottom edge 4), w2 interval
/// (workflow wf-2, bottom edge 5).
fn board() -> DashboardVm {
    DashboardVm {
        id: "d1".to_owned(),
        name: "Ops".to_owned(),
        created_at: "2026-01-01T00:00:00Z".to_owned(),
        updated_at: "2026-01-01T00:00:00Z".to_owned(),
        layout_cols: 12,
        layout_row_height: 80,
        widgets: vec![
            widget("w1", "Revenue", "wf-1", RefreshModeVm::Manual, 0, 4),
            widget("w2", "Latency", "wf-2", RefreshModeVm::Interval, 2, 3),
        ],
        enabled: true,
        refresh_paused: false,
    }
}

fn board_list() -> Vec<BoardListItemVm> {
    vec![
        BoardListItemVm { id: "d1".to_owned(), name: "Ops".to_owned() },
        BoardListItemVm { id: "d2".to_owned(), name: "Drift".to_owned() },
    ]
}

fn ok_run() -> WidgetRunVm {
    WidgetRunVm {
        status: WidgetRunStatus::Ok,
        error: None,
        row_count: 2,
        columns: vec!["category".to_owned(), "amount".to_owned()],
        rows: vec![
            vec![ChartValue::String("Alpha".to_owned()), ChartValue::Number(100.0)],
            vec![ChartValue::String("Beta".to_owned()), ChartValue::Number(200.0)],
        ],
        alert_fired: false,
    }
}

/// The transform seam's delivery (the T030/T182 memo output for w1's run).
fn seam_points() -> Vec<ChartDataPoint> {
    vec![
        ChartDataPoint::new()
            .with("category", CanvasValue::Text("Alpha".to_owned()))
            .with("amount", CanvasValue::Number(100.0)),
        ChartDataPoint::new()
            .with("category", CanvasValue::Text("Beta".to_owned()))
            .with("amount", CanvasValue::Number(200.0)),
    ]
}

fn history_entry(id: &str) -> RunIndexEntryVm {
    RunIndexEntryVm {
        id: id.to_owned(),
        started_at: "2026-01-01T00:00:00Z".to_owned(),
        status: WidgetRunStatus::Ok,
        alert_fired: false,
    }
}

// ---- The composite host -----------------------------------------------------

#[derive(Clone)]
struct AuditState {
    panel: DashboardPanelState,
    /// The T146 card state the editor seam composes (the T201 host's slice).
    monitor: MonitorScheduleAuthorizationState,
    /// The T018 dialog state the add-widget journey opens.
    add_dialog: AddToDashboardState,
    /// The per-widget transform seam (the tile's `chartData` memo output).
    tile_points: HashMap<String, Vec<ChartDataPoint>>,
    /// The run-history drawer's transform seam.
    history_points: Vec<ChartDataPoint>,
    dark: bool,
    // The host slots the panel fires.
    dashboard_changes: Vec<(String, String)>,
    create_panels: usize,
    create_firsts: usize,
    imports: usize,
    docs_opens: usize,
    pause_toggles: Vec<bool>,
    refresh_alls: usize,
    exports: usize,
    rename_commits: usize,
    renamed_to: Option<String>,
    add_widgets: usize,
    refresh_widgets: Vec<String>,
    saved_boards: Vec<DashboardVm>,
    deleted_panels: usize,
    deleted_widgets: Vec<String>,
}

impl AuditState {
    fn shell(panel: DashboardPanelState) -> Self {
        Self {
            panel,
            monitor: MonitorScheduleAuthorizationState::default(),
            add_dialog: AddToDashboardState::default(),
            tile_points: HashMap::new(),
            history_points: Vec::new(),
            dark: true,
            dashboard_changes: Vec::new(),
            create_panels: 0,
            create_firsts: 0,
            imports: 0,
            docs_opens: 0,
            pause_toggles: Vec::new(),
            refresh_alls: 0,
            exports: 0,
            rename_commits: 0,
            renamed_to: None,
            add_widgets: 0,
            refresh_widgets: Vec::new(),
            saved_boards: Vec::new(),
            deleted_panels: 0,
            deleted_widgets: Vec::new(),
        }
    }

    /// The hot store (the React suite's own seeding): the explicit-open
    /// mount with the list, the current board and w1's run already landed.
    fn loaded() -> Self {
        let mut panel = DashboardPanelState::mount(Some("d1"), None);
        panel.deliver_list(board_list());
        let mut runs = HashMap::new();
        runs.insert("w1".to_owned(), Some(ok_run()));
        panel.deliver_dashboard(board(), runs);
        let mut scene = Self::shell(panel);
        scene.tile_points.insert("w1".to_owned(), seam_points());
        scene.history_points = seam_points();
        scene
    }

    /// The mount with nothing delivered (React's first frame, bootstrap arm).
    fn cold() -> Self {
        Self::shell(DashboardPanelState::mount(None, None))
    }

    /// The bootstrap fetch landed no boards.
    fn empty_boards() -> Self {
        let mut scene = Self::cold();
        scene.panel.deliver_list(Vec::new());
        scene
    }

    /// The drawer's seam gate: a loaded run plus the widget's config.
    fn history_seam(&self) -> Option<&[ChartDataPoint]> {
        if self.panel.history_drawer.loaded_run.is_some() {
            Some(self.history_points.as_slice())
        } else {
            None
        }
    }
}

/// The T201 stand-in the port's module doc prescribes: React's
/// `widget-editor-drawer` testid on an inset-0 justify-end overlay, with the
/// T146 card composed under React WidgetEditorDrawer's own mount condition
/// (lines 313–323, read from the panel's editor state).
fn editor_seam(s: &AuditState) -> El {
    let mut aside = col().key("widget-editor-drawer").w(560.0).h_full();
    if let Some(editing) = s.panel.editing_widget.as_ref() {
        if !s.panel.active_dashboard_id.is_empty()
            && !s.panel.is_new_widget
            && matches!(editing.refresh.mode, RefreshModeVm::Interval)
        {
            aside = aside.child(
                MonitorScheduleAuthorization::<AuditState>::new(&s.monitor, NOW_MS)
                    .on_authorize(|s: &mut AuditState| s.monitor.begin_change())
                    .on_revoke(|s: &mut AuditState| s.monitor.begin_change())
                    .build(s.dark),
            );
        }
    }
    row().absolute(0.0, 0.0).w_full().h_full().justify_end().child(aside)
}

fn view(s: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let mut panel = DashboardPanel::<AuditState>::new(&s.panel)
        .state_fn(Rc::new(|s: &mut AuditState| &mut s.panel))
        .tile_chart_data(Some(&s.tile_points))
        .history_chart_data(s.history_seam())
        .on_dashboard_change(|s: &mut AuditState, id: &str, name: &str| {
            s.dashboard_changes.push((id.to_owned(), name.to_owned()));
        })
        .on_create_panel(|s: &mut AuditState| s.create_panels += 1)
        .on_create_first_board(|s: &mut AuditState| s.create_firsts += 1)
        .on_import(|s: &mut AuditState| s.imports += 1)
        .on_open_docs(|s: &mut AuditState| s.docs_opens += 1)
        .on_toggle_monitor_pause(|s: &mut AuditState, next: bool| {
            s.pause_toggles.push(next);
            s.panel.set_monitor_paused(next);
        })
        .on_refresh_all(|s: &mut AuditState| s.refresh_alls += 1)
        .on_export(|s: &mut AuditState| s.exports += 1)
        .on_rename_commit(|s: &mut AuditState| {
            s.rename_commits += 1;
            s.renamed_to = s.panel.commit_rename();
        })
        .on_add_widget(|s: &mut AuditState| {
            s.add_widgets += 1;
            s.panel.begin_add_widget("w-new");
            // The T018 arm the port's T201 doc places in the composite host:
            // the add-widget journey opens the add-to-dashboard dialog.
            s.add_dialog.begin_open();
        })
        .on_refresh_widget(|s: &mut AuditState, id: String| s.refresh_widgets.push(id))
        .on_save_board(|s: &mut AuditState, board: DashboardVm| s.saved_boards.push(board))
        .on_delete_panel(|s: &mut AuditState| {
            s.deleted_panels += 1;
            let remaining: Vec<BoardListItemVm> =
                s.panel.list.iter().filter(|b| b.id != "d1").cloned().collect();
            if let Some(pair) = s.panel.panel_deleted(remaining) {
                s.dashboard_changes.push(pair);
            }
        })
        .on_delete_widget(|s: &mut AuditState, id: String| {
            s.deleted_widgets.push(id.clone());
            if let Some(board) = s.panel.board_without_widget(&id) {
                s.saved_boards.push(board);
            }
        });
    if s.panel.editor_open {
        panel = panel.editor_drawer(editor_seam(s));
    }
    let body = panel.build(s.dark, &mut layer);
    // The composite host's T018 dialog rides the same layer (nothing while
    // closed — React's `Dialog open={false}` renders null).
    AddToDashboardDialog::<AuditState>::new("add-to-dashboard", &s.add_dialog)
        .on_close(|s: &mut AuditState| s.add_dialog.close())
        .build(s.dark, &mut layer);
    let mut probe = col().key("probe-root").w_full().h_full();
    probe = probe.child(body);
    for child in layer {
        probe = probe.child(child);
    }
    probe
}

// ---- Runtime helpers ----------------------------------------------------------

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env { system_dark: true, scale: 1.0 },
    );
    // The panel and its layer children (dialogs, the drawer, the seam) lay
    // out over real frames; a single dt=0 frame leaves the layer unborn.
    settle(&mut rt);
    rt
}

fn maybe(rt: &Runtime<AuditState>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| !e.exiting && e.key.as_deref() == Some(key))
}

fn has(rt: &Runtime<AuditState>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn element_text(rt: &Runtime<AuditState>, key: &str) -> Option<String> {
    maybe(rt, key).and_then(|e| e.text.clone())
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    // Move onto the control first and let the clock run: hit-testing follows
    // the pointer, so a down without a prior move lands on nothing.
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
}

/// Replace a pre-filled field's text: click, Cmd+A, paste.
fn type_into(rt: &mut Runtime<AuditState>, key: &str, text: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
    rt.key(KeyEvent {
        key: Key::Command('a'),
        mods: Mods { cmd: true, ..Mods::default() },
    });
    rt.frame(1.0 / 60.0);
    rt.paste(text);
    rt.frame(1.0 / 60.0);
}

// ---- The state machine (pure) -------------------------------------------------

#[test]
fn the_bootstrap_machine_picks_the_first_board_and_honors_the_explicit_open() {
    let mut state = DashboardPanelState::mount(None, None);
    assert!(state.bootstrapping, "no prop and no snapshot bootstrap");
    assert_eq!(state.apply_mount_effect(None, false), MountArm::Bootstrap);
    state.begin_list_fetch();
    assert!(state.list_loading);
    state.deliver_list(board_list());
    assert!(!state.bootstrapping && !state.list_loading);
    assert_eq!(state.active_dashboard_id, "d1", "boards[0].id");
    assert_eq!(state.list.len(), 2);

    // The explicit open (ConnectionPage cleared the snapshot): the prop
    // wins, no bootstrap.
    let mut explicit = DashboardPanelState::mount(Some("d9"), None);
    assert!(!explicit.bootstrapping);
    assert_eq!(
        explicit.apply_mount_effect(Some("d9"), false),
        MountArm::ExplicitOpen
    );
    assert_eq!(explicit.active_dashboard_id, "d9");
}

#[test]
fn deliver_dashboard_syncs_the_draft_and_monitor_pause_and_keeps_the_error_contract() {
    let mut state = DashboardPanelState::mount(Some("d1"), None);
    state.deliver_dashboard_error("first".to_owned());
    assert_eq!(state.error.as_deref(), Some("first"));
    state.begin_dashboard_load();
    assert!(state.loading && state.error.is_none(), "loadDashboard clears the error");
    let mut paused = board();
    paused.refresh_paused = true;
    state.deliver_dashboard(paused, HashMap::new());
    assert_eq!(state.name_draft(), "Ops", "setNameDraft(current.name)");
    assert!(state.monitor_paused, "setMonitorPaused(!!current.refreshPaused)");
    assert!(!state.loading && state.current.is_some());
    // The catch: the error lands, the stale board survives.
    state.deliver_dashboard_error("boom".to_owned());
    assert!(state.current.is_some());
    assert_eq!(state.error.as_deref(), Some("boom"));
}

#[test]
fn on_open_refreshes_due_lists_enabled_onopen_widgets_without_runs() {
    let mut state = DashboardPanelState::mount(Some("d1"), None);
    assert!(state.on_open_refreshes_due().is_empty(), "no board, no refreshes");
    let mut current = board();
    let mut disabled = widget("w4", "Disabled", "wf-4", RefreshModeVm::OnOpen, 9, 4);
    disabled.enabled = false;
    current.widgets = vec![
        widget("w1", "Revenue", "wf-1", RefreshModeVm::Manual, 0, 4),
        widget("w2", "Latency", "wf-2", RefreshModeVm::OnOpen, 2, 3),
        widget("w3", "Alerts", "wf-3", RefreshModeVm::OnOpen, 5, 4),
        disabled,
    ];
    state.deliver_dashboard(current, HashMap::new());
    assert_eq!(
        state.on_open_refreshes_due(),
        vec!["w2".to_owned(), "w3".to_owned()],
        "missing run entries refresh, disabled and manual never do"
    );
    // A landed run satisfies w2; a loaded-and-empty run (React's null) does not.
    state.set_run("w2", Some(ok_run()));
    state.set_run("w3", None);
    assert_eq!(state.on_open_refreshes_due(), vec!["w3".to_owned()]);
}

#[test]
fn title_sync_due_fires_once_per_board_stamp() {
    let mut state = DashboardPanelState::mount(Some("d1"), None);
    state.deliver_dashboard(board(), HashMap::new());
    assert_eq!(state.title_sync_due(), Some(("d1".to_owned(), "Ops".to_owned())));
    state.mark_title_synced();
    assert_eq!(state.title_sync_due(), None, "the id:name stamp matches");
    // A rename landing as a new current re-fires (the effect's dep).
    let mut renamed = board();
    renamed.name = "Ops2".to_owned();
    state.deliver_dashboard(renamed, HashMap::new());
    assert_eq!(state.title_sync_due(), Some(("d1".to_owned(), "Ops2".to_owned())));
}

#[test]
fn panel_deleted_clears_or_repoints_the_active_board() {
    let mut state = DashboardPanelState::mount(Some("d1"), None);
    state.deliver_list(board_list());
    assert_eq!(state.panel_deleted(Vec::new()), None);
    assert!(state.active_dashboard_id.is_empty() && state.list.is_empty());
    state.deliver_list(board_list());
    assert_eq!(
        state.panel_deleted(vec![BoardListItemVm {
            id: "d2".to_owned(),
            name: "Drift".to_owned()
        }]),
        Some(("d2".to_owned(), "Drift".to_owned())),
        "remaining[0] activates and notifies"
    );
    assert_eq!(state.active_dashboard_id, "d2");
}

#[test]
fn board_patches_build_save_payloads_without_touching_current() {
    let mut state = DashboardPanelState::mount(Some("d1"), None);
    state.deliver_dashboard(board(), HashMap::new());
    let patched = state.board_with_view_mode("w1", TileViewMode::Table).unwrap();
    assert_eq!(patched.widgets[0].view_mode, TileViewMode::Table);
    assert_eq!(patched.widgets[1].view_mode, TileViewMode::Chart);
    // The local current waits for the host's save (React's store patch).
    assert_eq!(
        state.current.as_ref().unwrap().widgets[0].view_mode,
        TileViewMode::Chart
    );

    let patched = state.board_with_chart_type("w2", ChartType::Line).unwrap();
    assert_eq!(
        patched.widgets[1].chart_config.as_ref().unwrap().chart_type,
        ChartType::Line
    );
    let config = ChartConfig { chart_type: ChartType::Pie, ..chart_config() };
    let patched = state.board_with_chart_config("w1", config.clone()).unwrap();
    assert_eq!(patched.widgets[0].chart_config, Some(config));
    // React maps over the list: an unknown id still saves the board.
    assert!(state.board_with_view_mode("nope", TileViewMode::Table).is_some());
}

#[test]
fn the_snapshot_round_trips_the_recreatable_view_state() {
    let mut state = DashboardPanelState::mount(Some("d1"), None);
    state.deliver_list(board_list());
    state.deliver_dashboard(board(), HashMap::new());
    assert!(state.begin_add_widget("w-new"));
    state.open_history(&state.current.as_ref().unwrap().widgets[0].clone());
    state.begin_rename();
    state.rename_edit = TextEdit::new("Draft".to_owned());
    let snapshot = state.snapshot();
    let restored = DashboardPanelState::mount(None, Some(&snapshot));
    assert_eq!(restored.active_dashboard_id, "d1");
    assert!(restored.editor_open && restored.is_new_widget);
    assert_eq!(
        restored.editing_widget.as_ref().map(|w| w.id.clone()),
        Some("w-new".to_owned())
    );
    assert_eq!(restored.editor_hidden_sql, state.editor_hidden_sql);
    assert!(restored.history_open);
    assert_eq!(
        restored.history_widget.as_ref().map(|w| w.id.clone()),
        Some("w1".to_owned())
    );
    assert!(restored.renaming);
    assert_eq!(restored.name_draft(), "Draft");
    assert!(!restored.bootstrapping, "the snapshot's board survives the remount");
}

#[test]
fn editor_saved_and_close_split_their_clearing_rules() {
    let mut state = DashboardPanelState::mount(Some("d1"), None);
    state.deliver_dashboard(board(), HashMap::new());
    assert!(state.begin_add_widget("w-new"));
    assert!(state.editor_saved(), "handleSaveWidget's isNewWidget tail");
    assert!(!state.editor_open && state.editing_widget.is_none() && !state.is_new_widget);
    assert!(
        state.editor_hidden_sql.is_some(),
        "the hidden sql survives the save; only onClose clears it"
    );
    state.close_editor();
    assert!(state.editor_hidden_sql.is_none() && state.user_workflows.is_empty());
}

#[test]
fn sanitize_export_stem_collapses_bad_runs_like_react() {
    assert_eq!(sanitize_export_stem("Ops Board!"), "Ops_Board_");
    assert_eq!(sanitize_export_stem("中文看板"), "_", "one maximal bad run");
    assert_eq!(sanitize_export_stem("a..b--c"), "a..b--c");
    assert_eq!(sanitize_export_stem(""), "dashboard");
    assert_eq!(sanitize_export_stem("   "), "_");
}

#[test]
fn next_widget_layout_and_the_empty_board_match_react() {
    let widgets = vec![
        widget("w1", "Revenue", "wf-1", RefreshModeVm::Manual, 0, 4),
        widget("w2", "Latency", "wf-2", RefreshModeVm::Interval, 2, 3),
    ];
    assert_eq!(
        next_widget_layout(&widgets),
        WidgetLayoutVm { x: 0, y: 5, w: 6, h: 4 },
        "below the deepest bottom edge (2 + 3)"
    );
    assert_eq!(
        next_widget_layout(&[]),
        WidgetLayoutVm { x: 0, y: 0, w: 6, h: 4 }
    );
    let board = create_empty_dashboard("id-1", "New", "2026-01-01T00:00:00Z");
    assert_eq!((board.layout_cols, board.layout_row_height), (12, 80));
    assert!(board.widgets.is_empty() && board.enabled && !board.refresh_paused);
    assert_eq!((board.id.as_str(), board.name.as_str()), ("id-1", "New"));
}

#[test]
fn the_monitor_toggle_glyph_swaps_with_the_pause_flag() {
    assert!(std::ptr::eq(monitor_toggle_glyph(false), &ICON_PAUSE), "running shows Pause");
    assert!(std::ptr::eq(monitor_toggle_glyph(true), &ICON_PLAY), "paused shows Play");
}

// ---- The early-return branches -------------------------------------------------

#[test]
fn the_bootstrap_branch_is_the_centered_spinner_line() {
    let cold_rt = rt(AuditState::cold());
    assert!(has(&cold_rt, "dashboard-panel"));
    assert!(has(&cold_rt, "dashboard-bootstrapping"));
    assert!(has(&cold_rt, "dashboard-bootstrapping-spinner"));
    assert_eq!(
        element_text(&cold_rt, "dashboard-bootstrapping-text").as_deref(),
        Some(t("common.loading"))
    );
    assert!(
        !has(&cold_rt, "dashboard-tab-bar")
            && !has(&cold_rt, "dashboard-panel-toolbar")
            && !has(&cold_rt, "dashboard-grid"),
        "no chrome before the branches resolve"
    );

    // The `||` arm: a list fetch with no board and no list shares the view.
    let mut scene = AuditState::shell(DashboardPanelState::mount(Some("d1"), None));
    scene.panel.active_dashboard_id = String::new();
    scene.panel.bootstrapping = false;
    scene.panel.begin_list_fetch();
    let list_loading_rt = rt(scene);
    assert!(has(&list_loading_rt, "dashboard-bootstrapping"), "listLoading with no board bootstraps");
}

#[test]
fn the_empty_boards_hero_offers_create_first() {
    let hero_rt = rt(AuditState::empty_boards());
    assert!(has(&hero_rt, "dashboard-empty-boards"));
    assert!(has(&hero_rt, "dashboard-empty-boards-icon"));
    assert_eq!(
        element_text(&hero_rt, "dashboard-empty-boards-text").as_deref(),
        Some(t("dashboard.emptyBoards"))
    );
    assert!(has(&hero_rt, "dashboard-create-first") && has(&hero_rt, "dashboard-create-first-icon"));
    assert_eq!(
        element_text(&hero_rt, "dashboard-create-first-text").as_deref(),
        Some(t("dashboard.createFirstBoard"))
    );
    assert!(!has(&hero_rt, "dashboard-panel-toolbar"));

    let mut press_rt = rt(AuditState::empty_boards());
    press(&mut press_rt, "dashboard-create-first");
    assert_eq!(press_rt.state.create_firsts, 1, "handleCreateFirstBoard");
}

#[test]
fn the_missing_id_line_when_boards_exist_but_none_is_active() {
    let mut scene = AuditState::loaded();
    scene.panel.active_dashboard_id = String::new();
    scene.panel.current = None;
    let rt = rt(scene);
    assert!(has(&rt, "dashboard-panel-missing-id"));
    assert_eq!(
        element_text(&rt, "dashboard-panel-missing-id-text").as_deref(),
        Some(t("dashboard.missingId"))
    );
    assert!(!has(&rt, "dashboard-tab-bar"), "the early return precedes the strip");
    assert!(!has(&rt, "dashboard-grid"));
}

// ---- The mount tree (the firstFrame suite's journey) ---------------------------

#[test]
fn the_hot_store_mounts_the_tab_bar_toolbar_and_grid() {
    let rt = rt(AuditState::loaded());
    // The tab strip and its actions.
    for key in [
        "dashboard-tab-bar-row",
        "dashboard-tab-bar",
        "dashboard-tab-bar-actions",
        "dashboard-tab-add",
        "dashboard-import",
        "dashboard-docs-help",
    ] {
        assert!(has(&rt, key), "{key}");
    }
    assert_eq!(element_text(&rt, "dashboard-tab-d1-name").as_deref(), Some("Ops"));
    assert!(has(&rt, "dashboard-tab-d1-content") && has(&rt, "dashboard-tab-d2-content"));
    // The underline is always present, transparent while inactive (T174).
    assert!(
        maybe(&rt, "dashboard-tab-d1-underline").is_some_and(|e| e.fill.a > 0.99),
        "the active tab's accent bar"
    );
    assert!(
        maybe(&rt, "dashboard-tab-d2-underline").is_some_and(|e| e.fill.a < 0.01),
        "the inactive tab's bar is transparent"
    );
    // The toolbar.
    for key in [
        "dashboard-panel-toolbar",
        "dashboard-panel-toolbar-title",
        "dashboard-panel-toolbar-actions",
        "dashboard-rename",
        "dashboard-rename-icon",
        "dashboard-pause-toggle",
        "dashboard-refresh-all",
        "dashboard-export",
        "dashboard-delete-panel",
    ] {
        assert!(has(&rt, key), "{key}");
    }
    assert_eq!(element_text(&rt, "dashboard-rename-name").as_deref(), Some("Ops"));
    assert_eq!(
        maybe(&rt, "dashboard-pause-toggle").and_then(|e| e.label.clone()).as_deref(),
        Some(t("dashboard.pauseMonitoring")),
        "the React title, live while running"
    );
    // The main column and the widget grid.
    assert!(has(&rt, "dashboard-main"));
    assert!(has(&rt, "dashboard-grid"));
    assert!(has(&rt, "dashboard-grid-cell-w1") && has(&rt, "dashboard-grid-cell-w2"));
    // One composed T069 tile per widget, with the tile's own key family.
    assert!(has(&rt, &tile_root_key("w1")));
    assert!(has(&rt, &tile_root_key("w2")));
    assert_eq!(element_text(&rt, "dashboard-tile-title").as_deref(), Some("Revenue"));
    assert!(
        has(&rt, "dashboard-view-toggle")
            && has(&rt, "dashboard-tile-chart")
            && has(&rt, "chart-canvas-container"),
        "the seam-fed chart body"
    );
    // The add-widget card.
    assert!(has(&rt, "dashboard-add-widget") && has(&rt, "dashboard-add-widget-icon"));
    assert_eq!(
        element_text(&rt, "dashboard-add-widget-text").as_deref(),
        Some(t("dashboard.addWidget"))
    );
    // Nothing rides the layer while everything is closed.
    assert!(
        !has(&rt, "widget-editor-drawer")
            && !has(&rt, "run-history-drawer")
            && !has(&rt, "dashboard-panel-delete-dialog")
            && !has(&rt, "add-to-dashboard-dialog")
    );
}

#[test]
fn loading_and_error_paint_inside_the_main_column() {
    // The loading line: loading with no board yet.
    let mut scene = AuditState::loaded();
    scene.panel.current = None;
    scene.panel.loading = true;
    let loading_rt = rt(scene);
    assert!(has(&loading_rt, "dashboard-main-loading") && has(&loading_rt, "dashboard-main-loading-spinner"));
    assert_eq!(
        element_text(&loading_rt, "dashboard-main-loading-text").as_deref(),
        Some(t("common.loading"))
    );
    assert!(!has(&loading_rt, "dashboard-grid"), "no grid without a board");

    // The error banner rides above the grid (React keeps the stale board).
    let mut scene = AuditState::loaded();
    scene.panel.error = Some("boom".to_owned());
    let error_rt = rt(scene);
    assert!(has(&error_rt, "dashboard-main-error"));
    assert_eq!(element_text(&error_rt, "dashboard-main-error-text").as_deref(), Some("boom"));
    assert!(has(&error_rt, "dashboard-grid"), "the stale board stays");
}

// ---- Grid geometry ---------------------------------------------------------------

#[test]
fn the_grid_is_a_wrapping_row_of_420pt_cells() {
    let rt = rt(AuditState::loaded());
    let [x1, y1, cw1, h1] = maybe(&rt, "dashboard-grid-cell-w1").expect("w1 laid out").rect;
    let [x2, y2, cw2, _] = maybe(&rt, "dashboard-grid-cell-w2").expect("w2 laid out").rect;
    let [xa, ya, cwa, cha] = maybe(&rt, "dashboard-add-widget").expect("add card laid out").rect;
    assert!((x1 - 16.0).abs() < 0.5, "the main column's p-4, got {x1}");
    assert!(
        (cw1 - 420.0).abs() < 0.5 && (cw2 - 420.0).abs() < 0.5,
        "the minmax(420px, 1fr) floor, got {cw1}/{cw2}"
    );
    assert!((x2 - (x1 + 420.0 + 12.0)).abs() < 0.5, "gap-3, got {}", x2 - x1 - 420.0);
    assert!((y1 - y2).abs() < 0.5, "two cells share the row");
    assert!(ya > y1 + 300.0, "the third card wraps below, got {ya} vs {y1}");
    assert!((xa - 16.0).abs() < 0.5, "the wrap starts at the row head");
    assert!(cha >= 319.5, "the add card's min-h-[320px], got {cha}");
    assert!((h1 - 320.0).abs() < 0.5, "the tile's h-80, got {h1}");
    assert!(cwa > 0.0);
}

// ---- Toolbar and tab actions ------------------------------------------------------

#[test]
fn the_tab_and_toolbar_actions_fire_the_host_slots() {
    let mut rt = rt(AuditState::loaded());
    press(&mut rt, "dashboard-tab-add");
    press(&mut rt, "dashboard-import");
    press(&mut rt, "dashboard-docs-help");
    press(&mut rt, "dashboard-export");
    assert_eq!(rt.state.create_panels, 1);
    assert_eq!(rt.state.imports, 1);
    assert_eq!(rt.state.docs_opens, 1);
    assert_eq!(rt.state.exports, 1);

    // Refresh-all swaps the glyph for the spinner until the host ends it.
    press(&mut rt, "dashboard-refresh-all");
    assert_eq!(rt.state.refresh_alls, 1);
    assert!(rt.state.panel.refreshing_all, "the state half ran first");
    settle(&mut rt);
    assert!(has(&rt, "dashboard-refresh-all-spinner"), "React swaps the glyph");
    rt.update(|s, _| s.panel.end_refresh_all());
    settle(&mut rt);
    assert!(!has(&rt, "dashboard-refresh-all-spinner"));
    assert!(has(&rt, "dashboard-refresh-all"), "the button returns");
}

#[test]
fn a_loading_board_disables_the_import_export_and_add_widget_controls() {
    let mut scene = AuditState::loaded();
    scene.panel.loading = true;
    let mut rt = rt(scene);
    press(&mut rt, "dashboard-import");
    press(&mut rt, "dashboard-export");
    press(&mut rt, "dashboard-add-widget");
    assert_eq!(rt.state.imports, 0);
    assert_eq!(rt.state.exports, 0);
    assert_eq!(rt.state.add_widgets, 0, "React's loading-disabled control is inert");
}

#[test]
fn the_pause_toggle_flips_the_title_the_glyph_and_the_host_state() {
    let mut rt = rt(AuditState::loaded());
    press(&mut rt, "dashboard-pause-toggle");
    assert_eq!(rt.state.pause_toggles, vec![true], "the computed next = !monitorPaused");
    assert!(rt.state.panel.monitor_paused, "the host applied setMonitorPaused(next)");
    settle(&mut rt);
    assert_eq!(
        maybe(&rt, "dashboard-pause-toggle").and_then(|e| e.label.clone()).as_deref(),
        Some(t("dashboard.resumeMonitoring")),
        "the title swaps with the flag"
    );
    press(&mut rt, "dashboard-pause-toggle");
    assert_eq!(rt.state.pause_toggles, vec![true, false]);
}

#[test]
fn switching_boards_moves_the_active_tab_and_notifies_the_host() {
    let mut rt = rt(AuditState::loaded());
    press(&mut rt, "dashboard-tab-d2");
    assert_eq!(rt.state.panel.active_dashboard_id, "d2");
    assert_eq!(rt.state.dashboard_changes, vec![("d2".to_owned(), "Drift".to_owned())]);
    settle(&mut rt);
    assert!(
        maybe(&rt, "dashboard-tab-d2-underline").is_some_and(|e| e.fill.a > 0.99),
        "the accent moved"
    );
    assert!(
        maybe(&rt, "dashboard-tab-d1-underline").is_some_and(|e| e.fill.a < 0.01),
        "the old tab's bar went transparent"
    );
}

#[test]
fn the_rename_field_commits_on_enter_and_cancels_on_escape() {
    let mut rt = rt(AuditState::loaded());
    press(&mut rt, "dashboard-rename");
    settle(&mut rt);
    assert!(has(&rt, "dashboard-rename-input"), "the field replaces the trigger");
    assert!(!has(&rt, "dashboard-rename"), "the trigger is gone while renaming");
    type_into(&mut rt, "dashboard-rename-input", "Renamed");
    assert_eq!(rt.state.panel.rename_edit.text, "Renamed");
    rt.key(KeyEvent { key: Key::Enter, mods: Mods::default() });
    settle(&mut rt);
    assert_eq!(rt.state.rename_commits, 1);
    assert_eq!(rt.state.renamed_to.as_deref(), Some("Renamed"), "the trimmed draft");
    assert!(!rt.state.panel.renaming);
    assert!(has(&rt, "dashboard-rename"), "the trigger returns");
    // The local board keeps the old name until the host's save lands.
    assert_eq!(element_text(&rt, "dashboard-rename-name").as_deref(), Some("Ops"));

    // Escape cancels without committing.
    press(&mut rt, "dashboard-rename");
    settle(&mut rt);
    type_into(&mut rt, "dashboard-rename-input", "Discard");
    rt.key(KeyEvent { key: Key::Escape, mods: Mods::default() });
    settle(&mut rt);
    assert!(!rt.state.panel.renaming);
    assert_eq!(rt.state.rename_commits, 1, "Escape only cancels");
}

// ---- The delete confirmations ------------------------------------------------------

#[test]
fn the_panel_delete_dialog_confirms_through_the_host() {
    let mut rt = rt(AuditState::loaded());
    assert!(!has(&rt, "dashboard-panel-delete-dialog"));
    press(&mut rt, "dashboard-delete-panel");
    settle(&mut rt);
    assert!(has(&rt, "dashboard-panel-delete-dialog"));
    assert_eq!(
        element_text(&rt, "dashboard-panel-delete-dialog-text").as_deref(),
        Some(t("dashboard.deletePanelConfirm"))
    );
    assert!(has(&rt, "dashboard-panel-delete-dialog-cancel"));
    assert!(has(&rt, "dashboard-delete-confirm"));

    // Cancel first: nothing leaves the panel.
    press(&mut rt, "dashboard-panel-delete-dialog-cancel");
    settle(&mut rt);
    assert!(!has(&rt, "dashboard-panel-delete-dialog"));
    assert_eq!(rt.state.deleted_panels, 0);

    // Confirm: the host deletes and the remaining board activates.
    press(&mut rt, "dashboard-delete-panel");
    settle(&mut rt);
    press(&mut rt, "dashboard-delete-confirm");
    assert_eq!(rt.state.deleted_panels, 1);
    assert!(!rt.state.panel.delete_confirm_open);
    assert_eq!(rt.state.panel.active_dashboard_id, "d2", "remaining[0]");
    assert_eq!(
        rt.state.dashboard_changes.last(),
        Some(&("d2".to_owned(), "Drift".to_owned())),
        "switchDashboard notifies"
    );
    settle(&mut rt);
    assert!(!has(&rt, "dashboard-panel-delete-dialog"));
}

#[test]
fn the_widget_delete_dialog_takes_the_widget_id() {
    let mut rt = rt(AuditState::loaded());
    press(&mut rt, "dashboard-tile-delete"); // w1's tile delete
    settle(&mut rt);
    assert!(has(&rt, "dashboard-panel-widget-delete-dialog"));
    assert_eq!(
        element_text(&rt, "dashboard-panel-widget-delete-dialog-text").as_deref(),
        Some(t("dashboard.deleteWidgetConfirm"))
    );
    assert!(
        has(&rt, "widget-delete-confirm"),
        "React's literal testid carries no dashboard- prefix"
    );
    press(&mut rt, "widget-delete-confirm");
    assert_eq!(rt.state.deleted_widgets, vec!["w1".to_owned()]);
    assert_eq!(rt.state.saved_boards.len(), 1);
    assert_eq!(rt.state.saved_boards[0].widgets.len(), 1, "the filtered board is the payload");
    assert_eq!(rt.state.saved_boards[0].widgets[0].id, "w2");
    assert!(rt.state.panel.widget_to_delete.is_none());
    settle(&mut rt);
    assert!(!has(&rt, "dashboard-panel-widget-delete-dialog"));
}

// ---- The editor seam and its composed children (T018 / T146) -----------------------

#[test]
fn the_add_widget_flow_opens_the_editor_seam_and_the_t018_dialog() {
    let mut rt = rt(AuditState::loaded());
    press(&mut rt, "dashboard-add-widget");
    assert_eq!(rt.state.add_widgets, 1);
    let panel = &rt.state.panel;
    assert!(panel.editor_open && panel.is_new_widget, "the as-new arm opens immediately");
    assert_eq!(
        panel.editing_widget.as_ref().map(|w| w.title.clone()),
        Some(t("dashboard.newWidget").to_owned())
    );
    assert_eq!(
        panel.editor_hidden_sql,
        Some(HiddenSqlVm {
            connection_id: String::new(),
            sql: "SELECT 1 AS v".to_owned()
        })
    );
    let draft = panel.editing_widget.as_ref().unwrap();
    assert_eq!(
        draft.layout,
        WidgetLayoutVm { x: 0, y: 5, w: 6, h: 4 },
        "nextWidgetLayout below the deepest widget"
    );
    assert_eq!(
        draft.chart_config.as_ref().map(|c| c.y_axes.clone()),
        Some(vec!["v".to_owned()])
    );

    settle(&mut rt);
    assert!(has(&rt, "widget-editor-drawer"), "the T201 seam mounts while editor_open");
    assert!(
        !has(&rt, "monitor-schedule-authorization"),
        "React's !isNew gate keeps the T146 card out"
    );
    // The T018 dialog this journey opens: loading first, the picker once
    // its list lands.
    assert!(has(&rt, "add-to-dashboard-loading"));
    rt.update(|s, _| {
        let token = s.add_dialog.request_epoch;
        assert!(s.add_dialog.deliver_list(
            vec![
                DashboardOptionVm { id: "d1".to_owned(), name: "Ops".to_owned() },
                DashboardOptionVm { id: "d2".to_owned(), name: "Drift".to_owned() },
            ],
            token
        ));
    });
    settle(&mut rt);
    assert!(has(&rt, "add-to-dashboard-dialog"));
    assert!(has(&rt, "dashboard-target-select-trigger"));
    assert!(has(&rt, "add-to-dashboard-confirm"));
    assert!(!has(&rt, "add-to-dashboard-loading"));

    // The dialog closes independently of the editor seam.
    press(&mut rt, "add-to-dashboard-cancel");
    settle(&mut rt);
    assert!(!has(&rt, "add-to-dashboard-dialog"));
    assert!(has(&rt, "widget-editor-drawer"));

    // The seam's onClose wiring: close_editor unmounts the drawer.
    rt.update(|s, _| s.panel.close_editor());
    settle(&mut rt);
    assert!(!has(&rt, "widget-editor-drawer"));
    assert!(!rt.state.panel.editor_open && rt.state.panel.editing_widget.is_none());
}

#[test]
fn editing_an_interval_widget_mounts_the_t146_card_in_the_seam() {
    let mut rt = rt(AuditState::loaded());
    // w1 (manual refresh, workflow wf-1) through the tile's real edit press.
    press(&mut rt, "dashboard-tile-edit");
    assert_eq!(
        rt.state.panel.editor_fetch_due(),
        Some("wf-1"),
        "openWidgetEditor awaits workflowGet"
    );
    assert!(!rt.state.panel.editor_open, "the drawer opens only after the await");
    settle(&mut rt);
    assert!(!has(&rt, "widget-editor-drawer"));
    // A visible workflow lands the list, not the hidden sql.
    rt.update(|s, _| {
        s.panel.deliver_editor_workflow(&EditorWorkflowVm {
            visibility: "private".to_owned(),
            connection: None,
            steps: Vec::new(),
            user_workflows: vec![WorkflowListItemVm {
                id: "wf-9".to_owned(),
                name: "Nine".to_owned(),
            }],
        });
    });
    assert!(rt.state.panel.editor_open);
    assert_eq!(rt.state.panel.user_workflows.len(), 1);
    settle(&mut rt);
    assert!(has(&rt, "widget-editor-drawer"));
    assert!(!has(&rt, "monitor-schedule-authorization"), "w1's refresh mode is manual");

    // w2 (interval refresh): the T201 host's condition mounts the T146 card.
    rt.update(|s, _| {
        let w2 = s.panel.current.as_ref().unwrap().widgets[1].clone();
        s.panel.begin_edit_widget(w2, false);
        s.panel.deliver_editor_workflow(&EditorWorkflowVm {
            visibility: "private".to_owned(),
            connection: None,
            steps: Vec::new(),
            user_workflows: Vec::new(),
        });
    });
    settle(&mut rt);
    for key in [
        "monitor-schedule-authorization",
        "monitor-schedule-title",
        "monitor-schedule-hint",
        "monitor-schedule-status",
        "monitor-schedule-actions",
        "monitor-schedule-authorize",
    ] {
        assert!(has(&rt, key), "{key}");
    }
    assert_eq!(
        element_text(&rt, "monitor-schedule-status").as_deref(),
        Some(t("dashboard.scheduleUnauthorized")),
        "no grant delivered yet"
    );
}

#[test]
fn a_dashboardhidden_workflow_lands_the_hidden_sql_with_reacts_chain() {
    let mut state = DashboardPanelState::mount(Some("d1"), None);
    state.deliver_dashboard(board(), HashMap::new());
    let w1 = state.current.as_ref().unwrap().widgets[0].clone();
    assert_eq!(
        state.begin_edit_widget(w1, false),
        EditorOpenFlow::FetchWorkflow("wf-1".to_owned())
    );

    // wf.connection wins; the query step's sql.
    state.deliver_editor_workflow(&EditorWorkflowVm {
        visibility: "dashboardHidden".to_owned(),
        connection: Some("conn-9".to_owned()),
        steps: vec![EditorWorkflowStepVm {
            kind: "query".to_owned(),
            connection: None,
            sql: Some("SELECT 2 AS v".to_owned()),
        }],
        user_workflows: Vec::new(),
    });
    assert_eq!(
        state.editor_hidden_sql,
        Some(HiddenSqlVm {
            connection_id: "conn-9".to_owned(),
            sql: "SELECT 2 AS v".to_owned()
        })
    );
    assert!(state.editor_open, "setEditorOpen(true) after the await");
    assert!(state.user_workflows.is_empty(), "the hidden branch skips workflowList");

    // The fallbacks: queryStep.connection, then '' and SELECT 1.
    let w1 = state.current.as_ref().unwrap().widgets[0].clone();
    state.begin_edit_widget(w1, false);
    state.deliver_editor_workflow(&EditorWorkflowVm {
        visibility: "dashboardHidden".to_owned(),
        connection: None,
        steps: vec![EditorWorkflowStepVm {
            kind: "query".to_owned(),
            connection: Some("q-conn".to_owned()),
            sql: None,
        }],
        user_workflows: Vec::new(),
    });
    assert_eq!(
        state.editor_hidden_sql,
        Some(HiddenSqlVm {
            connection_id: "q-conn".to_owned(),
            sql: "SELECT 1 AS v".to_owned()
        })
    );

    // The catch arm still opens the drawer, and a late delivery re-opens it
    // (React has no cancel token in openWidgetEditor).
    let w1 = state.current.as_ref().unwrap().widgets[0].clone();
    state.begin_edit_widget(w1, false);
    state.deliver_editor_workflow_error();
    assert!(state.editor_open);
    state.close_editor();
    assert!(!state.editor_open);
    state.deliver_editor_workflow(&EditorWorkflowVm {
        visibility: "private".to_owned(),
        connection: None,
        steps: Vec::new(),
        user_workflows: vec![WorkflowListItemVm { id: "wf-9".to_owned(), name: "Nine".to_owned() }],
    });
    assert!(state.editor_open, "the late delivery re-opens, as React's does");
}

// ---- The run-history drawer mount (T175) ------------------------------------------

#[test]
fn the_history_press_mounts_the_t175_drawer_and_serves_its_fetches() {
    let mut rt = rt(AuditState::loaded());
    assert!(!has(&rt, "run-history-drawer"));
    press(&mut rt, "dashboard-tile-history");
    assert!(rt.state.panel.history_open);
    assert_eq!(
        rt.state.panel.history_widget.as_ref().map(|w| w.id.clone()),
        Some("w1".to_owned())
    );
    {
        let drawer = &rt.state.panel.history_drawer;
        assert!(drawer.loading_index, "open_history runs begin_resync");
        assert_eq!((drawer.dashboard_id.as_str(), drawer.widget_id.as_str()), ("d1", "w1"));
    }
    settle(&mut rt);
    for key in [
        "run-history-drawer",
        "run-history-drawer-scrim",
        "run-history-drawer-panel",
        "run-history-drawer-header",
        "run-history-drawer-title",
        "run-history-drawer-widget-title",
        "run-history-drawer-close",
        "run-history-drawer-list-loading",
    ] {
        assert!(has(&rt, key), "{key}");
    }
    assert_eq!(
        element_text(&rt, "run-history-drawer-widget-title").as_deref(),
        Some("Revenue")
    );

    // The index lands, auto-selects, and the run loads through the seam.
    rt.update(|s, _| {
        let token = s.panel.history_drawer.index_epoch;
        assert!(s.panel.history_drawer.deliver_index(vec![history_entry("run-1")], token));
    });
    assert_eq!(
        rt.state.panel.history_drawer.run_fetch_due(),
        Some("run-1"),
        "the auto-selection arms the run fetch"
    );
    rt.update(|s, _| {
        let token = s.panel.history_drawer.run_epoch;
        assert!(s.panel.history_drawer.deliver_run(ok_run(), token));
    });
    settle(&mut rt);
    assert!(has(&rt, "run-history-drawer-entry-run-1"));
    assert_eq!(
        element_text(&rt, "run-history-drawer-entry-run-1-status").as_deref(),
        Some("Ok")
    );
    assert!(has(&rt, "run-history-view-toggle"));
    assert!(has(&rt, "run-history-chart"), "the seam-fed chart body");

    // The header button closes through the panel's close_history.
    press(&mut rt, "run-history-drawer-close");
    assert!(!rt.state.panel.history_open && rt.state.panel.history_widget.is_none());
    settle(&mut rt);
    assert!(!has(&rt, "run-history-drawer"));
}

#[test]
fn stale_history_deliveries_land_nothing_after_a_resync_or_close() {
    let mut state = DashboardPanelState::mount(Some("d1"), None);
    state.deliver_dashboard(board(), HashMap::new());
    let w1 = state.current.as_ref().unwrap().widgets[0].clone();
    state.open_history(&w1);
    let stale = state.history_drawer.index_epoch;

    // A re-open bumps both epochs: the old tokens are dead.
    state.open_history(&w1);
    assert!(!state.history_drawer.deliver_index(Vec::new(), stale), "the cancelled guard");
    assert!(state.history_drawer.loading_index, "the stale list landed nothing");
    assert!(
        !state.history_drawer.deliver_run(ok_run(), stale),
        "the run token died with the resync"
    );

    // A landed selection then a close: both epochs bump again.
    let token = state.history_drawer.index_epoch;
    assert!(state.history_drawer.deliver_index(vec![history_entry("run-1")], token));
    let run_token = state.history_drawer.run_epoch;
    state.close_history();
    assert!(
        !state.history_drawer.deliver_run(ok_run(), run_token),
        "close cancels the in-flight fetch"
    );
    assert!(!state.history_drawer.open);
    // Re-selecting the current entry never re-fires (React's dep array).
    state.open_history(&w1);
    assert!(state.history_drawer.select("run-1"));
    assert!(!state.history_drawer.select("run-1"), "the id did not change");
}

// ---- The composed tile slots ----------------------------------------------------

#[test]
fn the_tile_slots_route_through_the_panel_state() {
    let mut rt = rt(AuditState::loaded());
    // The refresh press is the panel's on_refresh_widget slot.
    press(&mut rt, "dashboard-tile-refresh");
    assert_eq!(rt.state.refresh_widgets, vec!["w1".to_owned()]);
    // The expand overlay rides the panel's layer through tile_state_mut.
    press(&mut rt, "dashboard-tile-expand");
    assert!(
        rt.state
            .panel
            .tile_states
            .get("w1")
            .is_some_and(|tile| tile.expanded)
    );
    settle(&mut rt);
    assert!(has(&rt, "dashboard-tile-overlay"));
    press(&mut rt, "dashboard-tile-overlay-collapse");
    settle(&mut rt);
    assert!(!has(&rt, "dashboard-tile-overlay"));
    // The view switch builds the board save payload (the local current
    // waits for the host's save).
    press(&mut rt, "widget-view-table");
    assert_eq!(rt.state.saved_boards.len(), 1);
    assert_eq!(rt.state.saved_boards[0].widgets[0].view_mode, TileViewMode::Table);
    assert_eq!(
        rt.state.panel.current.as_ref().unwrap().widgets[0].view_mode,
        TileViewMode::Chart,
        "the store patch lands on save"
    );
}
