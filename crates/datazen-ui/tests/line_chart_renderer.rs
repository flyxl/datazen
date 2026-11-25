//! T216: the line chart engine — factory dispatch, the log-hint pipeline,
//! plot geometry, the per-point `dot` layer, the hovered row's `activeDot`
//! layer, the `LabelList` values overlay, the hidden-series toggle,
//! hit-bucket routing and the two tooltip mounts.
//!
//! Tree observations come from `rt.elements()` (runtime-measured
//! rects/text/opacity), pressed and hovered through the runtime's own
//! pointer pipeline; the pure pipeline (`line_chart_plan` and the area
//! engine's lean `lib/chart` slices) is asserted directly, where the exact
//! geometry lives.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::chart::renderers::line_chart_renderer::{
    line_chart_plan, renderer_factory, LineChartRenderer, FormatSeams, ACTIVE_DOTS_KEY, AXES_KEY,
    DOTS_KEY, GRID_KEY, HIT_CELL_PREFIX, HIT_ZONE_KEY, LEGEND_ITEM_PREFIX, LEGEND_LABEL_PREFIX,
    LEGEND_KEY, LEGEND_SWATCH_PREFIX, PLOT_KEY, ROOT_KEY, SERIES_KEY_PREFIX, TOOLTIP_KEY,
    VALUE_PREFIX, VALUES_BAND_PREFIX, VALUES_KEY, X_AXIS_KEY, Y_AXIS_KEY,
};
use datazen_ui::components::chart::chart_canvas::{
    renderer_note_key, ChartCanvas, ChartConfig, ChartDataPoint, ChartType, ChartValue,
    PlaceholderChartRenderer,
};
use datazen_ui::components::chart::log_scale_tooltip::CONTENT_KEY;
use datazen_ui::ui::kit::kit;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn keys_with<S>(rt: &Runtime<S>, prefix: &str) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| {
            e.key
                .as_deref()
                .filter(|k| k.starts_with(prefix))
                .map(|k| k.to_string())
        })
        .collect()
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let r = e.rect;
        if r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5 {
            outside.push(format!("{} {:?}", e.key.unwrap_or(e.path), r));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of window {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---- Fixtures ----------------------------------------------------------------

fn point(label: &str, a: f64, b: f64) -> ChartDataPoint {
    ChartDataPoint::new()
        .with("label", ChartValue::Text(label.into()))
        .with("a", ChartValue::Number(a))
        .with("b", ChartValue::Number(b))
}

/// Two series spanning magnitudes (10–20 against 1000–2000): the log hint's
/// `use` arm.
fn log_spanning_data() -> Vec<ChartDataPoint> {
    vec![point("r0", 10.0, 1000.0), point("r1", 20.0, 2000.0)]
}

/// Two series inside one magnitude: the hint's `!use` arm.
fn near_data() -> Vec<ChartDataPoint> {
    vec![point("r0", 10.0, 15.0), point("r1", 20.0, 25.0)]
}

fn config(y_axes: &[&str]) -> ChartConfig {
    ChartConfig {
        y_axes: y_axes.iter().map(|s| s.to_string()).collect(),
        ..ChartConfig::default()
    }
}

/// A recording `formatCompact` seam: every value it sees is pushed and echoed
/// back as `c:{value}` — the observer for which numbers reached the formatter.
fn recording_compact(seen: &Rc<RefCell<Vec<f64>>>) -> Rc<dyn Fn(f64) -> String> {
    let seen = seen.clone();
    Rc::new(move |v: f64| {
        seen.borrow_mut().push(v);
        format!("c:{v}")
    })
}

fn recording_seams(compact: Rc<RefCell<Vec<f64>>>) -> FormatSeams {
    FormatSeams {
        format_compact: recording_compact(&compact),
        ..FormatSeams::default()
    }
}

// ---- Probe view --------------------------------------------------------------

#[derive(Clone)]
struct Scenario {
    config: ChartConfig,
    data: Vec<ChartDataPoint>,
    dark: bool,
    compact: bool,
    height: Option<f32>,
    engine: LineChartRenderer,
    seams: Option<FormatSeams>,
    seen: Option<Rc<RefCell<Vec<ChartType>>>>,
}

impl Default for Scenario {
    fn default() -> Self {
        Self {
            config: ChartConfig::default(),
            data: Vec::new(),
            dark: true,
            compact: false,
            height: None,
            engine: LineChartRenderer::new(),
            seams: None,
            seen: None,
        }
    }
}

#[derive(Default)]
struct ChartState {
    scenario: Scenario,
    clicks: Vec<usize>,
}

fn line_view(s: &ChartState, _cx: &mut Cx<ChartState>) -> El {
    let sc = &s.scenario;
    let mut engine = sc.engine.clone();
    if let Some(seams) = &sc.seams {
        engine = engine
            .format_compact(Rc::clone(&seams.format_compact))
            .format_axis_tick(Rc::clone(&seams.format_axis_tick))
            .format_epoch_ms(Rc::clone(&seams.format_epoch_ms));
    }
    let mut canvas = ChartCanvas::<ChartState>::new(&sc.config, &sc.data)
        .dark(sc.dark)
        .compact(sc.compact)
        .on_data_point_click(|st: &mut ChartState, i: usize| st.clicks.push(i));
    if let Some(h) = sc.height {
        canvas = canvas.height(h);
    }
    if let Some(seen) = &sc.seen {
        let seen = seen.clone();
        let engine_for_factory = engine.clone();
        canvas = canvas.renderer_factory(move |t| {
            seen.borrow_mut().push(t);
            match t {
                ChartType::Line => Box::new(engine_for_factory.clone()),
                other => Box::new(PlaceholderChartRenderer::new(other)),
            }
        });
    } else {
        canvas = canvas.renderer_factory(renderer_factory(engine));
    }
    col().w_full().h_full().child(canvas.build())
}

fn runtime(sc: Scenario, size: (f32, f32)) -> Runtime<ChartState> {
    let dark = sc.dark;
    let mut rt = Runtime::new(
        ChartState {
            scenario: sc,
            ..ChartState::default()
        },
        line_view,
        size,
        env(dark),
    );
    for _ in 0..24 { rt.frame(1.0 / 60.0); }
    rt
}

fn scenario(config: ChartConfig, data: Vec<ChartDataPoint>) -> Scenario {
    Scenario {
        config,
        data,
        ..Scenario::default()
    }
}

fn line_scenario(data: Vec<ChartDataPoint>) -> Scenario {
    let mut config = config(&["a", "b"]);
    config.chart_type = ChartType::Line;
    scenario(config, data)
}

fn click_at(rt: &mut Runtime<ChartState>, x: f32, y: f32) {
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn click_key_center(rt: &mut Runtime<ChartState>, key: &str) {
    let r = element(rt, key).rect;
    click_at(rt, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
}

fn hover_at(rt: &mut Runtime<ChartState>, x: f32, y: f32) {
    rt.pointer_move(x, y);
    rt.frame(1.0 / 60.0);
}

fn hover_key_center(rt: &mut Runtime<ChartState>, key: &str) {
    let r = element(rt, key).rect;
    hover_at(rt, r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
}

// ---- Factory dispatch ---------------------------------------------------------

#[test]
fn the_factory_routes_line_to_the_engine_and_other_types_to_placeholders() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut sc = line_scenario(log_spanning_data());
    sc.seen = Some(seen.clone());
    let rt = runtime(sc, (960.0, 640.0));

    assert!(has_key(&rt, ROOT_KEY), "the engine's frame is mounted");
    assert!(
        !has_key(&rt, renderer_note_key(ChartType::Line)),
        "the Line placeholder note is gone"
    );
    assert!(
        seen.borrow().iter().all(|t| *t == ChartType::Line),
        "every build selected the line engine: {:?}",
        seen.borrow()
    );

    // The other four types still get their honest placeholders.
    let seen_bar = Rc::new(RefCell::new(Vec::new()));
    let mut bar_config = config(&["a", "b"]);
    bar_config.chart_type = ChartType::Bar;
    let mut sc_bar = scenario(bar_config, log_spanning_data());
    sc_bar.seen = Some(seen_bar.clone());
    let rt_bar = runtime(sc_bar, (960.0, 640.0));
    assert!(
        has_key(&rt_bar, renderer_note_key(ChartType::Bar)),
        "Bar keeps its placeholder through the same factory"
    );
    assert!(!has_key(&rt_bar, ROOT_KEY), "no line frame for Bar");
}

// ---- The log-hint pipeline ------------------------------------------------------

#[test]
fn the_log_hint_maps_series_that_span_magnitudes_into_log10_space() {
    let compact = Rc::new(RefCell::new(Vec::new()));
    let seams = recording_seams(compact.clone());
    let data = log_spanning_data();
    let cfg = config(&["a", "b"]);
    let plan = line_chart_plan(&data, &cfg, &BTreeSet::new(), true, &seams);

    // computeLogScaleHint: 2000/10 = 200 ≥ 10, no negatives → use, domainMin 10.
    assert!(plan.log_hint.use_log);
    assert_eq!(plan.log_hint.domain_min, 10.0);

    // mapToLogScale rewrote the slots and stashed the originals.
    let r0 = &plan.chart_data[0];
    assert_eq!(r0.get("a"), Some(&ChartValue::Number(1.0))); // log10(10)
    assert_eq!(r0.get("__orig__a"), Some(&ChartValue::Number(10.0)));
    assert_eq!(r0.get("b"), Some(&ChartValue::Number(3.0))); // log10(1000)
    assert_eq!(r0.get("__orig__b"), Some(&ChartValue::Number(1000.0)));
    let r1 = &plan.chart_data[1];
    match r1.get("a") {
        Some(ChartValue::Number(v)) => assert!((v - 20.0_f64.log10()).abs() < 1e-9),
        other => panic!("mapped a at r1 is log10(20), got {other:?}"),
    }
    assert_eq!(r1.get("__orig__a"), Some(&ChartValue::Number(20.0)));

    // The Y domain is over the mapped values, bottom-clamped at 0.
    assert!((plan.y_domain[0] - 0.0).abs() < 1e-12);
    assert!((plan.y_domain[1] - 2000.0_f64.log10()).abs() < 1e-12);

    // YAxis tickFormatter = logTickFormatter: the compact seam saw 10^v (the
    // same round trip the engine computed, so the spelling is not the assert).
    let top = &plan.y_ticks[0];
    assert!((top.value - 2000.0_f64.log10()).abs() < 1e-9, "top tick first");
    let want_top = format!("c:{}", 10.0_f64.powf(2000.0_f64.log10()));
    assert_eq!(top.label, want_top);
    let seen = compact.borrow();
    assert!(
        seen.iter().any(|v| (v - 2000.0).abs() < 1e-6),
        "the seam formatted 10^log10(2000), saw {seen:?}"
    );
    assert!(
        seen.iter().any(|v| (v - 1.0).abs() < 1e-6),
        "the bottom tick formatted 10^0, saw {seen:?}"
    );

    // The plan's series points follow the mapped values through the domain
    // (band x, value-scaled y).
    let a = &plan.series[0];
    let span = 2000.0_f64.log10();
    match a.points[0] {
        Some([x, y]) => {
            assert!((x - 0.25).abs() < 1e-6, "band centre: {x}");
            assert!((y - (1.0 - 1.0 / span) as f32).abs() < 1e-6, "log10(10) in [0, span]: {y}");
        }
        other => panic!("r0 has a point, got {other:?}"),
    }
}

#[test]
fn the_log_hint_leaves_one_magnitude_data_untouched() {
    let plan = line_chart_plan(
        &near_data(),
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert!(!plan.log_hint.use_log);
    assert_eq!(plan.log_hint.domain_min, 1.0, "the off arm's default");
    // The rows are the input, no __orig__ stash.
    assert_eq!(plan.chart_data[0].get("a"), Some(&ChartValue::Number(10.0)));
    assert_eq!(plan.chart_data[0].get("__orig__a"), None);
    // The domain is the plain [0, 25] and the ticks read it.
    assert_eq!(plan.y_domain, [0.0, 25.0]);
    assert_eq!(plan.y_ticks[0].label, "25");
    assert_eq!(plan.y_ticks[4].label, "0");
}

#[test]
fn string_series_values_pass_the_ts_number_coercions_into_the_pipeline() {
    // `toNumber`: strings parse (`Number('10') === 10`), a NaN parse is
    // `null` (skipped), null is skipped — the three arms the pipeline reads.
    let data = vec![
        ChartDataPoint::new()
            .with("a", ChartValue::Text("10".into()))
            .with("b", ChartValue::Text("1000".into())),
        ChartDataPoint::new()
            .with("a", ChartValue::Text(" 20 ".into()))
            .with("b", ChartValue::Number(2000.0)),
    ];
    let series = vec!["a".to_string(), "b".to_string()];
    let hint = datazen_ui::components::chart::renderers::area_chart_renderer::compute_log_scale_hint(
        &data, &series,
    );
    assert!(hint.use_log, "string numbers count: 2000/10 >= 10");
    assert_eq!(hint.domain_min, 10.0);

    let mapped = datazen_ui::components::chart::renderers::area_chart_renderer::map_to_log_scale(
        &data,
        &series,
        hint.domain_min,
    );
    assert_eq!(mapped[0].get("a"), Some(&ChartValue::Number(1.0))); // log10(10)
    assert_eq!(mapped[0].get("__orig__a"), Some(&ChartValue::Number(10.0)));
    // `Number(' 20 ')` trims before it parses, as JS does.
    match mapped[1].get("a") {
        Some(ChartValue::Number(v)) => assert!((v - 20.0_f64.log10()).abs() < 1e-9),
        other => panic!("' 20 ' parsed to log10(20), got {other:?}"),
    }
    assert_eq!(mapped[1].get("__orig__a"), Some(&ChartValue::Number(20.0)));

    // A NaN parse is toNumber's null: the slot is untouched, no __orig__ stash.
    let nan_text = vec![ChartDataPoint::new()
        .with("a", ChartValue::Text("n/a".into()))
        .with("b", ChartValue::Number(1000.0))];
    let hint = datazen_ui::components::chart::renderers::area_chart_renderer::compute_log_scale_hint(
        &nan_text, &series,
    );
    assert!(!hint.use_log, "the NaN string never counted as a value");
    let mapped = datazen_ui::components::chart::renderers::area_chart_renderer::map_to_log_scale(
        &nan_text,
        &series,
        10.0,
    );
    assert_eq!(mapped[0].get("a"), Some(&ChartValue::Text("n/a".into())));
    assert_eq!(mapped[0].get("__orig__a"), None, "v == null → continue");
    assert_eq!(mapped[0].get("__orig__b"), Some(&ChartValue::Number(1000.0)));

    // Through the line plan: the NaN row is a gap (no point, no dot, no label).
    let mut cfg = config(&["a", "b"]);
    cfg.show_values = true;
    let plan = line_chart_plan(&nan_text, &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(plan.series[0].points, vec![None], "the NaN parse draws nothing");
    assert_eq!(plan.series[0].labels, vec![None], "and labels nothing");
}

#[test]
fn null_slots_break_the_line_and_skip_their_dots_and_labels() {
    // React 105-136: recharts breaks `<Line>` at null/NaN (connectNulls stays
    // false) — no segment, no `dot`, no `LabelList` entry for the row.
    let data = vec![
        ChartDataPoint::new()
            .with("a", ChartValue::Number(10.0))
            .with("b", ChartValue::Number(10.0)),
        ChartDataPoint::new()
            .with("a", ChartValue::Null)
            .with("b", ChartValue::Number(20.0)),
        ChartDataPoint::new()
            .with("a", ChartValue::Number(30.0))
            .with("b", ChartValue::Number(30.0)),
    ];
    // (The spread is 30/10 = 3 < 10x — the log hint stays off and the domain
    // is the plain [0, 30].)
    let mut cfg = config(&["a", "b"]);
    cfg.show_values = true;
    let plan = line_chart_plan(&data, &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(
        plan.series[0].points,
        vec![
            Some([(0.0 + 0.5) / 3.0, (1.0 - 10.0 / 30.0) as f32]),
            None,
            Some([(2.0 + 0.5) / 3.0, (1.0 - 30.0 / 30.0) as f32]),
        ]
    );
    assert_eq!(plan.series[0].labels, vec![Some("10".into()), None, Some("30".into())]);
    assert_eq!(
        plan.series[1].points,
        vec![
            Some([(0.0 + 0.5) / 3.0, (1.0 - 10.0 / 30.0) as f32]),
            Some([(1.0 + 0.5) / 3.0, (1.0 - 20.0 / 30.0) as f32]),
            Some([(2.0 + 0.5) / 3.0, (1.0 - 30.0 / 30.0) as f32]),
        ],
        "series b keeps every row"
    );
}

// ---- The time domain ---------------------------------------------------------------

#[test]
fn a_time_domain_positions_points_by_epoch_and_ticks_by_the_configured_values() {
    let epoch_seen = Rc::new(RefCell::new(Vec::new()));
    let epoch = {
        let seen = epoch_seen.clone();
        Rc::new(move |v: f64| {
            seen.borrow_mut().push(v);
            format!("e:{v}")
        })
    };
    let seams = FormatSeams {
        format_epoch_ms: epoch,
        ..FormatSeams::default()
    };
    let data = vec![
        ChartDataPoint::new()
            .with("t", ChartValue::Number(0.0))
            .with("a", ChartValue::Number(1.0))
            .with("b", ChartValue::Number(2.0)),
        ChartDataPoint::new()
            .with("t", ChartValue::Number(75.0))
            .with("a", ChartValue::Number(3.0))
            .with("b", ChartValue::Number(4.0)),
    ];
    let mut cfg = config(&["a", "b"]);
    cfg.x_axis = Some("t".into()); // the time axis reads the epoch column
    cfg.time_domain = Some([0.0, 100.0]);
    cfg.time_ticks = vec![0.0, 50.0];
    let plan = line_chart_plan(&data, &cfg, &BTreeSet::new(), true, &seams);

    // X positions come from the epoch value scale, not the band index.
    assert_eq!(plan.series[0].points, vec![Some([0.0, 0.75]), Some([0.75, 0.25])]);
    // Ticks come from config.timeTicks through the epoch seam.
    let labels: Vec<&str> = plan.x_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, vec!["e:0", "e:50"]);
    assert_eq!(plan.grid_xs, vec![0.0, 0.5]);
    let seen = epoch_seen.borrow();
    assert_eq!(*seen, vec![0.0, 50.0]);
}

#[test]
fn time_domain_rows_outside_the_domain_stay_gaps_and_ticks_clamp_to_the_plot() {
    // React 62-66 (`domain={config.timeDomain}` + `ticks={config.timeTicks}`):
    // rows outside the domain are hidden (recharts clips them away), and the
    // tick fractions clamp to the plot's edges.
    let data = vec![
        ChartDataPoint::new()
            .with("t", ChartValue::Number(0.0))
            .with("a", ChartValue::Number(1.0))
            .with("b", ChartValue::Number(2.0)),
        ChartDataPoint::new()
            .with("t", ChartValue::Number(50.0))
            .with("a", ChartValue::Number(3.0))
            .with("b", ChartValue::Number(4.0)),
        ChartDataPoint::new()
            .with("t", ChartValue::Number(200.0))
            .with("a", ChartValue::Number(5.0))
            .with("b", ChartValue::Number(6.0)),
    ];
    let mut cfg = config(&["a", "b"]);
    cfg.x_axis = Some("t".into());
    cfg.time_domain = Some([0.0, 100.0]);
    cfg.time_ticks = vec![0.0, 50.0, 150.0];
    let plan = line_chart_plan(&data, &cfg, &BTreeSet::new(), true, &FormatSeams::default());

    // Rows 0/1 draw at their epoch fractions; the t=200 row is a gap.
    assert_eq!(
        plan.series[0].points,
        vec![
            Some([0.0, (1.0 - 1.0 / 6.0) as f32]),
            Some([0.5, 0.5]),
            None,
        ]
    );
    // The 150 tick clamps to the right edge (fraction 1, band 2).
    assert_eq!(plan.grid_xs, vec![0.0, 0.5, 1.0]);
    let rows: Vec<usize> = plan.x_ticks.iter().map(|t| t.row).collect();
    assert_eq!(rows, vec![0, 1, 2]);
}

#[test]
fn a_time_domain_chart_mounts_neither_dot_layer_even_while_hovered() {
    // React 115-118 (`dot={config.timeDomain ? false : ...}` and
    // `activeDot={config.timeDomain ? false : ...}`): BOTH dot layers stay
    // off under a time domain, while the tooltip still mounts.
    let engine = LineChartRenderer::new();
    let mut cfg = config(&["a", "b"]);
    cfg.chart_type = ChartType::Line;
    cfg.x_axis = Some("t".into());
    cfg.time_domain = Some([0.0, 100.0]);
    let data = vec![
        ChartDataPoint::new()
            .with("t", ChartValue::Number(0.0))
            .with("a", ChartValue::Number(1.0))
            .with("b", ChartValue::Number(2.0)),
        ChartDataPoint::new()
            .with("t", ChartValue::Number(75.0))
            .with("a", ChartValue::Number(3.0))
            .with("b", ChartValue::Number(4.0)),
    ];
    let mut sc = scenario(cfg, data);
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));

    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert!(has_key(&rt, TOOLTIP_KEY), "the plain tooltip still mounts");
    assert!(
        !has_key(&rt, DOTS_KEY),
        "React 115: dot is false under a time domain"
    );
    assert!(
        !has_key(&rt, ACTIVE_DOTS_KEY),
        "React 117: activeDot is false under a time domain"
    );
    assert!(!has_key(&rt, CONTENT_KEY), "no log hint (4 / 1 < 10)");
    assert_eq!(engine.hovered_row(), Some(0), "the hover is retained");
}

// ---- The hidden-series toggle ---------------------------------------------------

#[test]
fn a_legend_press_hides_and_restores_a_series() {
    let engine = LineChartRenderer::new();
    let mut sc = line_scenario(near_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));

    assert!(has_key(&rt, "line-chart-series-a") && has_key(&rt, "line-chart-series-b"));
    assert!(has_key(&rt, DOTS_KEY), "the categorical dot layer is mounted");
    assert!(has_key(&rt, LEGEND_KEY), "multiSeries mounts the legend");

    // Press the alpha legend entry → its line disappears, beta stays.
    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert!(engine.hidden_series().contains("a"), "engine owns the set");
    assert!(!has_key(&rt, "line-chart-series-a"), "the hidden line is gone");
    assert!(has_key(&rt, "line-chart-series-b"), "the other series stays");
    // React's formatter dims the hidden entry's name to 0.35.
    assert!(
        (element(&rt, &format!("{LEGEND_LABEL_PREFIX}a")).opacity - 0.35).abs() < 1e-3,
        "the hidden label dims"
    );
    assert_eq!(element(&rt, &format!("{LEGEND_LABEL_PREFIX}b")).opacity, 1.0);

    // Press again → restored.
    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert!(has_key(&rt, "line-chart-series-a"));
    assert!(engine.hidden_series().is_empty());
    assert_eq!(element(&rt, &format!("{LEGEND_LABEL_PREFIX}a")).opacity, 1.0);
}

#[test]
fn a_single_series_mounts_no_legend() {
    let mut cfg = config(&["a"]);
    cfg.chart_type = ChartType::Line;
    let rt = runtime(scenario(cfg, near_data()), (960.0, 640.0));
    assert!(!has_key(&rt, LEGEND_KEY), "React gates the legend on multiSeries");
    assert!(has_key(&rt, "line-chart-series-a"));
}

// ---- Geometry --------------------------------------------------------------------

#[test]
fn the_line_grid_axes_series_and_dot_canvases_fill_the_plot_rect() {
    let rt = runtime(line_scenario(near_data()), (960.0, 640.0));
    let plot = element(&rt, PLOT_KEY).rect;
    assert!(
        plot[2] > 200.0 && plot[3] > 200.0,
        "a real plot region, got {plot:?}"
    );

    for key in [
        GRID_KEY,
        AXES_KEY,
        "line-chart-series-a",
        "line-chart-series-b",
        DOTS_KEY,
    ] {
        let r = element(&rt, key).rect;
        for i in 0..4 {
            assert!(
                (r[i] - plot[i]).abs() <= 0.5,
                "{key} shares the plot rect (dim {i}): {r:?} vs {plot:?}"
            );
        }
    }

    // The Y-axis column abuts the plot and matches its height.
    let y_axis = element(&rt, Y_AXIS_KEY).rect;
    assert!((y_axis[0] + y_axis[2] - plot[0]).abs() <= 0.5, "{y_axis:?}");
    assert!((y_axis[3] - plot[3]).abs() <= 0.5, "{y_axis:?}");

    // X tick labels sit centred in their bands (n = 2 → 0.25 / 0.75).
    for (row, frac) in [(0usize, 0.25f32), (1, 0.75)] {
        let t = element(&rt, &format!("{X_AXIS_KEY}-tick-{row}")).rect;
        let want = plot[0] + frac * plot[2];
        assert!(
            (t[0] + t[2] * 0.5 - want).abs() <= 2.0,
            "tick {row} centred at {want}, got {t:?}"
        );
    }

    // Y ticks: five, stacked top first (the top one is the domain max).
    let ticks: Vec<rune::ElementInfo> = (0..5)
        .map(|t| element(&rt, &format!("{Y_AXIS_KEY}-tick-{t}")))
        .collect();
    for w in ticks.windows(2) {
        assert!(w[0].rect[1] < w[1].rect[1], "ticks run top to bottom");
    }
    assert_eq!(ticks[0].text.as_deref(), Some("25"), "the top tick is the domain max");

    // Legend entries: one per series.
    let items = keys_with(&rt, LEGEND_ITEM_PREFIX);
    assert_eq!(items.len(), 2, "{items:?}");
}

#[test]
fn the_categorical_x_key_reads_the_configured_column_and_the_index_fallback() {
    // React 60: `dataKey={config.xAxis ?? '__index'}` — a configured column's
    // tick labels read that slot, the fallback renders the row index.
    let mut cfg = config(&["a", "b"]);
    cfg.x_axis = Some("label".into());
    let data = vec![
        point("Jan", 10.0, 15.0),
        point("February-long-month-xyz", 20.0, 25.0),
    ];
    let plan = line_chart_plan(&data, &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    let labels: Vec<&str> = plan.x_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, vec!["Jan", "February-long-mont…"], "the truncation arm");

    // The fallback: no xAxis → recharts reads the '__index' column the
    // transform writes (absent from these rows → the empty tick).
    let indexed = vec![
        ChartDataPoint::new()
            .with("__index", ChartValue::Number(0.0))
            .with("a", ChartValue::Number(10.0))
            .with("b", ChartValue::Number(15.0)),
        ChartDataPoint::new()
            .with("__index", ChartValue::Number(5.0))
            .with("a", ChartValue::Number(20.0))
            .with("b", ChartValue::Number(25.0)),
    ];
    let plan = line_chart_plan(
        &indexed,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.x_key, "__index");
    let labels: Vec<&str> = plan.x_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, vec!["0", "5"]);
}

#[test]
fn wide_row_sets_thin_their_x_ticks_to_the_band_grid() {
    let data: Vec<ChartDataPoint> = (0..10)
        .map(|i| point(&format!("r{i}"), i as f64, 9.0 - i as f64))
        .collect();
    let plan = line_chart_plan(
        &data,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    let rows: Vec<usize> = plan.x_ticks.iter().map(|t| t.row).collect();
    assert_eq!(rows, vec![0, 2, 4, 6, 8], "every ceil(10/8)=2nd band");
    let want: Vec<f32> = [0usize, 2, 4, 6, 8]
        .iter()
        .map(|r| (*r as f32 + 0.5) / 10.0)
        .collect();
    assert_eq!(plan.grid_xs, want);
}

// ---- Hit routing -----------------------------------------------------------------

#[test]
fn pressing_inside_a_data_point_rect_routes_its_row_index() {
    let mut rt = runtime(line_scenario(near_data()), (960.0, 640.0));

    click_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}1"));
    assert_eq!(rt.state.clicks, vec![1], "onClick → activeTooltipIndex 1");
    click_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert_eq!(rt.state.clicks, vec![1, 0]);

    // The hit cells are the observable rectangles: they tile the plot.
    let plot = element(&rt, PLOT_KEY).rect;
    let c0 = element(&rt, &format!("{HIT_CELL_PREFIX}0")).rect;
    let c1 = element(&rt, &format!("{HIT_CELL_PREFIX}1")).rect;
    assert!((c0[2] - plot[2] * 0.5).abs() <= 1.0, "{c0:?} vs {plot:?}");
    assert!((c1[0] - (c0[0] + c0[2])).abs() <= 1.0, "{c1:?} after {c0:?}");
    assert!((c0[1] - plot[1]).abs() <= 1.0 && (c0[3] - plot[3]).abs() <= 1.0);
    assert!(has_key(&rt, HIT_ZONE_KEY));
}

// ---- The dot layers (React 115-118) -----------------------------------------------

#[test]
fn hovering_a_row_mounts_the_active_dots_and_leaving_clears_them() {
    let engine = LineChartRenderer::new();
    let mut sc = line_scenario(near_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));

    // The per-point `dot` layer rides from the first frame (r: 3 discs).
    assert!(has_key(&rt, DOTS_KEY), "dot mounts with the categorical chart");
    // The `activeDot` layer only mounts over the hovered row.
    assert!(!has_key(&rt, ACTIVE_DOTS_KEY));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert!(has_key(&rt, ACTIVE_DOTS_KEY), "activeDot mounts over row 0");
    assert_eq!(engine.hovered_row(), Some(0));

    // Both layers share the plot rect (the discs are view-box fractions).
    let plot = element(&rt, PLOT_KEY).rect;
    for key in [DOTS_KEY, ACTIVE_DOTS_KEY] {
        let r = element(&rt, key).rect;
        for i in 0..4 {
            assert!((r[i] - plot[i]).abs() <= 0.5, "{key} dim {i}: {r:?} vs {plot:?}");
        }
    }

    // Leaving the chart clears the active layer (recharts' tooltip
    // deactivates); the per-point layer stays — it is not hover state.
    rt.pointer_leave();
    rt.frame(1.0 / 60.0);
    assert!(!has_key(&rt, ACTIVE_DOTS_KEY));
    assert!(has_key(&rt, DOTS_KEY), "the dot layer is not hover state");
    assert_eq!(engine.hovered_row(), None);
}

#[test]
fn hiding_a_series_drops_its_discs_but_keeps_the_layer_mounted() {
    let engine = LineChartRenderer::new();
    let mut sc = line_scenario(near_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));

    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert!(has_key(&rt, DOTS_KEY), "the layer stays for series b");
    assert!(!has_key(&rt, "line-chart-series-a"));
    // The plan is the observable channel for which series still paints.
    let plan = line_chart_plan(
        &near_data(),
        &config(&["a", "b"]),
        &engine.hidden_series(),
        true,
        &FormatSeams::default(),
    );
    assert!(plan.series[0].hidden && !plan.series[1].hidden);
    assert_eq!(plan.series[0].points, vec![Some([0.25, 0.6]), Some([0.75, 0.2])]);
}

