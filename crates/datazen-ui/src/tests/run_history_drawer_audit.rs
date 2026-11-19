//! Independent audit for T175 (the RunHistoryDrawer Rune port).
//!
//! Tester-written: the coder's module is a black box and every assertion
//! below re-reads `packages/frontend/src/windows/dashboard/RunHistoryDrawer.tsx`
//! and its React test `__tests__/RunHistoryDrawer.test.tsx`: the null render
//! while closed, the index load and first-entry auto-selection, the epoch
//! guards on every delivery, the re-select-no-refetch rule, the shell
//! geometry (right-anchored `max-w-3xl` aside over the inert scrim), the
//! left column's three branches, the entry rows (time / status ink /
//! alert glyph), the view switch and the loaded run's chart / table /
//! error / timeout / empty states. Element evidence comes from
//! `Runtime::elements()` (runtime layout), never from a value the component
//! reports about itself; presses go through real pointer events.
//!
//! The chart transform (`widgetRunToChartData`, T030/T182) is a documented
//! seam: the audit feeds the drawer transformed points exactly as the host
//! will, and asserts the drawer's own gating on top of them.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::chart::chart_canvas::{
    renderer_note_key, ChartConfig, ChartDataPoint, ChartType, ChartValue as CanvasValue,
};
use crate::support::chart_field_inference::ChartValue;
use crate::ui::kit::kit;
use crate::ui::palette;
use crate::views::dashboard::chart_widget_tile::{
    ChartWidgetVm, TileViewMode, WidgetRunStatus, WidgetRunVm,
};
use crate::views::dashboard::run_history_drawer::{
    RunHistoryDrawer, RunHistoryDrawerState, RunIndexEntryVm, HISTORY_LIMIT,
};

// ---- Fixtures ---------------------------------------------------------------

fn widget() -> ChartWidgetVm {
    ChartWidgetVm {
        id: "w1".to_owned(),
        title: "Sales".to_owned(),
        view_mode: TileViewMode::Chart,
        chart_config: Some(ChartConfig {
            chart_type: ChartType::Bar,
            x_axis: Some("category".to_owned()),
            y_axes: vec!["amount".to_owned()],
            ..Default::default()
        }),
        enabled: true,
    }
}

fn entry(
    id: &str,
    started_at: &str,
    status: WidgetRunStatus,
    alert_fired: bool,
) -> RunIndexEntryVm {
    RunIndexEntryVm {
        id: id.to_owned(),
        started_at: started_at.to_owned(),
        status,
        alert_fired,
    }
}

