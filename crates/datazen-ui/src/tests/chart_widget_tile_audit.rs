//! Independent audit for T069 (the ChartWidgetTile Rune port).
//!
//! Tester-written: the coder's module is a black box and every assertion
//! below re-reads `packages/frontend/src/windows/dashboard/ChartWidgetTile.tsx`
//! and its React test `__tests__/ChartWidgetTile.test.tsx`: the tile shell
//! and its keys, the header's conditional switches, the eight body states,
//! the fired-alert glyph, the expand overlay's gate/geometry/close path, the
//! axis-configurator seam, and the pure derivations (`hasRenderableChart`,
//! the run→result adaptation, the field inference). Element evidence comes
//! from `Runtime::elements()` (runtime layout), never from a value the
//! component reports about itself; presses go through real pointer events.
//!
//! The chart transform (`widgetRunToChartData`, T030/T182) and
//! `AxisConfigurator` (T319) are documented seams: the audit feeds the tile
//! transformed points and a stand-in axis slot exactly as the host will, and
//! asserts the tile's own gating on top of them.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::chart::chart_canvas::{
    renderer_note_key, ChartConfig, ChartDataPoint, ChartType, ChartValue as CanvasValue,
};
use crate::support::chart_field_inference::ChartValue;
use crate::views::dashboard::chart_widget_tile::{
    has_renderable_chart, run_fields, tile_root_key, widget_run_to_chart_result, ChartWidgetTile,
    ChartWidgetTileState, ChartWidgetVm, TileViewMode, WidgetRunStatus, WidgetRunVm,
};

// ---- Fixtures ---------------------------------------------------------------