// ---- The tooltip mounts -----------------------------------------------------------

#[test]
fn hovering_a_row_mounts_the_plain_tooltip_and_the_active_dots() {
    let mut sc = line_scenario(near_data());
    sc.engine = LineChartRenderer::new().format_compact(Rc::new(|v: f64| format!("c:{v}")));
    let mut rt = runtime(sc, (960.0, 640.0));

    assert!(!has_key(&rt, TOOLTIP_KEY));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert!(has_key(&rt, TOOLTIP_KEY), "the plain contentStyle box");
    assert_eq!(
        element(&rt, "line-chart-tooltip-item-0").text.as_deref(),
        Some("a : 10")
    );
    assert_eq!(
        element(&rt, "line-chart-tooltip-item-1").text.as_deref(),
        Some("b : 15")
    );
    // activeDot (r: 5) at the hovered row, per series.
    assert!(has_key(&rt, ACTIVE_DOTS_KEY));

    rt.pointer_leave();
    rt.frame(1.0 / 60.0);
    assert!(!has_key(&rt, TOOLTIP_KEY));
    assert!(!has_key(&rt, ACTIVE_DOTS_KEY));
}

#[test]
fn hovering_a_row_mounts_the_log_scale_tooltip_and_leaving_clears_it() {
    let compact = Rc::new(RefCell::new(Vec::new()));
    let mut sc = line_scenario(log_spanning_data());
    sc.seams = Some(recording_seams(compact));
    let mut rt = runtime(sc, (960.0, 640.0));

    assert!(!has_key(&rt, CONTENT_KEY), "no tooltip before the pointer arrives");
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}1"));
    assert!(has_key(&rt, CONTENT_KEY), "T136's box is mounted");

    // The formatter arms: the __orig__ slot read back through the seam.
    assert_eq!(
        element(&rt, "chart-log-scale-tooltip-item-0").text.as_deref(),
        Some("a : c:20")
    );
    assert_eq!(
        element(&rt, "chart-log-scale-tooltip-item-1").text.as_deref(),
        Some("b : c:2000")
    );

    // The tooltip lives inside the hovered bucket.
    let cell = element(&rt, &format!("{HIT_CELL_PREFIX}1")).rect;
    let tip = element(&rt, CONTENT_KEY).rect;
    assert!(
        tip[0] >= cell[0] - 0.5 && tip[0] <= cell[0] + cell[2] + 0.5,
        "anchored at the bucket: {tip:?} in {cell:?}"
    );

    // Leaving the chart clears it (recharts' tooltip deactivates).
    rt.pointer_leave();
    rt.frame(1.0 / 60.0);
    assert!(!has_key(&rt, CONTENT_KEY));
}