fn entries() -> Vec<RunIndexEntryVm> {
    vec![
        entry("run-1", "2026-01-01T00:00:00Z", WidgetRunStatus::Ok, false),
        entry("run-2", "2026-01-02T00:00:00Z", WidgetRunStatus::Error, true),
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

/// The transform seam's delivery: two points shaped like the run above.
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

#[derive(Clone)]
struct AuditState {
    drawer: RunHistoryDrawerState,
    widget: ChartWidgetVm,
    points: Vec<ChartDataPoint>,
    /// The seam gate: the host's memo is null until the run and the config
    /// are both there (and this audit chooses to deliver).
    supply_chart_data: bool,
    dark: bool,
    closes: usize,
    selections: Vec<String>,
    view_modes: Vec<TileViewMode>,
}

impl AuditState {
    /// Open, index landed, first entry auto-selected, its ok run loaded.
    fn loaded() -> Self {
        let mut drawer = RunHistoryDrawerState::default();
        drawer.begin_resync("dash-1", &widget());
        let token = drawer.index_epoch;
        drawer.deliver_index(entries(), token);
        let token = drawer.run_epoch;
        drawer.deliver_run(ok_run(), token);
        Self {
            drawer,
            widget: widget(),
            points: seam_points(),
            supply_chart_data: true,
            dark: true,
            closes: 0,
            selections: Vec::new(),
            view_modes: Vec::new(),
        }
    }

    /// Open with the index still in flight.
    fn loading_index() -> Self {
        let mut drawer = RunHistoryDrawerState::default();
        drawer.begin_resync("dash-1", &widget());
        Self {
            drawer,
            ..Self::loaded_rest()
        }
    }

    fn loaded_rest() -> Self {
        Self {
            drawer: RunHistoryDrawerState::default(),
            widget: widget(),
            points: seam_points(),
            supply_chart_data: true,
            dark: true,
            closes: 0,
            selections: Vec::new(),
            view_modes: Vec::new(),
        }
    }

    /// The host's memo gates: a loaded run plus a widget config.
    fn seam_slice(&self) -> Option<&[ChartDataPoint]> {
        if self.drawer.loaded_run.is_some()
            && self.widget.chart_config.is_some()
            && self.supply_chart_data
        {
            Some(self.points.as_slice())
        } else {
            None
        }
    }
}

fn view(s: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    RunHistoryDrawer::<AuditState>::new(&s.drawer, &s.widget)
        .chart_data(s.seam_slice())
        .on_close(|s: &mut AuditState| {
            s.closes += 1;
            s.drawer.close();
        })
        .on_select(|s: &mut AuditState, id: String| {
            s.selections.push(id.clone());
            s.drawer.select(&id);
        })
        .on_view_mode_change(|s: &mut AuditState, mode: TileViewMode| {
            s.view_modes.push(mode);
            s.drawer.set_view_mode(mode);
        })
        .build(s.dark, &mut layer);
    // The layer children ride the root so elements() sees them. The root
    // must be sized: the drawer's scrim/aside are absolute-positioned and
    // collapse (and get culled) inside an unsized column.
    let mut root = col().key("probe").w_full().h_full();
    for child in layer {
        root = root.child(child);
    }
    root
}

fn rt(scene: AuditState) -> Runtime<AuditState> {
    let mut rt = Runtime::new(
        scene,
        view,
        (960.0, 640.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    // The drawer rides a layer with an entrance animation: real frames let
    // it lay out (a single dt=0 frame leaves the layer's content unborn).
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
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

fn press(rt: &mut Runtime<AuditState>, key: &str) {
    let point = rt
        .find(key)
        .unwrap_or_else(|| panic!("{key} is laid out and pressable"));
    // Move onto the control first and let the clock run: a control mounted by
    // this runtime's first frame is pressable only after its enter motion.
    rt.pointer_move(point.0, point.1);
    rt.frame(1.0 / 60.0);
    rt.pointer_down(point.0, point.1);
    rt.pointer_up(point.0, point.1);
    for _ in 0..8 {
        rt.frame(1.0 / 60.0);
    }
}

fn settle(rt: &mut Runtime<AuditState>) {
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
}

// ---- the state machine (React effects 1 and 2) -------------------------------

#[test]
fn a_closed_drawer_needs_a_resync_and_builds_nothing() {
    let mut rt = rt(AuditState::loaded_rest());
    rt.update(|s, _| {
        assert!(s.drawer.needs_resync("dash-1", "w1"), "closed needs a resync");
    });
    let mut layer = Vec::new();
    RunHistoryDrawer::<AuditState>::new(&rt.state.drawer, &rt.state.widget)
        .build(rt.state.dark, &mut layer);
    assert!(layer.is_empty(), "React renders null while closed");
}

#[test]
fn begin_resync_resets_and_arms_the_index_fetch() {
    let mut state = RunHistoryDrawerState::default();
    state.begin_resync("dash-1", &widget());
    assert!(state.open);
    assert!(state.loading_index, "setLoadingIndex(true)");
    assert_eq!(state.selected_id, None);
    assert!(state.loaded_run.is_none());
    assert_eq!(state.index_error, None);
    assert_eq!(state.run_error, None);
    assert_eq!(state.view_mode, TileViewMode::Chart, "setViewMode(widget.viewMode)");
    assert!(!state.needs_resync("dash-1", "w1"), "the pair is served");
    assert!(state.needs_resync("dash-2", "w1"), "a dashboard change refetches");
    assert!(state.needs_resync("dash-1", "w2"), "a widget change refetches");

    // The view mode re-seeds from the widget on every open.
    let mut table_widget = widget();
    table_widget.view_mode = TileViewMode::Table;
    state.begin_resync("dash-1", &table_widget);
    assert_eq!(state.view_mode, TileViewMode::Table);
}

#[test]
fn deliver_index_lands_the_list_and_auto_selects_the_first_entry() {
    let mut state = RunHistoryDrawerState::default();
    state.begin_resync("dash-1", &widget());
    let token = state.index_epoch;
    assert!(state.deliver_index(entries(), token));
    assert_eq!(state.index.len(), 2);
    assert_eq!(state.selected_id.as_deref(), Some("run-1"), "entries[0].id");
    assert!(!state.loading_index, "the finally stops the spinner");
    // The auto-selection armed the run fetch, as React's effect cascade does.
    assert!(state.loading_run);
    assert_eq!(state.run_fetch_due(), Some("run-1"));
}

#[test]
fn stale_tokens_land_nothing() {
    let mut state = RunHistoryDrawerState::default();
    state.begin_resync("dash-1", &widget());
    let stale = state.index_epoch - 1;
    assert!(!state.deliver_index(entries(), stale));
    assert!(!state.deliver_index_error("boom".to_owned(), stale));
    assert!(state.loading_index, "the spinner is untouched");
    assert!(state.index.is_empty());
    assert!(!state.deliver_run(ok_run(), state.run_epoch - 1));
    assert!(!state.deliver_run_error("boom".to_owned(), state.run_epoch - 1));
}

#[test]
fn a_closed_drawer_drops_in_flight_deliveries() {
    let mut state = RunHistoryDrawerState::default();
    state.begin_resync("dash-1", &widget());
    let token = state.index_epoch;
    state.deliver_index(entries(), token);
    let run_token = state.run_epoch;
    state.close();
    assert!(!state.deliver_run(ok_run(), run_token), "the cleanup cancelled it");
    assert!(!state.deliver_index(entries(), token), "the index token died too");
    assert!(state.needs_resync("dash-1", "w1"), "reopen re-runs the effect");
}

#[test]
fn deliver_index_error_lands_the_text_and_stops_the_spinner() {
    let mut state = RunHistoryDrawerState::default();
    state.begin_resync("dash-1", &widget());
    let token = state.index_epoch;
    assert!(state.deliver_index_error("Error: index boom".to_owned(), token));
    assert_eq!(state.index_error.as_deref(), Some("Error: index boom"));
    assert!(!state.loading_index);
    assert!(state.index.is_empty(), "React never set the list");
}

#[test]
fn re_selecting_the_current_entry_does_not_refetch() {
    let mut state = RunHistoryDrawerState::default();
    state.begin_resync("dash-1", &widget());
    let token = state.index_epoch;
    state.deliver_index(entries(), token);
    let run_token = state.run_epoch;
    state.deliver_run(ok_run(), run_token);
    assert!(!state.loading_run);

    assert!(!state.select("run-1"), "React's dep array did not change");
    assert!(!state.loading_run, "no fetch was armed");
    assert_eq!(state.run_fetch_due(), None);

    assert!(state.select("run-2"));
    assert!(state.loading_run, "setLoadingRun(true)");
    assert_eq!(state.run_error, None, "setRunError(null)");
    assert_eq!(state.run_fetch_due(), Some("run-2"));
    assert!(
        state.loaded_run.is_some(),
        "the previous run stays loaded under the overlay"
    );
}

#[test]
fn deliver_run_error_clears_the_run_and_lands_the_text() {
    let mut state = RunHistoryDrawerState::default();
    state.begin_resync("dash-1", &widget());
    let token = state.index_epoch;
    state.deliver_index(entries(), token);
    let run_token = state.run_epoch;
    state.deliver_run(ok_run(), run_token);
    state.select("run-2");
    let run_token = state.run_epoch;
    assert!(state.deliver_run_error("Error: run load failed".to_owned(), run_token));
    assert!(state.loaded_run.is_none(), "setLoadedRun(null)");
    assert_eq!(state.run_error.as_deref(), Some("Error: run load failed"));
    assert!(!state.loading_run);
}

#[test]
fn history_limit_is_reacts_fifty() {
    assert_eq!(HISTORY_LIMIT, 50, "the listWidgetRuns page size");
}

// ---- the shell ---------------------------------------------------------------

#[test]
fn the_open_drawer_renders_the_shell() {
    let rt = rt(AuditState::loaded());
    assert!(has(&rt, "run-history-drawer"), "data-testid run-history-drawer");
    assert!(has(&rt, "run-history-drawer-scrim"));
    assert!(has(&rt, "run-history-drawer-panel"));
    assert!(has(&rt, "run-history-drawer-header"));
    assert_eq!(
        element_text(&rt, "run-history-drawer-title").as_deref(),
        Some(crate::i18n::t("dashboard.runHistory"))
    );
    assert_eq!(element_text(&rt, "run-history-drawer-widget-title").as_deref(), Some("Sales"));
    assert!(has(&rt, "run-history-drawer-close"), "the X button");
    assert!(has(&rt, "run-history-drawer-body"));
    assert!(has(&rt, "run-history-drawer-list"));
    assert!(has(&rt, "run-history-drawer-entry-run-1"));
    assert!(has(&rt, "run-history-drawer-entry-run-2"));
}

#[test]
fn the_panel_is_right_anchored_and_capped_at_max_w_3xl() {
    let rt = rt(AuditState::loaded());
    let panel = maybe(&rt, "run-history-drawer-panel").expect("the aside is laid out");
    let [x, y, w, h] = panel.rect;
    assert!((w - 768.0).abs() < 0.5, "React max-w-3xl, got {w}");
    assert!((x + w - 960.0).abs() < 0.5, "flex justify-end anchors the right edge, got {x}");
    assert!(y.abs() < 0.5, "inset-0 top, got {y}");
    assert!((y + h - 640.0).abs() < 0.5, "h-full, got {h}");
}

#[test]
fn the_scrim_is_the_forty_percent_black_and_inert() {
    let geom_rt = rt(AuditState::loaded());
    let scrim = maybe(&geom_rt, "run-history-drawer-scrim").expect("the scrim is laid out");
    assert_eq!(
        scrim.fill,
        rune::Color::BLACK.alpha(0.4),
        "React bg-black/40"
    );
    assert!(!scrim.focusable, "aria-hidden and pressless");
    // The scrim is not a press target: pressing it must not close (React's
    // has no onClick).
    let mut scrim_rt = rt(AuditState::loaded());
    press(&mut scrim_rt, "run-history-drawer-scrim");
    assert_eq!(scrim_rt.state.closes, 0, "only the header button closes");
}

// ---- the left column ---------------------------------------------------------

#[test]
fn the_index_loading_state_shows_the_spinner() {
    let rt = rt(AuditState::loading_index());
    assert!(has(&rt, "run-history-drawer-list-loading"));
    assert!(has(&rt, "run-history-drawer-list-loading-spinner"));
    assert!(!has(&rt, "run-history-drawer-list-empty"), "loading and empty are exclusive");
    assert!(!has(&rt, "run-history-drawer-entry-run-1"), "no entries while loading");
    assert!(!has(&rt, "run-history-select-run"), "the hint needs !loadingIndex");
}

#[test]
fn an_empty_index_shows_no_history_and_the_select_run_hint() {
    let mut scene = AuditState::loaded_rest();
    scene.drawer.begin_resync("dash-1", &scene.widget);
    let token = scene.drawer.index_epoch;
    scene.drawer.deliver_index(Vec::new(), token);
    let rt = rt(scene);
    assert!(has(&rt, "run-history-drawer-list-empty"));
    assert_eq!(
        element_text(&rt, "run-history-drawer-list-empty-text").as_deref(),
        Some(crate::i18n::t("dashboard.noHistory"))
    );
    assert_eq!(
        element_text(&rt, "run-history-select-run-text").as_deref(),
        Some(crate::i18n::t("dashboard.selectRun")),
        "React !runError && !selectedId && !loadingIndex"
    );
    assert!(!has(&rt, "run-history-view-toggle"), "no run, no switch");
}

#[test]
fn entries_carry_their_time_status_and_alert_glyph() {
    let rt = rt(AuditState::loaded());
    // The time line is the shared formatter's output for the wire text.
    let expected = crate::support::formatters::format_last_connected(Some("2026-01-01T00:00:00Z"));
    assert_eq!(
        element_text(&rt, "run-history-drawer-entry-run-1-time").as_deref(),
        Some(expected.as_str())
    );
    // `capitalize` on the wire words.
    assert_eq!(element_text(&rt, "run-history-drawer-entry-run-1-status").as_deref(), Some("Ok"));
    assert_eq!(element_text(&rt, "run-history-drawer-entry-run-2-status").as_deref(), Some("Error"));
    // `statusClass`: ok emerald, error danger, on the text ink.
    let k = kit(true);
    let ok_status = maybe(&rt, "run-history-drawer-entry-run-1-status").expect("laid out");
    assert_eq!(ok_status.text_color, Some(palette::run_ok_emerald(true)));
    let error_status = maybe(&rt, "run-history-drawer-entry-run-2-status").expect("laid out");
    assert_eq!(error_status.text_color, Some(k.c.danger.fg));
    // `alertFired` gates the glyph.
    assert!(has(&rt, "run-history-drawer-entry-run-2-alert"));
    assert!(!has(&rt, "run-history-drawer-entry-run-1-alert"));
}

#[test]
fn a_timeout_entry_uses_the_timeout_amber_ink() {
    let mut scene = AuditState::loaded_rest();
    scene.drawer.begin_resync("dash-1", &scene.widget);
    let token = scene.drawer.index_epoch;
    scene.drawer.deliver_index(
        vec![entry("run-t", "2026-01-03T00:00:00Z", WidgetRunStatus::Timeout, false)],
        token,
    );
    let rt = rt(scene);
    let status = maybe(&rt, "run-history-drawer-entry-run-t-status").expect("laid out");
    assert_eq!(status.text_color, Some(palette::timeout_amber(true)));
    assert_eq!(status.text.as_deref(), Some("Timeout"));
}

#[test]
fn pressing_an_entry_reports_the_selection() {
    let mut rt = rt(AuditState::loaded());
    press(&mut rt, "run-history-drawer-entry-run-2");
    assert_eq!(rt.state.selections, vec!["run-2".to_owned()]);
    assert!(rt.state.drawer.loading_run, "the host armed the run fetch");
    assert_eq!(rt.state.drawer.run_fetch_due(), Some("run-2"));
}

#[test]
fn the_close_button_fires_on_close() {
    let mut rt = rt(AuditState::loaded());
    press(&mut rt, "run-history-drawer-close");
    assert_eq!(rt.state.closes, 1);
    assert!(!rt.state.drawer.open, "the host applied close");
}

#[test]
fn the_index_error_banner_shows_the_text() {
    let mut scene = AuditState::loaded_rest();
    scene.drawer.begin_resync("dash-1", &scene.widget);
    let token = scene.drawer.index_epoch;
    scene.drawer.deliver_index_error("Error: index boom".to_owned(), token);
    let rt = rt(scene);
    assert_eq!(
        element_text(&rt, "run-history-drawer-index-error-text").as_deref(),
        Some("Error: index boom")
    );
    // The error left the list empty, so the left column shows noHistory and
    // the right column the select-run hint (both React branches).
    assert!(has(&rt, "run-history-drawer-list-empty"));
    assert!(has(&rt, "run-history-select-run"));
}

// ---- the right column ----------------------------------------------------------

#[test]
fn an_ok_run_shows_the_view_toggle_and_the_chart() {
    let rt = rt(AuditState::loaded());
    assert!(has(&rt, "run-history-view-toggle"), "data-testid");
    assert!(has(&rt, "run-history-view-chart"));
    assert!(has(&rt, "run-history-view-table"));
    assert!(has(&rt, "run-history-chart"));
    assert!(has(&rt, "chart-canvas-container"), "the T008 canvas mounts");
    assert!(has(&rt, renderer_note_key(ChartType::Bar)));
}

#[test]
fn the_table_toggle_switches_to_the_grid_and_back() {
    let mut rt = rt(AuditState::loaded());
    press(&mut rt, "run-history-view-table");
    assert_eq!(rt.state.view_modes, vec![TileViewMode::Table]);
    assert!(has(&rt, "run-history-table"));
    assert!(has(&rt, "run-history-table-grid"));
    assert!(has(&rt, "run-history-table-grid-head"));
    assert_eq!(
        element_text(&rt, "run-history-table-grid-cell-0-0-text").as_deref(),
        Some("Alpha"),
        "the cell text is the JS String(value) form"
    );
    assert!(!has(&rt, "run-history-chart"), "no chart in table mode");

    press(&mut rt, "run-history-view-chart");
    assert_eq!(rt.state.view_modes, vec![TileViewMode::Table, TileViewMode::Chart]);
    assert!(has(&rt, "run-history-chart"));
    assert!(!has(&rt, "run-history-table"));
}

#[test]
fn the_toggle_only_exists_for_an_ok_run() {
    let mut scene = AuditState::loaded_rest();
    scene.drawer.begin_resync("dash-1", &scene.widget);
    let token = scene.drawer.index_epoch;
    scene.drawer.deliver_index(entries(), token);
    let token = scene.drawer.run_epoch;
    scene.drawer.deliver_run(
        WidgetRunVm {
            status: WidgetRunStatus::Error,
            error: Some("Query failed".to_owned()),
            ..ok_run()
        },
        token,
    );
    let rt = rt(scene);
    assert!(!has(&rt, "run-history-view-toggle"), "React loadedRun?.status === 'ok'");
    assert_eq!(
        element_text(&rt, "run-history-error-state-text").as_deref(),
        Some("Query failed")
    );
}

#[test]
fn an_error_run_without_a_message_falls_back_to_the_i18n_key() {
    let mut scene = AuditState::loaded_rest();
    scene.drawer.begin_resync("dash-1", &scene.widget);
    let token = scene.drawer.index_epoch;
    scene.drawer.deliver_index(entries(), token);
    let token = scene.drawer.run_epoch;
    scene.drawer.deliver_run(
        WidgetRunVm {
            status: WidgetRunStatus::Error,
            error: None,
            ..ok_run()
        },
        token,
    );
    let rt = rt(scene);
    assert_eq!(
        element_text(&rt, "run-history-error-state-text").as_deref(),
        Some(crate::i18n::t("dashboard.runError")),
        "React run.error ?? t('dashboard.runError')"
    );
}

#[test]
fn a_timeout_run_shows_the_timeout_state() {
    let mut scene = AuditState::loaded_rest();
    scene.drawer.begin_resync("dash-1", &scene.widget);
    let token = scene.drawer.index_epoch;
    scene.drawer.deliver_index(entries(), token);
    let token = scene.drawer.run_epoch;
    scene.drawer.deliver_run(
        WidgetRunVm {
            status: WidgetRunStatus::Timeout,
            ..ok_run()
        },
        token,
    );
    let rt = rt(scene);
    assert_eq!(
        element_text(&rt, "run-history-timeout-state-text").as_deref(),
        Some(crate::i18n::t("dashboard.runTimeout"))
    );
}

#[test]
fn a_failed_run_load_shows_the_load_error_and_drops_the_run() {
    let mut scene = AuditState::loaded_rest();
    scene.drawer.begin_resync("dash-1", &scene.widget);
    let token = scene.drawer.index_epoch;
    scene.drawer.deliver_index(entries(), token);
    let token = scene.drawer.run_epoch;
    scene.drawer.deliver_run_error("Error: run load failed".to_owned(), token);
    let rt = rt(scene);
    assert_eq!(
        element_text(&rt, "run-history-load-error-text").as_deref(),
        Some("Error: run load failed")
    );
    assert!(!has(&rt, "run-history-view-toggle"), "loadedRun(null) hides the switch");
    assert!(!has(&rt, "run-history-chart"));
}

#[test]
fn an_unconfigurable_chart_shows_the_hint_by_row_count() {
    // rowCount > 0 but the seam delivered no points.
    let mut scene = AuditState::loaded();
    scene.supply_chart_data = false;
    let unconfigured_rt = rt(scene);
    assert_eq!(
        element_text(&unconfigured_rt, "run-history-empty-chart-text").as_deref(),
        Some(crate::i18n::t("dashboard.chartNotConfigured"))
    );
    assert!(!has(&unconfigured_rt, "run-history-chart"));

    let mut scene = AuditState::loaded();
    scene.supply_chart_data = false;
    scene.drawer.loaded_run = Some(WidgetRunVm {
        row_count: 0,
        rows: Vec::new(),
        ..ok_run()
    });
    let rt = rt(scene);
    assert_eq!(
        element_text(&rt, "run-history-empty-chart-text").as_deref(),
        Some(crate::i18n::t("dashboard.emptyResult")),
        "React rowCount === 0 ? emptyResult : chartNotConfigured"
    );
}

#[test]
fn an_empty_table_shows_the_empty_result_state() {
    let mut scene = AuditState::loaded();
    scene.drawer.set_view_mode(TileViewMode::Table);
    scene.drawer.loaded_run = Some(WidgetRunVm {
        row_count: 0,
        rows: Vec::new(),
        ..ok_run()
    });
    let rt = rt(scene);
    assert!(has(&rt, "run-history-empty-table"));
    assert!(!has(&rt, "run-history-table"), "React hides the table wrapper");
}

#[test]
fn the_view_toggle_stays_while_the_next_run_loads() {
    let mut rt = rt(AuditState::loaded());
    rt.update(|s, _| {
        assert!(s.drawer.select("run-2"));
    });
    settle(&mut rt);
    // React gates the switch on the loaded run alone; the loading overlay
    // replaces the content beneath it.
    assert!(has(&rt, "run-history-view-toggle"));
    assert!(has(&rt, "run-history-loading"));
    assert!(has(&rt, "run-history-loading-spinner"));
    assert!(!has(&rt, "run-history-chart"), "the content branches are !loadingRun");
}

#[test]
fn the_selected_entry_paints_the_raised_fill() {
    let rt = rt(AuditState::loaded());
    let k = kit(true);
    let selected = maybe(&rt, "run-history-drawer-entry-run-1").expect("laid out");
    assert_eq!(selected.fill, k.c.surface_raised, "React bg-surface-raised");
    let other = maybe(&rt, "run-history-drawer-entry-run-2").expect("laid out");
    assert_ne!(other.fill, k.c.surface_raised, "unselected stays plain");
}

// ---- geometry -----------------------------------------------------------------

#[test]
fn the_drawer_fits_the_960_window_in_both_themes() {
    for dark in [true, false] {
        let mut scene = AuditState::loaded();
        scene.dark = dark;
        let mut rt = rt(scene);
        settle(&mut rt);
        let mut outside = Vec::new();
        for e in rt.elements() {
            let [x, y, w, h] = e.rect;
            if x < -0.5 || y < -0.5 || x + w > 960.5 || y + h > 640.5 {
                outside.push(format!("{:?}", e.key));
            }
        }
        assert!(outside.is_empty(), "dark={dark}: {outside:?}");
    }
}