fn widget() -> ChartWidgetVm {
    ChartWidgetVm {
        id: "w1".to_owned(),
        title: "Sales Chart".to_owned(),
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
    widget: ChartWidgetVm,
    run: Option<WidgetRunVm>,
    busy: bool,
    points: Vec<ChartDataPoint>,
    tile: ChartWidgetTileState,
    supply_axis_slot: bool,
    dark: bool,
    edits: usize,
    deletes: usize,
    histories: usize,
    refreshes: usize,
    view_modes: Vec<TileViewMode>,
    chart_types: Vec<ChartType>,
    configs: Vec<ChartConfig>,
}

impl AuditState {
    fn chart_scene() -> Self {
        Self {
            widget: widget(),
            run: Some(ok_run()),
            busy: false,
            points: seam_points(),
            tile: ChartWidgetTileState::default(),
            supply_axis_slot: false,
            dark: true,
            edits: 0,
            deletes: 0,
            histories: 0,
            refreshes: 0,
            view_modes: Vec::new(),
            chart_types: Vec::new(),
            configs: Vec::new(),
        }
    }

    /// The host's memo gates: chart view + a widget config.
    fn seam_slice(&self) -> Option<&[ChartDataPoint]> {
        if self.run.is_some()
            && matches!(self.widget.view_mode, TileViewMode::Chart)
            && self.widget.chart_config.is_some()
        {
            Some(self.points.as_slice())
        } else {
            None
        }
    }
}

fn view(s: &AuditState, _cx: &mut Cx<AuditState>) -> El {
    let mut layer = Vec::new();
    let mut tile = ChartWidgetTile::<AuditState>::new(&s.widget, &s.tile)
        .run(s.run.as_ref())
        .busy(s.busy)
        .chart_data(s.seam_slice())
        .on_edit(|s: &mut AuditState| s.edits += 1)
        .on_delete(|s: &mut AuditState| s.deletes += 1)
        .on_history(|s: &mut AuditState| s.histories += 1)
        .on_refresh(|s: &mut AuditState| s.refreshes += 1)
        .on_expand(|s: &mut AuditState| s.tile.begin_expand(&s.widget))
        .on_collapse(|s: &mut AuditState| s.tile.collapse())
        .on_view_mode_change(|s: &mut AuditState, mode: TileViewMode| {
            s.view_modes.push(mode);
            s.widget.view_mode = mode;
        })
        .on_chart_type_change(|s: &mut AuditState, chart_type: ChartType| {
            s.chart_types.push(chart_type);
            if let Some(config) = s.widget.chart_config.as_mut() {
                config.chart_type = chart_type;
            }
        })
        .on_chart_config_change(|s: &mut AuditState, config: ChartConfig| {
            s.configs.push(config.clone());
            s.tile.set_overlay_config(config);
        });
    if s.supply_axis_slot {
        // The T319 seam: a stand-in AxisConfigurator element, keyed so the
        // overlay's gate is observable.
        tile = tile.axis_configurator(col().key("audit-axis-slot").w(180.0).h_full());
    }
    let tile_el = tile.build(s.dark, &mut layer);
    // The layer children ride the root so elements() sees them.
    let mut root = col().key("probe");
    root = root.child(tile_el);
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
    rt.frame(0.0);
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

// ---- the pure derivations ---------------------------------------------------

#[test]
fn has_renderable_chart_is_reacts_predicate() {
    let points = seam_points();
    let mut config = widget().chart_config.unwrap();
    assert!(
        has_renderable_chart(Some(points.as_slice()), &config),
        "data + one y axis renders"
    );
    assert!(
        !has_renderable_chart(None, &config),
        "React's chartData null renders nothing"
    );
    config.y_axes.clear();
    assert!(
        !has_renderable_chart(Some(points.as_slice()), &config),
        "no y axes renders nothing"
    );
    config.y_axes.push("amount".to_owned());
    assert!(
        !has_renderable_chart(Some(&[]), &config),
        "empty data renders nothing"
    );
}

#[test]
fn widget_run_to_chart_result_types_columns_unknown_and_passes_rows() {
    let result = widget_run_to_chart_result(&ok_run());
    assert_eq!(result.columns.len(), 2);
    assert_eq!(result.columns[0].name, "category");
    assert_eq!(result.columns[0].data_type, "unknown");
    assert_eq!(result.columns[1].name, "amount");
    assert_eq!(result.rows.len(), 2);
    assert_eq!(result.rows[0][0], ChartValue::String("Alpha".to_owned()));
    assert_eq!(result.rows[1][1], ChartValue::Number(200.0));
}

#[test]
fn run_fields_infers_the_runs_columns() {
    let fields = run_fields(&ok_run());
    assert_eq!(fields.len(), 2, "one field per run column");
    assert_eq!(fields[0].name, "category");
    assert_eq!(fields[1].name, "amount");
    // React's try/catch arm ([]) is unreachable here: an empty run has no
    // columns, hence no fields, which is the same observable outcome.
    let mut empty = ok_run();
    empty.columns.clear();
    empty.rows.clear();
    assert!(run_fields(&empty).is_empty());
}

#[test]
fn begin_expand_seeds_the_local_config_and_collapse_clears_the_flag() {
    let mut state = ChartWidgetTileState::default();
    let base = widget();
    state.begin_expand(&base);
    assert!(state.expanded);
    assert_eq!(state.local_config, base.chart_config.clone().unwrap());

    // React's mount initialiser: a widget without a config takes the default.
    let mut bare = base;
    bare.chart_config = None;
    state.begin_expand(&bare);
    assert!(state.expanded);
    assert_eq!(state.local_config, ChartConfig::default());

    state.set_overlay_config(widget().chart_config.unwrap());
    state.collapse();
    assert!(!state.expanded, "collapse only clears the flag");
}

// ---- the tile shell (React test 1) ------------------------------------------

#[test]
fn renders_tile_title_and_the_chart_view_toggle() {
    let rt = rt(AuditState::chart_scene());
    assert!(
        has(&rt, &tile_root_key("w1")),
        "data-testid dashboard-tile + widget id w1"
    );
    assert_eq!(
        element_text(&rt, "dashboard-tile-title").as_deref(),
        Some("Sales Chart")
    );
    assert!(has(&rt, "dashboard-view-toggle"));
    assert!(has(&rt, "widget-view-chart"));
    assert!(has(&rt, "widget-view-table"));
    assert!(has(&rt, "dashboard-chart-type-toggle"), "chart mode + wired callback");
    // The chart body mounts the T008 canvas with its placeholder renderer.
    assert!(has(&rt, "dashboard-tile-chart"));
    assert!(has(&rt, "chart-canvas-container"));
    assert!(has(&rt, renderer_note_key(ChartType::Bar)));
}

#[test]
fn the_tile_is_the_fixed_height_card() {
    let rt = rt(AuditState::chart_scene());
    let tile = maybe(&rt, &tile_root_key("w1")).expect("the tile is laid out");
    let [x, y, w, h] = tile.rect;
    assert!((h - 320.0).abs() < 0.5, "React h-80, got {h}");
    assert!(w > 0.0 && w <= 960.5, "fills its cell, got {w}");
    assert!(x >= -0.5 && y >= -0.5, "on-screen at {x},{y}");
}

// ---- the header switches (React test 2) --------------------------------------

#[test]
fn the_table_toggle_switches_the_view_mode() {
    let mut rt = rt(AuditState::chart_scene());
    press(&mut rt, "widget-view-table");
    assert_eq!(rt.state.view_modes, vec![TileViewMode::Table]);
    assert!(matches!(rt.state.widget.view_mode, TileViewMode::Table));
}

#[test]
fn a_type_button_reports_its_chart_type() {
    let mut rt = rt(AuditState::chart_scene());
    press(&mut rt, "dashboard-chart-type-toggle-pie");
    assert_eq!(rt.state.chart_types, vec![ChartType::Pie]);
}

#[test]
fn the_type_switch_needs_chart_mode() {
    // React: `onChartTypeChange && viewMode === 'chart'` (the callback is
    // always wired in this audit's host).
    let mut scene = AuditState::chart_scene();
    scene.widget.view_mode = TileViewMode::Table;
    let rt = rt(scene);
    assert!(!has(&rt, "dashboard-chart-type-toggle"));
}

#[test]
fn edit_history_and_refresh_presses_reach_the_host() {
    // React test 4.
    let mut rt = rt(AuditState::chart_scene());
    press(&mut rt, "dashboard-tile-edit");
    press(&mut rt, "dashboard-tile-history");
    press(&mut rt, "dashboard-tile-refresh");
    assert_eq!(rt.state.edits, 1);
    assert_eq!(rt.state.histories, 1);
    assert_eq!(rt.state.refreshes, 1);
}

#[test]
fn the_delete_button_fires_when_wired() {
    // The audit's host always wires on_delete, so the button exists; a host
    // that leaves it unwired renders no button (React `{onDelete && …}`) —
    // that half is the builder's Option contract, documented in the module.
    let mut rt = rt(AuditState::chart_scene());
    assert!(has(&rt, "dashboard-tile-delete"));
    press(&mut rt, "dashboard-tile-delete");
    assert_eq!(rt.state.deletes, 1);
}

#[test]
fn the_alert_glyph_needs_a_fired_alert() {
    let mut scene = AuditState::chart_scene();
    scene.run.as_mut().unwrap().alert_fired = true;
    let fired_rt = rt(scene);
    assert!(has(&fired_rt, "dashboard-tile-alert"));

    let rt = rt(AuditState::chart_scene());
    assert!(!has(&rt, "dashboard-tile-alert"), "no glyph without alertFired");
}

#[test]
fn a_disabled_widget_dims_the_whole_tile() {
    let mut scene = AuditState::chart_scene();
    scene.widget.enabled = false;
    let rt = rt(scene);
    let tile = maybe(&rt, &tile_root_key("w1")).expect("laid out");
    assert!(
        tile.opacity < 0.99,
        "React opacity-60 dims the card, got {}",
        tile.opacity
    );
}

// ---- the body states ---------------------------------------------------------

#[test]
fn a_busy_first_run_shows_the_running_state_and_an_inert_refresh() {
    let mut scene = AuditState::chart_scene();
    scene.run = None;
    scene.busy = true;
    let rt = rt(scene);
    assert!(has(&rt, "dashboard-tile-running"));
    assert!(has(&rt, "dashboard-tile-running-spinner"));
    assert!(!has(&rt, "dashboard-tile-no-run"), "busy and idle are exclusive");
    let refresh = maybe(&rt, "dashboard-tile-refresh").expect("laid out");
    assert!(!refresh.focusable, "React disabled (busy) is not focusable");
    assert!(has(&rt, "dashboard-tile-refresh-spinner"), "the glyph swaps for the spinner");
}

#[test]
fn an_idle_never_run_tile_shows_the_no_run_hint() {
    let mut scene = AuditState::chart_scene();
    scene.run = None;
    let rt = rt(scene);
    assert!(has(&rt, "dashboard-tile-no-run"));
    assert_eq!(
        element_text(&rt, "dashboard-tile-no-run-text"),
        Some(crate::i18n::t("dashboard.noRunYet").to_owned())
    );
}

#[test]
fn an_error_run_shows_its_message() {
    // React test 5.
    let mut scene = AuditState::chart_scene();
    scene.run = Some(WidgetRunVm {
        status: WidgetRunStatus::Error,
        error: Some("Query failed".to_owned()),
        ..ok_run()
    });
    let rt = rt(scene);
    assert!(has(&rt, "dashboard-tile-error"));
    assert_eq!(
        element_text(&rt, "dashboard-tile-error-text").as_deref(),
        Some("Query failed")
    );
}

#[test]
fn an_error_run_without_a_message_falls_back_to_the_i18n_key() {
    let mut scene = AuditState::chart_scene();
    scene.run = Some(WidgetRunVm {
        status: WidgetRunStatus::Error,
        error: None,
        ..ok_run()
    });
    let rt = rt(scene);
    assert_eq!(
        element_text(&rt, "dashboard-tile-error-text"),
        Some(crate::i18n::t("dashboard.runError").to_owned()),
        "React run.error ?? t('dashboard.runError')"
    );
}

#[test]
fn a_timeout_run_shows_the_timeout_state() {
    let mut scene = AuditState::chart_scene();
    scene.run = Some(WidgetRunVm {
        status: WidgetRunStatus::Timeout,
        ..ok_run()
    });
    let rt = rt(scene);
    assert!(has(&rt, "dashboard-tile-timeout"));
    assert_eq!(
        element_text(&rt, "dashboard-tile-timeout-text"),
        Some(crate::i18n::t("dashboard.runTimeout").to_owned())
    );
}

#[test]
fn an_unconfigurable_chart_shows_the_hint_and_the_configure_shortcut() {
    // rowCount > 0 but the seam delivered no points: chartNotConfigured plus
    // the accent edit button.
    let mut scene = AuditState::chart_scene();
    scene.points.clear();
    let hint_rt = rt(scene);
    assert!(has(&hint_rt, "dashboard-tile-empty-chart"));
    assert_eq!(
        element_text(&hint_rt, "dashboard-tile-empty-chart-text"),
        Some(crate::i18n::t("dashboard.chartNotConfigured").to_owned())
    );
    assert!(has(&hint_rt, "dashboard-tile-configure"));
    assert!(!has(&hint_rt, "dashboard-tile-chart"), "no chart without data");

    // The shortcut is the edit action (React's `onClick={onEdit}`).
    let mut scene = AuditState::chart_scene();
    scene.points.clear();
    let mut rt = rt(scene);
    press(&mut rt, "dashboard-tile-configure");
    assert_eq!(rt.state.edits, 1, "the shortcut is the edit action");
}

#[test]
fn an_empty_result_chart_shows_empty_without_the_shortcut() {
    let mut scene = AuditState::chart_scene();
    scene.points.clear();
    scene.run = Some(WidgetRunVm {
        row_count: 0,
        rows: Vec::new(),
        ..ok_run()
    });
    let rt = rt(scene);
    assert_eq!(
        element_text(&rt, "dashboard-tile-empty-chart-text"),
        Some(crate::i18n::t("dashboard.emptyResult").to_owned()),
        "React rowCount === 0 ? emptyResult : chartNotConfigured"
    );
    assert!(!has(&rt, "dashboard-tile-configure"), "no shortcut without rows");
}

#[test]
fn the_table_view_renders_the_shared_grid() {
    // React test 3 (viewMode table renders the DataTable).
    let mut scene = AuditState::chart_scene();
    scene.widget.view_mode = TileViewMode::Table;
    let rt = rt(scene);
    assert!(has(&rt, "dashboard-tile-table"));
    assert!(has(&rt, "dashboard-tile-table-grid"));
    assert!(has(&rt, "dashboard-tile-table-grid-head"));
    assert!(has(&rt, "dashboard-tile-table-grid-row-0"));
    assert!(has(&rt, "dashboard-tile-table-grid-cell-0-0"));
    assert_eq!(
        element_text(&rt, "dashboard-tile-table-grid-cell-0-0-text"),
        Some("Alpha".to_owned()),
        "the cell text is the JS String(value) form"
    );
    assert_eq!(
        element_text(&rt, "dashboard-tile-table-grid-cell-0-1-text"),
        Some("100".to_owned())
    );
    assert!(!has(&rt, "dashboard-tile-chart"), "no chart in table mode");
    assert!(!has(&rt, "dashboard-chart-type-toggle"), "the type switch is chart-only");
}

#[test]
fn an_empty_table_shows_the_empty_result_state() {
    let mut scene = AuditState::chart_scene();
    scene.widget.view_mode = TileViewMode::Table;
    scene.run = Some(WidgetRunVm {
        row_count: 0,
        rows: Vec::new(),
        ..ok_run()
    });
    let rt = rt(scene);
    assert!(has(&rt, "dashboard-tile-empty-table"));
    assert!(!has(&rt, "dashboard-tile-table"), "React hides the table wrapper");
}

// ---- the expand overlay ------------------------------------------------------

#[test]
fn the_expand_button_only_exists_for_an_ok_run() {
    let ok_rt = rt(AuditState::chart_scene());
    assert!(has(&ok_rt, "dashboard-tile-expand"));

    let mut scene = AuditState::chart_scene();
    scene.run = Some(WidgetRunVm {
        status: WidgetRunStatus::Error,
        ..ok_run()
    });
    let rt = rt(scene);
    assert!(!has(&rt, "dashboard-tile-expand"), "React run?.status === 'ok'");
}

#[test]
fn expanding_mounts_the_overlay_and_closing_unmounts_it() {
    let mut rt = rt(AuditState::chart_scene());
    assert!(!has(&rt, "dashboard-tile-overlay"));
    press(&mut rt, "dashboard-tile-expand");
    assert!(rt.state.tile.expanded, "the host applied begin_expand");
    settle(&mut rt);
    assert!(has(&rt, "dashboard-tile-overlay"));
    assert!(has(&rt, "dashboard-tile-overlay-header"));
    assert!(has(&rt, "dashboard-tile-overlay-title"));

    press(&mut rt, "dashboard-tile-overlay-collapse");
    assert!(!rt.state.tile.expanded, "the host applied collapse");
    settle(&mut rt);
    assert!(!has(&rt, "dashboard-tile-overlay"));
}

#[test]
fn the_overlay_is_pinned_below_the_title_band() {
    // React `fixed inset-0 top-10`: from 40pt down to the window's bottom.
    let mut rt = rt(AuditState::chart_scene());
    rt.update(|s, _| s.tile.begin_expand(&s.widget));
    settle(&mut rt);
    let overlay = maybe(&rt, "dashboard-tile-overlay").expect("the overlay is laid out");
    let [x, y, w, h] = overlay.rect;
    assert!((y - 40.0).abs() < 0.5, "top-10, got {y}");
    assert!((x - 0.0).abs() < 0.5 && (w - 960.0).abs() < 0.5, "inset-x-0, got {x},{w}");
    assert!(
        y + h <= 640.5,
        "the pinned element ends inside the window, got bottom {}",
        y + h
    );
}

#[test]
fn the_overlay_reuses_the_tile_data_while_the_local_config_matches() {
    let mut rt = rt(AuditState::chart_scene());
    rt.update(|s, _| s.tile.begin_expand(&s.widget));
    settle(&mut rt);
    // begin_expand seeds local_config from the widget, so the seam's one
    // delivery serves the overlay too (React recomputes the same memo).
    assert!(has(&rt, "chart-canvas"), "the overlay's non-compact canvas");
    assert!(has(&rt, renderer_note_key(ChartType::Bar)));
}

#[test]
fn the_axis_slot_needs_chart_mode_and_fields() {
    let mut scene = AuditState::chart_scene();
    scene.supply_axis_slot = true;
    scene.tile.begin_expand(&scene.widget);
    let slot_rt = rt(scene);
    assert!(has(&slot_rt, "audit-axis-slot"), "chart mode + two fields");
    assert!(has(&slot_rt, "dashboard-tile-overlay-body"));

    // Table mode hides it even with fields (React's viewMode gate).
    let mut scene = AuditState::chart_scene();
    scene.supply_axis_slot = true;
    scene.widget.view_mode = TileViewMode::Table;
    scene.tile.begin_expand(&scene.widget);
    let rt = rt(scene);
    assert!(!has(&rt, "audit-axis-slot"));
}

#[test]
fn the_overlay_table_matches_the_tile_table() {
    let mut scene = AuditState::chart_scene();
    scene.widget.view_mode = TileViewMode::Table;
    scene.tile.begin_expand(&scene.widget);
    let rt = rt(scene);
    assert!(has(&rt, "dashboard-tile-overlay"));
    // React's overlay DataTable carries no testid of its own — the grid key
    // follows the overlay prefix the port documents.
    assert!(has(&rt, "dashboard-tile-overlay-grid"));
    assert!(has(&rt, "dashboard-tile-overlay-grid-row-1"));
    assert!(!has(&rt, "dashboard-tile-overlay-empty"));
}

#[test]
fn the_overlay_empty_chart_note_follows_the_row_count() {
    let mut scene = AuditState::chart_scene();
    scene.points.clear();
    scene.tile.begin_expand(&scene.widget);
    let unconfigured_rt = rt(scene);
    assert_eq!(
        element_text(&unconfigured_rt, "dashboard-tile-overlay-empty-text"),
        Some(crate::i18n::t("dashboard.chartNotConfigured").to_owned())
    );

    let mut scene = AuditState::chart_scene();
    scene.points.clear();
    scene.run = Some(WidgetRunVm {
        row_count: 0,
        rows: Vec::new(),
        ..ok_run()
    });
    scene.tile.begin_expand(&scene.widget);
    let rt = rt(scene);
    assert_eq!(
        element_text(&rt, "dashboard-tile-overlay-empty-text"),
        Some(crate::i18n::t("dashboard.emptyResult").to_owned())
    );
}

#[test]
fn an_expanded_overlay_does_not_mount_for_a_failed_run() {
    // React: `expanded && run?.status === 'ok'`.
    let mut scene = AuditState::chart_scene();
    scene.run = Some(WidgetRunVm {
        status: WidgetRunStatus::Timeout,
        ..ok_run()
    });
    scene.tile.begin_expand(&scene.widget);
    let rt = rt(scene);
    assert!(!has(&rt, "dashboard-tile-overlay"));
}

// ---- geometry ----------------------------------------------------------------

#[test]
fn the_tile_fits_the_960_window_in_both_themes() {
    for dark in [true, false] {
        let mut scene = AuditState::chart_scene();
        scene.dark = dark;
        scene.tile.begin_expand(&scene.widget);
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