#[test]
fn hiding_a_series_keeps_it_out_of_the_tooltip_and_keeps_the_domain_spanning_it() {
    // React 113 (`hide={hiddenSeries.has(yKey)}`): recharts drops the hidden
    // line from the tooltip payload — and nothing recomputes the Y domain, so
    // the hidden series' max still spans it.
    let mut hidden = BTreeSet::new();
    hidden.insert("b".to_string());
    let plan = line_chart_plan(
        &near_data(),
        &config(&["a", "b"]),
        &hidden,
        true,
        &FormatSeams::default(),
    );
    assert_eq!(
        plan.y_domain,
        [0.0, 25.0],
        "b holds the max and still spans the domain (a alone would be [0, 20])"
    );
    assert!(!plan.series[0].hidden && plan.series[1].hidden);

    // Through the tree: hide beta, hover — the payload carries alpha alone.
    let engine = LineChartRenderer::new();
    let mut sc = line_scenario(near_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));
    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}b"));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert!(has_key(&rt, TOOLTIP_KEY));
    assert_eq!(
        element(&rt, "line-chart-tooltip-item-0").text.as_deref(),
        Some("a : 10")
    );
    assert!(
        !has_key(&rt, "line-chart-tooltip-item-1"),
        "the hidden series is not in the payload"
    );
}

// ---- The LabelList (React 120-134) --------------------------------------------------

#[test]
fn the_label_list_shows_the_value_display_and_the_log_round_trip() {
    // React 120-134 (`config.showValues` gates the LabelList; the formatter is
    // `logTickFormatter` under the log hint, `undefined` otherwise).
    let compact = Rc::new(RefCell::new(Vec::new()));
    let seams = recording_seams(compact.clone());

    // The plain arm: recharts' default label display of the slot.
    let mut cfg = config(&["a", "b"]);
    cfg.chart_type = ChartType::Line;
    cfg.show_values = true;
    let plan = line_chart_plan(&near_data(), &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(
        plan.series[0].labels,
        vec![Some("10".into()), Some("20".into())],
        "String(n) display"
    );
    assert_eq!(
        plan.series[1].labels,
        vec![Some("15".into()), Some("25".into())]
    );

    // The log arm: formatCompact(10^mapped v) — the same round trip the YAxis
    // tick formatter makes. r0's a is 10 → log10(10) = 1 → 10^1 = 10.
    let mut log_cfg = config(&["a", "b"]);
    log_cfg.show_values = true;
    let plan = line_chart_plan(
        &log_spanning_data(),
        &log_cfg,
        &BTreeSet::new(),
        true,
        &seams,
    );
    // The mapped value round-trips through 10^v exactly as f64 computes it
    // (JS `Math.pow(10, log10(20))` is 20.000000000000004 too).
    let want_r1 = format!("c:{}", 10.0_f64.powf(20.0_f64.log10()));
    assert_eq!(
        plan.series[0].labels,
        vec![Some("c:10".into()), Some(want_r1)],
        "logTickFormatter over the mapped slots"
    );
    let seen = compact.borrow().clone();
    assert!(
        seen.iter().any(|v| (v - 10.0).abs() < 1e-9),
        "the label round-tripped 10^log10(10), saw {seen:?}"
    );

    // A numeric-parsing text slot keeps its string (the display arm reads
    // the slot itself, not the parsed number).
    let text_data = vec![
        ChartDataPoint::new()
            .with("a", ChartValue::Text("10".into()))
            .with("b", ChartValue::Number(15.0)),
    ];
    let plan = line_chart_plan(&text_data, &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(plan.series[0].labels, vec![Some("10".into())]);
    // A non-parsing text is toNumber's null: a gap with no label at all.
    let nan_text = vec![
        ChartDataPoint::new()
            .with("a", ChartValue::Text("n/a".into()))
            .with("b", ChartValue::Number(15.0)),
    ];
    let plan = line_chart_plan(&nan_text, &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(plan.series[0].labels, vec![None]);

    // showValues off → no labels at all.
    let plan = line_chart_plan(
        &near_data(),
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert!(plan.series.iter().all(|s| s.labels.iter().all(|l| l.is_none())));
}

#[test]
fn the_label_list_overlay_mounts_bands_and_labels_and_hides_with_the_series() {
    // The tree side: `config.showValues` mounts the overlay, one band column
    // per row, one label per series stacked at the band's top.
    let mut sc = line_scenario(near_data());
    sc.config.show_values = true;
    let rt = runtime(sc, (960.0, 640.0));

    assert!(has_key(&rt, VALUES_KEY), "the LabelList overlay is mounted");
    assert_eq!(keys_with(&rt, VALUES_BAND_PREFIX).len(), 2, "one band per row");
    assert_eq!(
        element(&rt, "line-chart-value-0-a").text.as_deref(),
        Some("10")
    );
    assert_eq!(
        element(&rt, "line-chart-value-1-b").text.as_deref(),
        Some("25")
    );

    // The labels ride their band's top: same x slot as the band column.
    let plot = element(&rt, PLOT_KEY).rect;
    let band0 = element(&rt, "line-chart-values-band-0").rect;
    assert!((band0[2] - plot[2] * 0.5).abs() <= 1.0, "{band0:?} vs {plot:?}");
    let value0 = element(&rt, "line-chart-value-0-a").rect;
    assert!(
        (value0[0] + value0[2] * 0.5 - (band0[0] + band0[2] * 0.5)).abs() <= 2.0,
        "centred over the point's band: {value0:?} in {band0:?}"
    );

    // showValues off → no overlay.
    let rt_off = runtime(line_scenario(near_data()), (960.0, 640.0));
    assert!(!has_key(&rt_off, VALUES_KEY));

    // A hidden series keeps its band slot but loses its labels.
    let engine = LineChartRenderer::new();
    let mut sc_hidden = line_scenario(near_data());
    sc_hidden.config.show_values = true;
    sc_hidden.engine = engine.clone();
    let mut rt2 = runtime(sc_hidden, (960.0, 640.0));
    click_key_center(&mut rt2, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert!(has_key(&rt2, VALUES_KEY));
    assert!(
        !has_key(&rt2, "line-chart-value-0-a"),
        "the hidden series' labels are gone"
    );
    assert!(
        has_key(&rt2, "line-chart-value-0-b"),
        "the visible series keeps its labels"
    );
    assert_eq!(keys_with(&rt2, VALUES_BAND_PREFIX).len(), 2, "the bands stay");
}

// ---- The overlap dash map ---------------------------------------------------------

#[test]
fn the_dash_map_wires_the_pattern_into_the_line_plan() {
    // The plan is the observable channel: identical series cycle from solid.
    let same = vec![point("r0", 5.0, 5.0), point("r1", 5.0, 5.0)];
    let plan = line_chart_plan(
        &same,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert!(plan.series[0].dash.is_none(), "the first stays solid");
    assert_eq!(plan.series[1].dash, Some(&[8.0_f32, 4.0][..]), "the second '8 4'");

    // Distant series stay solid.
    let far = vec![point("r0", 10.0, 80.0), point("r1", 20.0, 90.0)];
    let plan = line_chart_plan(
        &far,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert!(plan.series.iter().all(|s| s.dash.is_none()));

    // A single series never builds the map (React gates it on multiSeries).
    let plan = line_chart_plan(
        &far,
        &config(&["a"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert!(plan.series.iter().all(|s| s.dash.is_none()));
}

// ---- The empty chart --------------------------------------------------------------

#[test]
fn an_empty_chart_keeps_an_honest_frame() {
    // React renders the empty <LineChart> frame: axes and (multiSeries) the
    // legend, no rows. The lean domain is an honest [0, 1] and nothing gets
    // dashed by an undefined range.
    let cfg = config(&["a", "b"]);
    let plan = line_chart_plan(&[], &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(plan.row_count, 0);
    assert_eq!(plan.y_domain, [0.0, 1.0]);
    assert!(plan.x_ticks.is_empty() && plan.grid_xs.is_empty());
    assert_eq!(plan.y_ticks.len(), 5);
    assert!(
        plan.series.iter().all(|s| s.dash.is_none()),
        "no data → no overlap → nothing dashed"
    );
    assert_eq!(plan.series[0].points, Vec::<Option<[f32; 2]>>::new());
    assert!(plan.series[0].labels.is_empty());

    // The frame mounts: axes, the legend (multiSeries), no hit cells, and
    // the dot layer still mounts (an empty canvas — `dot` is not row-gated).
    let rt = runtime(line_scenario(Vec::new()), (960.0, 640.0));
    assert!(has_key(&rt, ROOT_KEY));
    assert!(has_key(&rt, Y_AXIS_KEY) && has_key(&rt, X_AXIS_KEY));
    assert!(has_key(&rt, DOTS_KEY), "dot is a layer, not a row");
    let cells: Vec<String> = keys_with(&rt, HIT_CELL_PREFIX)
        .into_iter()
        .filter(|k| k != HIT_ZONE_KEY)
        .collect();
    assert!(cells.is_empty(), "no rows → no hit cells: {cells:?}");
    assert_eq!(keys_with(&rt, LEGEND_ITEM_PREFIX).len(), 2);
}

// ---- The Y domain's kinds ----------------------------------------------------------

#[test]
fn negative_mixed_and_flat_data_get_their_own_domain_kinds() {
    // The lean Y domain (module docs, React's recharts default domain):
    // non-positive data → [min, 0], across zero → [min, max], flat → widened
    // by one so the line does not collapse onto the axis.
    let negatives = vec![point("r0", -5.0, -4.0), point("r1", -2.0, -1.0)];
    let plan = line_chart_plan(
        &negatives,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [-5.0, 0.0]);
    assert!((plan.y_ticks[0].value - 0.0).abs() < 1e-12, "the top tick is 0");
    assert!((plan.y_ticks[4].value - (-5.0)).abs() < 1e-12);

    let mixed = vec![point("r0", -2.0, 3.0)];
    let plan = line_chart_plan(
        &mixed,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [-2.0, 3.0]);

    let flat = vec![point("r0", 5.0, 5.0)];
    let plan = line_chart_plan(
        &flat,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [5.0, 6.0], "flat data widens by one");
    assert!((plan.y_ticks[0].value - 6.0).abs() < 1e-12);
}

// ---- The legend and the recharts defaults -------------------------------------------

#[test]
fn the_legend_reads_the_series_and_the_axes_take_the_recharts_defaults() {
    let engine = LineChartRenderer::new();
    let mut sc = line_scenario(near_data());
    sc.engine = engine.clone();
    let rt = runtime(sc, (960.0, 640.0));

    // The swatches carry the series colour; the labels carry the name.
    let k = kit(true);
    let palette = datazen_ui::components::chart::renderers::area_chart_renderer::series_palette(k);
    let swatch_a = element(&rt, &format!("{LEGEND_SWATCH_PREFIX}a"));
    assert_eq!(swatch_a.fill, palette[0]);
    let swatch_b = element(&rt, &format!("{LEGEND_SWATCH_PREFIX}b"));
    assert_eq!(swatch_b.fill, palette[1]);
    assert_eq!(
        element(&rt, &format!("{LEGEND_LABEL_PREFIX}a")).text.as_deref(),
        Some("a")
    );
    let swatch_rect = element(&rt, &format!("{LEGEND_SWATCH_PREFIX}a")).rect;
    assert!(
        (swatch_rect[2] - 8.0).abs() <= 0.5 && (swatch_rect[3] - 8.0).abs() <= 0.5,
        "the swatch chip is 8x8: {swatch_rect:?}"
    );

    // The engine's toggle is React's insert-or-remove.
    engine.toggle_series("a");
    assert!(engine.hidden_series().contains("a"));
    engine.toggle_series("a");
    assert!(engine.hidden_series().is_empty());

    // recharts' own defaults: the 60pt YAxis column, the 30pt XAxis row,
    // the 22pt legend row.
    assert!((element(&rt, Y_AXIS_KEY).rect[2] - 60.0).abs() <= 1.0);
    assert!((element(&rt, X_AXIS_KEY).rect[3] - 30.0).abs() <= 1.0);
    assert!((element(&rt, LEGEND_KEY).rect[3] - 22.0).abs() <= 1.0);
}

#[test]
fn series_colours_come_from_the_kit_palette_in_order_and_wrap_at_five() {
    // React 19 (`getColorPalette`) + React 110 (`stroke={colors[i % colors.length]}`):
    // seven series walk the five-colour palette and wrap, in declaration order.
    let mut row = ChartDataPoint::new();
    for (i, c) in "abcdefg".chars().enumerate() {
        row = row.with(c.to_string(), ChartValue::Number(i as f64 + 1.0));
    }
    let data = vec![row];
    let plan = line_chart_plan(
        &data,
        &config(&["a", "b", "c", "d", "e", "f", "g"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    let k = kit(true);
    let want = datazen_ui::components::chart::renderers::area_chart_renderer::series_palette(k);
    for (i, series) in plan.series.iter().enumerate() {
        assert_eq!(series.color, want[i % want.len()], "series {i} wraps the palette");
    }
}

// ---- The plain tooltip's content arms -----------------------------------------------

#[test]
fn the_plain_tooltip_shows_js_string_display_for_every_value_kind() {
    // The value arms of recharts' default tooltip content: JS `String(n)`
    // display — integers plain, fractions shortest-round-trip, the edge
    // spellings verbatim.
    let data = vec![ChartDataPoint::new()
        .with("a", ChartValue::Number(0.5))
        .with("b", ChartValue::Number(-1.25))];
    let mut cfg = config(&["a", "b"]);
    cfg.chart_type = ChartType::Line;
    let engine = LineChartRenderer::new().format_compact(Rc::new(|v: f64| format!("c:{v}")));
    let mut sc = scenario(cfg, data);
    sc.engine = engine;
    let mut rt = runtime(sc, (960.0, 640.0));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert_eq!(
        element(&rt, "line-chart-tooltip-item-0").text.as_deref(),
        Some("a : 0.5")
    );
    assert_eq!(
        element(&rt, "line-chart-tooltip-item-1").text.as_deref(),
        Some("b : -1.25")
    );
}

// ---- CartesianGrid's invisible arm (React 54-58) -----------------------------------

#[test]
fn the_grid_canvas_stays_mounted_at_zero_visibility_when_show_grid_is_false() {
    // React 57 (`opacity={config.showGrid ? 0.3 : 0}`): a hidden grid is an
    // invisible one, not an unmounted one — the canvas stays in the tree.
    let mut sc = line_scenario(near_data());
    sc.config.show_grid = false;
    let rt = runtime(sc, (960.0, 640.0));
    assert!(
        has_key(&rt, GRID_KEY),
        "React 57: opacity 0, the CartesianGrid stays mounted"
    );
}

// ---- The default seams -------------------------------------------------------------

#[test]
fn the_default_seams_keep_reacts_compact_and_axis_tick_arms() {
    // The lean slices of format.ts the engine calls (React 4/16/67): the
    // B/M/K thresholds, the integer arm, the axis tick's null, truncation and
    // epoch-window arms, and the epoch clock itself.
    let seams = FormatSeams::default();
    let compact = |v: f64| (seams.format_compact)(v);
    assert_eq!(compact(1.5e9), "1.5B", "format.ts 15: abs >= 1e9");
    assert_eq!(compact(2.5e6), "2.5M", "format.ts 16: abs >= 1e6");
    assert_eq!(compact(25_000.0), "25.0K", "format.ts 17: abs >= 1e4");
    assert_eq!(compact(25.0), "25", "format.ts 18: Number.isInteger");
    assert_eq!(compact(25.5), "25.5", "format.ts 19: abs >= 1 → 1 digit");
    assert_eq!(compact(0.25), "0.25", "format.ts 19: below 1 → 2 digits");

    let axis = |v: &ChartValue| (seams.format_axis_tick)(v);
    assert_eq!(axis(&ChartValue::Null), "", "format.ts 44: null → ''");
    let long = "x".repeat(25);
    assert_eq!(
        axis(&ChartValue::Text(long)),
        format!("{}…", "x".repeat(18)),
        "format.ts 49: > 20 chars → 18 + ellipsis"
    );
    // format.ts 27-33: the 2000–2100 epoch-ms window, both ends inclusive.
    assert_eq!(axis(&ChartValue::Number(946_684_800_000.0)), "00:00:00");
    assert_eq!(axis(&ChartValue::Number(4_102_444_800_000.0)), "00:00:00");
    assert_eq!(
        axis(&ChartValue::Number(946_688_461_000.0)),
        "01:01:01",
        "2000-01-01T01:01:01Z"
    );
    assert_eq!(
        axis(&ChartValue::Number(946_684_799_999.0)),
        "946684799999",
        "below the window → formatNumber"
    );
    assert_eq!(
        axis(&ChartValue::Number(4_102_444_800_001.0)),
        "4102444800001",
        "above the window → formatNumber"
    );
    assert_eq!(axis(&ChartValue::Number(2.5)), "2.5");
    assert_eq!(axis(&ChartValue::Number(2.567)), "2.57");
}

// ---- Window fit --------------------------------------------------------------------

#[test]
fn every_part_stays_inside_the_window_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for compact in [false, true] {
                let mut sc = line_scenario(log_spanning_data());
                sc.dark = dark;
                sc.compact = compact;
                sc.height = Some(240.0);
                let rt = runtime(sc, (w, h));
                assert_in_window(
                    &rt,
                    w,
                    h,
                    &format!("line dark={dark} compact={compact} {w}x{h}"),
                );
                assert!(has_key(&rt, ROOT_KEY));
            }
        }
    }
}

// ---- Data shrinking under a hover ----------------------------------------------------

#[test]
fn data_shrinking_under_a_hover_unmounts_the_tooltip_and_the_active_dots() {
    // The hovered row index is clamped to the rows that still exist: the
    // tooltip and the active dots unmount instead of pointing past the data
    // (recharts clears its activeTooltipIndex when the row count drops) —
    // while the per-point `dot` layer stays, mounted on the rows that remain.
    let mut rt = runtime(line_scenario(near_data()), (960.0, 640.0));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}1"));
    assert!(has_key(&rt, TOOLTIP_KEY) && has_key(&rt, ACTIVE_DOTS_KEY));

    rt.update(|s: &mut ChartState, _tx| {
        s.scenario.data = vec![point("r0", 10.0, 15.0)];
    });
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
    assert!(
        !has_key(&rt, TOOLTIP_KEY),
        "the gone row unmounts the tooltip"
    );
    assert!(
        !has_key(&rt, ACTIVE_DOTS_KEY),
        "the gone row unmounts the active dots"
    );
    assert!(
        has_key(&rt, DOTS_KEY),
        "the per-point dot layer is not hover state: it re-mounts on the rows that remain"
    );
}

// ==== T216 audit round ==========================================================
//
// Every test below pins a seam the port's first 31 tests missed; each one was
// found by a probe mutation that survived the trimmed suite, and every probe
// now dies on its pin. The React positions cite
// `packages/frontend/src/components/chart/renderers/LineChartRenderer.tsx`.

// ---- The X-tick stride's cap (React 66's recharts auto-interval) ----------------

#[test]
fn nine_rows_still_thin_their_x_ticks_to_every_second_band() {
    // MAX_X_TICKS = 8: nine rows take stride ceil(9/8) = 2, the first band of
    // every stride — the count where a cap of 9 would stop thinning (the
    // probe mutation MAX_X_TICKS 8→9 survived the suite red-handed).
    let data: Vec<ChartDataPoint> = (0..9)
        .map(|i| point(&format!("r{i}"), i as f64, 8.0 - i as f64))
        .collect();
    let plan = line_chart_plan(
        &data,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    let rows: Vec<usize> = plan.x_ticks.iter().map(|t| t.row).collect();
    assert_eq!(rows, vec![0, 2, 4, 6, 8]);
}

// ---- The tooltip's anchor (the module docs' coarser-anchor deviation) -----------

#[test]
fn the_plain_tooltip_anchors_at_the_bucket_with_the_declared_inset() {
    // The floating content anchors at the hovered bucket's top-left with the
    // engine's declared 2pt/8pt inset (TOOLTIP_DX/TOOLTIP_DY — the probe
    // mutation DY 8→0 survived the suite: the bucket-containment check alone
    // could not see the vertical inset).
    let mut rt = runtime(line_scenario(near_data()), (960.0, 640.0));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    let cell = element(&rt, &format!("{HIT_CELL_PREFIX}0")).rect;
    let tip = element(&rt, TOOLTIP_KEY).rect;
    assert!(
        (tip[0] - (cell[0] + 2.0)).abs() <= 0.5,
        "the 2pt horizontal inset: {tip:?} in {cell:?}"
    );
    assert!(
        (tip[1] - (cell[1] + 8.0)).abs() <= 0.5,
        "the 8pt vertical inset: {tip:?} in {cell:?}"
    );
}

// ---- The LabelList's position="top" anchor (React 121) ---------------------------

#[test]
fn the_value_labels_ride_the_bands_top_stacked_in_series_order() {
    // `position="top"` mounts the labels at the top of their band column,
    // stacked per series in declaration order (the probe mutation
    // justify_start→justify_end survived the suite: the horizontal-centring
    // check could not see which end of the band column the stack rode).
    let mut sc = line_scenario(near_data());
    sc.config.show_values = true;
    let rt = runtime(sc, (960.0, 640.0));

    let plot = element(&rt, PLOT_KEY).rect;
    let value_a = element(&rt, "line-chart-value-0-a").rect;
    let value_b = element(&rt, "line-chart-value-0-b").rect;
    assert!(
        (value_a[1] - plot[1]).abs() <= 2.0,
        "series a's label rides the plot's top: {value_a:?} vs {plot:?}"
    );
    assert!(
        value_b[1] > value_a[1],
        "series b's label stacks below series a's (declaration order): {value_b:?} vs {value_a:?}"
    );
    assert!(
        value_b[1] - value_a[1] < value_a[3] + 2.0,
        "the stack is one label tall per series: {value_b:?} vs {value_a:?}"
    );

    // The equivalents documented instead of pinned (the canvas interior is
    // not tree-observable — T166's class): DOT_R/ACTIVE_DOT_R/STROKE_W and
    // the grid's 0.3 alpha live in the view-box shape ops; the dot layers'
    // hidden-series filter and the line's present-run cut read the pinned
    // plan, and the legend gap literals match the area engine's own pin set.
}
