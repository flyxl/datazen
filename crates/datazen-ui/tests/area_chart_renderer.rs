//! T166: the area chart engine — factory dispatch, the log-hint pipeline,
//! plot geometry, the hidden-series toggle, hit-bucket routing and the two
//! tooltip mounts.
//!
//! Tree observations come from `rt.elements()` (runtime-measured
//! rects/text/opacity), pressed and hovered through the runtime's own
//! pointer pipeline; the pure pipeline (`area_chart_plan` and its lean
//! `lib/chart` slices) is asserted directly, where the exact geometry lives.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::chart::renderers::area_chart_renderer::{
    area_chart_plan, build_overlap_dash_map, compute_log_scale_hint, dashed_segments,
    map_to_log_scale, renderer_factory, AreaChartRenderer, FormatSeams, AXES_KEY, DOTS_KEY,
    GRID_KEY, HIT_CELL_PREFIX, HIT_ZONE_KEY, LEGEND_ITEM_PREFIX, LEGEND_LABEL_PREFIX,
    LEGEND_SWATCH_PREFIX, LEGEND_KEY, PLOT_KEY, ROOT_KEY, SERIES_KEY_PREFIX, TOOLTIP_KEY,
    X_AXIS_KEY, Y_AXIS_KEY,
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
    engine: AreaChartRenderer,
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
            engine: AreaChartRenderer::new(),
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

fn area_view(s: &ChartState, _cx: &mut Cx<ChartState>) -> El {
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
                ChartType::Area => Box::new(engine_for_factory.clone()),
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
        area_view,
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

fn area_scenario(data: Vec<ChartDataPoint>) -> Scenario {
    let mut config = config(&["a", "b"]);
    config.chart_type = ChartType::Area;
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
fn the_factory_routes_area_to_the_engine_and_other_types_to_placeholders() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut sc = area_scenario(log_spanning_data());
    sc.seen = Some(seen.clone());
    let rt = runtime(sc, (960.0, 640.0));

    assert!(has_key(&rt, ROOT_KEY), "the engine's frame is mounted");
    assert!(
        !has_key(&rt, renderer_note_key(ChartType::Area)),
        "the Area placeholder note is gone"
    );
    assert!(
        seen.borrow().iter().all(|t| *t == ChartType::Area),
        "every build selected the area engine: {:?}",
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
    assert!(!has_key(&rt_bar, ROOT_KEY), "no area frame for Bar");
}

// ---- The log-hint pipeline ------------------------------------------------------

#[test]
fn the_log_hint_maps_series_that_span_magnitudes_into_log10_space() {
    let compact = Rc::new(RefCell::new(Vec::new()));
    let seams = recording_seams(compact.clone());
    let data = log_spanning_data();
    let cfg = config(&["a", "b"]);
    let plan = area_chart_plan(&data, &cfg, &BTreeSet::new(), true, &seams);

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
    assert_eq!(
        a.points,
        vec![
            Some([0.25, (1.0 - 1.0 / span) as f32]),
            Some([0.75, (1.0 - 20.0_f64.log10() / span) as f32]),
        ]
    );
}

#[test]
fn the_log_hint_leaves_one_magnitude_data_untouched() {
    let compact = Rc::new(RefCell::new(Vec::new()));
    let seams = recording_seams(compact.clone());
    let data = near_data();
    let cfg = config(&["a", "b"]);
    let plan = area_chart_plan(&data, &cfg, &BTreeSet::new(), true, &seams);

    assert!(!plan.log_hint.use_log);
    assert_eq!(plan.log_hint.domain_min, 1.0);
    assert_eq!(plan.chart_data, data, "no __orig__ slots, no rewriting");
    assert_eq!(plan.y_domain, [0.0, 25.0]);

    // YAxis tickFormatter = formatCompact(v): the raw tick values.
    assert_eq!(plan.y_ticks[0].label, "c:25");
    assert_eq!(plan.y_ticks[4].label, "c:0");
    let seen = compact.borrow();
    assert!(seen.iter().any(|v| *v == 25.0), "{seen:?}");
    assert!(seen.iter().any(|v| *v == 0.0), "{seen:?}");

    // Band positions (i + 0.5)/n; y from the [0, 25] domain.
    assert_eq!(
        plan.series[0].points,
        vec![
            Some([0.25, (1.0 - 10.0 / 25.0) as f32]),
            Some([0.75, (1.0 - 20.0 / 25.0) as f32])
        ]
    );
    assert_eq!(
        plan.series[1].points,
        vec![
            Some([0.25, (1.0 - 15.0 / 25.0) as f32]),
            Some([0.75, (1.0 - 25.0 / 25.0) as f32])
        ]
    );

    // The hint's other arms: a single series, and any negative value.
    assert!(!compute_log_scale_hint(&data, &["a".to_string()]).use_log);
    let negatives = vec![point("r0", -5.0, 2.0), point("r1", -4.0, 3.0)];
    assert!(!compute_log_scale_hint(&negatives, &["a".to_string(), "b".to_string()]).use_log);

    // The grid hairlines sit at the tick fractions.
    let want: Vec<f32> = (0..5).map(|t| t as f32 / 4.0).collect();
    assert_eq!(plan.grid_ys, want);
    assert_eq!(plan.grid_xs, vec![0.25, 0.75]);
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
    let hint = compute_log_scale_hint(&data, &series);
    assert!(hint.use_log, "string numbers count: 2000/10 >= 10");
    assert_eq!(hint.domain_min, 10.0);

    let mapped = map_to_log_scale(&data, &series, hint.domain_min);
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
    let hint = compute_log_scale_hint(&nan_text, &series);
    assert!(!hint.use_log, "the NaN string never counted as a value");
    let mapped = map_to_log_scale(&nan_text, &series, 10.0);
    assert_eq!(mapped[0].get("a"), Some(&ChartValue::Text("n/a".into())));
    assert_eq!(mapped[0].get("__orig__a"), None, "v == null → continue");
    assert_eq!(mapped[0].get("__orig__b"), Some(&ChartValue::Number(1000.0)));
}

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
    let plan = area_chart_plan(&data, &cfg, &BTreeSet::new(), true, &seams);

    // X positions come from the epoch value scale, not the band index.
    assert_eq!(plan.series[0].points, vec![Some([0.0, 0.75]), Some([0.75, 0.25])]);
    // Ticks come from config.timeTicks through the epoch seam.
    let labels: Vec<&str> = plan.x_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, vec!["e:0", "e:50"]);
    assert_eq!(plan.grid_xs, vec![0.0, 0.5]);
    let seen = epoch_seen.borrow();
    assert_eq!(*seen, vec![0.0, 50.0]);
}

// ---- The hidden-series toggle ---------------------------------------------------

#[test]
fn a_legend_press_hides_and_restores_a_series() {
    let engine = AreaChartRenderer::new();
    let mut sc = area_scenario(near_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));

    assert!(has_key(&rt, "area-chart-series-a") && has_key(&rt, "area-chart-series-b"));
    assert!(has_key(&rt, LEGEND_KEY), "multiSeries mounts the legend");

    // Press the alpha legend entry → its area disappears, beta stays.
    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert!(engine.hidden_series().contains("a"), "engine owns the set");
    assert!(!has_key(&rt, "area-chart-series-a"), "the hidden area is gone");
    assert!(has_key(&rt, "area-chart-series-b"), "the other series stays");
    // React's formatter dims the hidden entry's name to 0.35.
    assert!(
        (element(&rt, &format!("{LEGEND_LABEL_PREFIX}a")).opacity - 0.35).abs() < 1e-3,
        "the hidden label dims"
    );
    assert_eq!(element(&rt, &format!("{LEGEND_LABEL_PREFIX}b")).opacity, 1.0);

    // Press again → restored.
    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert!(has_key(&rt, "area-chart-series-a"));
    assert!(engine.hidden_series().is_empty());
    assert_eq!(element(&rt, &format!("{LEGEND_LABEL_PREFIX}a")).opacity, 1.0);
}

#[test]
fn a_single_series_mounts_no_legend() {
    let mut cfg = config(&["a"]);
    cfg.chart_type = ChartType::Area;
    let rt = runtime(scenario(cfg, near_data()), (960.0, 640.0));
    assert!(!has_key(&rt, LEGEND_KEY), "React gates the legend on multiSeries");
    assert!(has_key(&rt, "area-chart-series-a"));
}

// ---- Geometry --------------------------------------------------------------------

#[test]
fn the_area_grid_and_axes_canvases_fill_the_plot_rect() {
    let rt = runtime(area_scenario(near_data()), (960.0, 640.0));
    let plot = element(&rt, PLOT_KEY).rect;
    assert!(
        plot[2] > 200.0 && plot[3] > 200.0,
        "a real plot region, got {plot:?}"
    );

    for key in [GRID_KEY, AXES_KEY, "area-chart-series-a", "area-chart-series-b"] {
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
fn wide_row_sets_thin_their_x_ticks_to_the_band_grid() {
    let data: Vec<ChartDataPoint> = (0..10)
        .map(|i| point(&format!("r{i}"), i as f64, 9.0 - i as f64))
        .collect();
    let compact = Rc::new(RefCell::new(Vec::new()));
    let seams = recording_seams(compact);
    let plan = area_chart_plan(&data, &config(&["a", "b"]), &BTreeSet::new(), true, &seams);
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
    let mut rt = runtime(area_scenario(near_data()), (960.0, 640.0));

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

// ---- The tooltip mounts -----------------------------------------------------------

#[test]
fn hovering_a_row_mounts_the_log_scale_tooltip_and_leaving_clears_it() {
    let compact = Rc::new(RefCell::new(Vec::new()));
    let mut sc = area_scenario(log_spanning_data());
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
fn hovering_a_row_mounts_the_plain_tooltip_and_the_active_dots() {
    let mut sc = area_scenario(near_data());
    sc.engine = AreaChartRenderer::new().format_compact(Rc::new(|v: f64| format!("c:{v}")));
    let mut rt = runtime(sc, (960.0, 640.0));

    assert!(!has_key(&rt, "area-chart-tooltip"));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert!(has_key(&rt, "area-chart-tooltip"), "the plain contentStyle box");
    assert_eq!(
        element(&rt, "area-chart-tooltip-item-0").text.as_deref(),
        Some("a : 10")
    );
    assert_eq!(
        element(&rt, "area-chart-tooltip-item-1").text.as_deref(),
        Some("b : 15")
    );
    // activeDot (r: 5) at the hovered row, per series.
    assert!(has_key(&rt, "area-chart-active-dots"));

    rt.pointer_leave();
    rt.frame(1.0 / 60.0);
    assert!(!has_key(&rt, "area-chart-tooltip"));
    assert!(!has_key(&rt, "area-chart-active-dots"));
}

// ---- The overlap dash map ---------------------------------------------------------

#[test]
fn overlapping_series_get_dashes_and_distant_ones_stay_solid() {
    // Identical series: the range-0 arm cycles every series from solid.
    let same = vec![point("r0", 5.0, 5.0), point("r1", 5.0, 5.0)];
    let m = build_overlap_dash_map(&same, &["a".to_string(), "b".to_string()]);
    assert_eq!(m.get("a").and_then(|d| *d), None, "the first stays solid");
    assert_eq!(
        m.get("b").and_then(|d| *d),
        Some(&[8.0_f32, 4.0][..]),
        "the second gets '8 4'"
    );

    // Nearly identical (diff under 5% of the range): dashed the same way.
    let near = vec![point("r0", 10.0, 10.4), point("r1", 20.0, 20.4)];
    let m = build_overlap_dash_map(&near, &["a".to_string(), "b".to_string()]);
    assert_eq!(m.get("b").and_then(|d| *d), Some(&[8.0_f32, 4.0][..]));

    // Far apart: both solid.
    let far = vec![point("r0", 10.0, 80.0), point("r1", 20.0, 90.0)];
    let m = build_overlap_dash_map(&far, &["a".to_string(), "b".to_string()]);
    assert_eq!(m.get("a").and_then(|d| *d), None);
    assert_eq!(m.get("b").and_then(|d| *d), None);

    // A single series never overlaps (React gates the map on multiSeries).
    assert!(build_overlap_dash_map(&far, &["a".to_string()]).values().all(|d| d.is_none()));
}

#[test]
fn dash_patterns_cut_a_polyline_into_on_runs() {
    let line = vec![[0.0, 0.0], [10.0, 0.0]];
    assert_eq!(
        dashed_segments(&line, &[2.0, 2.0]),
        vec![
            vec![[0.0, 0.0], [2.0, 0.0]],
            vec![[4.0, 0.0], [6.0, 0.0]],
            vec![[8.0, 0.0], [10.0, 0.0]]
        ]
    );
    assert_eq!(
        dashed_segments(&line, &[8.0, 4.0]),
        vec![vec![[0.0, 0.0], [8.0, 0.0]]]
    );

    // A pattern that spans a vertex keeps running through it.
    let elbow = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0]];
    assert_eq!(
        dashed_segments(&elbow, &[2.0, 2.0]),
        vec![
            vec![[0.0, 0.0], [2.0, 0.0]],
            vec![[4.0, 0.0], [4.0, 2.0]]
        ]
    );
}

// ---- Window fit --------------------------------------------------------------------

#[test]
fn every_part_stays_inside_the_window_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for compact in [false, true] {
                let mut sc = area_scenario(log_spanning_data());
                sc.dark = dark;
                sc.compact = compact;
                sc.height = Some(240.0);
                let rt = runtime(sc, (w, h));
                assert_in_window(
                    &rt,
                    w,
                    h,
                    &format!("area dark={dark} compact={compact} {w}x{h}"),
                );
                assert!(has_key(&rt, ROOT_KEY));
            }
        }
    }
}

// ==== T166 audit round ==========================================================
//
// Every test below pins a seam the port's first 15 tests missed; each one was
// found by a probe mutation that survived the trimmed suite, and every probe
// dies on its pin (see the audit ledger). The React positions cite
// `packages/frontend/src/components/chart/renderers/AreaChartRenderer.tsx`
// (or the lean `lib/chart` slice's own file) line by line.

// ---- Series colours (React 19, 110-111) ------------------------------------------

#[test]
fn series_colours_come_from_the_kit_palette_in_order_and_wrap_at_five() {
    // React 19 (`getColorPalette`) + React 110/111
    // (`stroke/fill={colors[i % colors.length]}`): seven series walk the
    // five-colour palette and wrap, in declaration order.
    let mut row = ChartDataPoint::new();
    for (i, c) in "abcdefg".chars().enumerate() {
        row = row.with(c.to_string(), ChartValue::Number(i as f64 + 1.0));
    }
    let data = vec![row];
    let plan = area_chart_plan(
        &data,
        &config(&["a", "b", "c", "d", "e", "f", "g"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    let k = kit(true);
    let want = [
        k.c.info.fg,
        k.c.success.fg,
        k.c.warning.fg,
        k.c.danger.fg,
        k.c.text_accent,
        k.c.info.fg,    // i % 5 wraps
        k.c.success.fg,
    ];
    for (i, (s, w)) in plan.series.iter().zip(want).enumerate() {
        assert_eq!(s.color, w, "series {i} takes palette[{i} % 5]");
    }
}

#[test]
fn the_legend_reads_the_series_and_the_axes_take_the_recharts_defaults() {
    // React 88-103 (the Legend entry: a swatch, the series name, the toggle
    // target) + React 105-121 (one <Area> per yAxes entry, colours in order)
    // + recharts' own YAxis width 60 / XAxis height 30 defaults.
    let rt = runtime(area_scenario(near_data()), (960.0, 640.0));
    let k = kit(true);

    let item = element(&rt, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert_eq!(item.role, "button", "the legend entry is a pressable");
    assert_eq!(item.label.as_deref(), Some("a"));
    assert_eq!(
        element(&rt, &format!("{LEGEND_LABEL_PREFIX}a")).text.as_deref(),
        Some("a"),
        "the entry announces the series name"
    );
    assert_eq!(
        element(&rt, &format!("{LEGEND_LABEL_PREFIX}b")).text.as_deref(),
        Some("b")
    );
    // The swatch is the series colour: colors[0] / colors[1] in order.
    assert_eq!(element(&rt, &format!("{LEGEND_SWATCH_PREFIX}a")).fill, k.c.info.fg);
    assert_eq!(
        element(&rt, &format!("{LEGEND_SWATCH_PREFIX}b")).fill,
        k.c.success.fg
    );

    // recharts' axis defaults: the Y axis column is 60pt, the X axis row 30pt.
    let y_axis = element(&rt, Y_AXIS_KEY).rect;
    assert!((y_axis[2] - 60.0).abs() <= 0.5, "{y_axis:?}");
    let x_axis = element(&rt, X_AXIS_KEY).rect;
    assert!((x_axis[3] - 30.0).abs() <= 0.5, "{x_axis:?}");
}

// ---- The dash map's source and wiring (React 21-24, 37-40, 114) ------------------

#[test]
fn the_dash_map_reads_the_log_mapped_rows_and_wires_the_pattern_into_the_plan() {
    // React 21-24 (`chartData` is `mapToLogScale`'s output) + React 37-40
    // (`buildOverlapDashMap(chartData, ...)` — the mapped rows, not the raw
    // ones) + React 114 (`strokeDasharray={dashMap[yKey]}`).
    //
    // Three series whose a/b rows overlap on the raw scale but not once
    // log10-compressed: React dashes nothing, because it measured the mapped
    // rows.
    let mapped_semantics = vec![
        ChartDataPoint::new()
            .with("a", ChartValue::Number(10.0))
            .with("b", ChartValue::Number(20.0))
            .with("c", ChartValue::Number(1000.0)),
        ChartDataPoint::new()
            .with("a", ChartValue::Number(10.0))
            .with("b", ChartValue::Number(20.0))
            .with("c", ChartValue::Number(1000.0)),
    ];
    let cfg = config(&["a", "b", "c"]);
    let plan = area_chart_plan(
        &mapped_semantics,
        &cfg,
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert!(plan.log_hint.use_log, "1000/10 = 100 spans magnitudes");
    for s in &plan.series {
        assert!(s.dash.is_none(), "on the mapped rows nothing overlaps: {:?}", s.key);
    }

    // The wiring: an overlapping pair really lands in the SeriesPlan —
    // series 1 carries STROKE_DASH_PATTERNS[1] ('8 4').
    let overlapping = vec![point("r0", 10.0, 10.4), point("r1", 20.0, 20.4)];
    let plan = area_chart_plan(
        &overlapping,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert!(plan.series[0].dash.is_none(), "the first overlapping series stays solid");
    assert_eq!(
        plan.series[1].dash,
        Some(&[8.0_f32, 4.0][..]),
        "the second gets '8 4'"
    );
}

// ---- The log hint's boundaries (transform.ts 203, 205) -----------------------------

#[test]
fn the_log_hint_uses_log_at_exactly_ten_times_and_clamps_domain_min_to_one() {
    // transform.ts 203 (`if (globalMax / minPositive < 10) return off`): a
    // spread of exactly 10 is NOT less than 10 — the log arm fires.
    let exact = vec![point("r0", 1.0, 10.0), point("r1", 1.0, 10.0)];
    let series = vec!["a".to_string(), "b".to_string()];
    let hint = compute_log_scale_hint(&exact, &series);
    assert!(hint.use_log, "10 / 1 = 10 is not < 10");
    assert_eq!(hint.domain_min, 1.0);
    let mapped = map_to_log_scale(&exact, &series, hint.domain_min);
    assert_eq!(mapped[0].get("a"), Some(&ChartValue::Number(0.0))); // log10(max(1, 1))
    assert_eq!(mapped[0].get("b"), Some(&ChartValue::Number(1.0))); // log10(10)

    // transform.ts 205 (`Math.max(1, Math.pow(10, Math.floor(Math.log10(minPositive))))`):
    // a sub-unit minPositive would give 10^-1; the max(1, ..) clamps it.
    let sub_unit = vec![point("r0", 0.5, 20.0)];
    let hint = compute_log_scale_hint(&sub_unit, &series);
    assert!(hint.use_log, "20 / 0.5 = 40 spans magnitudes");
    assert_eq!(hint.domain_min, 1.0, "max(1, 10^-1) is 1");
    let mapped = map_to_log_scale(&sub_unit, &series, hint.domain_min);
    assert_eq!(mapped[0].get("a"), Some(&ChartValue::Number(0.0))); // log10(max(0.5, 1))
    assert_eq!(mapped[0].get("__orig__a"), Some(&ChartValue::Number(0.5)));
}

// ---- The value coercions' skip arms (transform.ts 160-167, 225) ---------------------

#[test]
fn null_and_nan_slots_keep_the_transforms_skip_arms() {
    // transform.ts 160-167 (`toNumber`): null is null — the slot is skipped
    // whole (no __orig__ stash, no rewrite); transform.ts 225
    // (`Math.log10(Math.max(v, domainMin))`): a NaN number stashes NaN and
    // maps to NaN (Math.max(NaN, d) === NaN).
    let series = vec!["a".to_string(), "b".to_string()];
    let data = vec![
        ChartDataPoint::new()
            .with("a", ChartValue::Null)
            .with("b", ChartValue::Number(1.0)),
        ChartDataPoint::new()
            .with("a", ChartValue::Number(f64::NAN))
            .with("b", ChartValue::Number(2000.0)),
    ];
    let hint = compute_log_scale_hint(&data, &series);
    assert!(hint.use_log, "b spans 1..2000; the null and the NaN never count");
    let mapped = map_to_log_scale(&data, &series, hint.domain_min);

    // null: untouched, nothing stashed.
    assert_eq!(mapped[0].get("a"), Some(&ChartValue::Null));
    assert_eq!(mapped[0].get("__orig__a"), None, "v == null → continue");
    assert_eq!(mapped[0].get("__orig__b"), Some(&ChartValue::Number(1.0)));

    // NaN: the original is stashed and the slot maps to NaN.
    match mapped[1].get("a") {
        Some(ChartValue::Number(v)) => assert!(v.is_nan(), "log10(Math.max(NaN, d)) is NaN"),
        other => panic!("the NaN slot maps to NaN, got {other:?}"),
    }
    match mapped[1].get("__orig__a") {
        Some(ChartValue::Number(v)) => assert!(v.is_nan(), "the NaN original is stashed"),
        other => panic!("__orig__a is NaN, got {other:?}"),
    }
}

// ---- The X axis' key and formatter (React 60, 67) -----------------------------------

#[test]
fn the_categorical_x_key_reads_the_configured_column_and_the_index_fallback() {
    // React 60 (`dataKey={config.xAxis ?? '__index'}`) + React 67
    // (`tickFormatter={formatAxisTick}` — the non-time arm): the tick label
    // is the row's slot under the configured column, through the seam.
    let data = vec![point("r0", 10.0, 15.0), point("r1", 20.0, 25.0)];
    let mut cfg = config(&["a", "b"]);
    cfg.x_axis = Some("label".into());
    let plan = area_chart_plan(
        &data,
        &cfg,
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.x_key, "label");
    let labels: Vec<&str> = plan.x_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, vec!["r0", "r1"], "the label column feeds formatAxisTick");

    // The fallback: no xAxis → recharts reads the '__index' column the
    // transform writes.
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
    let plan = area_chart_plan(
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

// ---- The time domain's edges (React 63-67) ------------------------------------------

#[test]
fn time_domain_rows_outside_the_domain_stay_gaps_and_ticks_clamp_to_the_plot() {
    // React 65-66 (`domain={config.timeDomain}` + `ticks={config.timeTicks}`):
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
    let plan = area_chart_plan(
        &data,
        &cfg,
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );

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

// ---- activeDot's time-domain arm (React 117-119) --------------------------------------

#[test]
fn a_time_domain_chart_mounts_no_active_dots_even_while_hovered() {
    // React 117-119 (`activeDot={config.timeDomain ? false : {...}}`): the
    // dots never mount under a time domain, while the tooltip still does.
    let engine = AreaChartRenderer::new();
    let mut cfg = config(&["a", "b"]);
    cfg.chart_type = ChartType::Area;
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
        "React 118: activeDot is false under a time domain"
    );
    assert!(!has_key(&rt, CONTENT_KEY), "no log hint (4 / 1 < 10)");
    assert_eq!(engine.hovered_row(), Some(0), "the hover is retained");
}

// ---- Hidden series: payload and domain (React 115) --------------------------------------

#[test]
fn hiding_a_series_keeps_it_out_of_the_tooltip_and_keeps_the_domain_spanning_it() {
    // React 115 (`hide={hiddenSeries.has(yKey)}`): recharts drops the hidden
    // area from the tooltip payload — and nothing recomputes the Y domain, so
    // the hidden series' max still spans it.
    let mut hidden = BTreeSet::new();
    hidden.insert("b".to_string());
    let plan = area_chart_plan(
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
    let engine = AreaChartRenderer::new();
    let mut sc = area_scenario(near_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));
    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}b"));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert!(has_key(&rt, TOOLTIP_KEY));
    assert_eq!(
        element(&rt, "area-chart-tooltip-item-0").text.as_deref(),
        Some("a : 10")
    );
    assert!(
        !has_key(&rt, "area-chart-tooltip-item-1"),
        "the hidden series is not in the payload"
    );
}

// ---- The empty chart (React 42-122 with data: []) -----------------------------------------

#[test]
fn an_empty_chart_keeps_an_honest_frame() {
    // React renders the empty <AreaChart> frame: axes and (multiSeries) the
    // legend, no rows. The lean domain is an honest [0, 1] and nothing gets
    // dashed by an undefined range.
    let cfg = config(&["a", "b"]);
    let plan = area_chart_plan(
        &[],
        &cfg,
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.row_count, 0);
    assert_eq!(plan.y_domain, [0.0, 1.0]);
    assert!(plan.x_ticks.is_empty() && plan.grid_xs.is_empty());
    assert_eq!(plan.y_ticks.len(), 5);
    assert!(
        plan.series.iter().all(|s| s.dash.is_none()),
        "no data → no overlap → nothing dashed"
    );
    assert_eq!(plan.series[0].points, Vec::<Option<[f32; 2]>>::new());

    // The frame mounts: axes, the legend (multiSeries), no hit cells.
    let rt = runtime(area_scenario(Vec::new()), (960.0, 640.0));
    assert!(has_key(&rt, ROOT_KEY));
    assert!(has_key(&rt, Y_AXIS_KEY) && has_key(&rt, X_AXIS_KEY));
    let cells: Vec<String> = keys_with(&rt, HIT_CELL_PREFIX)
        .into_iter()
        .filter(|k| k != HIT_ZONE_KEY)
        .collect();
    assert!(cells.is_empty(), "no rows → no hit cells: {cells:?}");
    assert_eq!(keys_with(&rt, LEGEND_ITEM_PREFIX).len(), 2);
}

// ---- The Y domain's kinds (the module docs' recharts [0, auto] behaviour) ----------------

#[test]
fn negative_mixed_and_flat_data_get_their_own_domain_kinds() {
    // The lean Y domain (module docs, React's recharts default domain):
    // non-positive data → [min, 0], across zero → [min, max], flat → widened
    // by one so the area does not collapse onto the axis.
    let negatives = vec![point("r0", -5.0, -4.0), point("r1", -2.0, -1.0)];
    let plan = area_chart_plan(
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
    let plan = area_chart_plan(
        &mixed,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [-2.0, 3.0]);

    let flat = vec![point("r0", 5.0, 5.0)];
    let plan = area_chart_plan(
        &flat,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [5.0, 6.0], "flat data widens by one");
    assert!((plan.y_ticks[0].value - 6.0).abs() < 1e-12);
}

// ---- Data shrinking under a hover (recharts clears activeTooltipIndex) -------------------
#[test]
fn shrinking_the_data_under_a_hover_unmounts_the_tooltip_and_the_dots() {
    // The hovered row index is clamped to the rows that still exist: the
    // tooltip and the active dots unmount instead of pointing past the data
    // (recharts clears its activeTooltipIndex when the row count drops).
    let mut rt = runtime(area_scenario(near_data()), (960.0, 640.0));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}1"));
    assert!(has_key(&rt, TOOLTIP_KEY) && has_key(&rt, DOTS_KEY));

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
        !has_key(&rt, DOTS_KEY),
        "the gone row unmounts the active dots"
    );
}

// ---- The default format seams (format.ts 12-19, 27-50) ------------------------------------

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

// ---- The X-tick stride's cap (React 66's recharts auto-interval) ---------------------------

#[test]
fn nine_rows_still_thin_their_x_ticks_to_every_second_band() {
    // The lean recharts-interval cap (MAX_X_TICKS = 8): nine rows take
    // stride ceil(9/8) = 2, the first band of every stride — the count where
    // a cap of 9 would stop thinning.
    let data: Vec<ChartDataPoint> = (0..9)
        .map(|i| point(&format!("r{i}"), i as f64, 8.0 - i as f64))
        .collect();
    let plan = area_chart_plan(
        &data,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    let rows: Vec<usize> = plan.x_ticks.iter().map(|t| t.row).collect();
    assert_eq!(rows, vec![0, 2, 4, 6, 8]);
}

// ---- CartesianGrid's invisible arm (React 54-58) ---------------------------------------------

#[test]
fn the_grid_canvas_stays_mounted_at_zero_visibility_when_show_grid_is_false() {
    // React 57 (`opacity={config.showGrid ? 0.3 : 0}`): a hidden grid is an
    // invisible one, not an unmounted one — the canvas stays in the tree.
    let mut sc = area_scenario(near_data());
    sc.config.show_grid = false;
    let rt = runtime(sc, (960.0, 640.0));
    assert!(
        has_key(&rt, GRID_KEY),
        "React 57: opacity 0, the CartesianGrid stays mounted"
    );
}

// ---- seriesOverlap.ts' null coercion (T153 slice, seriesOverlap.ts 29, 79-81) ---------------

#[test]
fn an_overlap_probe_treats_null_as_zero_and_an_absent_slot_as_skipped() {
    // seriesOverlap.ts 29 / 79-81 (`Number(pt[key])` + `Number.isFinite`):
    // a null slot coerces to 0 and counts; an absent slot is undefined → NaN
    // → skipped.
    let with_null = vec![
        ChartDataPoint::new()
            .with("a", ChartValue::Null)
            .with("b", ChartValue::Number(4.6)),
        ChartDataPoint::new()
            .with("a", ChartValue::Number(5.0))
            .with("b", ChartValue::Number(5.0)),
    ];
    let m = build_overlap_dash_map(
        &with_null,
        &["a".to_string(), "b".to_string()],
    );
    assert_eq!(
        m.get("b").and_then(|d| *d),
        None,
        "the null row counts as |0 - 4.6| and breaks the overlap"
    );

    let with_absent = vec![
        ChartDataPoint::new().with("b", ChartValue::Number(4.6)),
        ChartDataPoint::new()
            .with("a", ChartValue::Number(5.0))
            .with("b", ChartValue::Number(5.0)),
    ];
    let m = build_overlap_dash_map(
        &with_absent,
        &["a".to_string(), "b".to_string()],
    );
    assert_eq!(
        m.get("b").and_then(|d| *d),
        Some(&[8.0_f32, 4.0][..]),
        "the absent row is skipped, the rest overlaps"
    );
}

// ==== T166 audit round, second pass ============================================
//
// Twelve more seams the first 16 pins missed; every one was found by a probe
// mutation that survived the whole trimmed suite (159 green with the probe
// live), and every probe dies on its pin below. React positions cite
// `packages/frontend/src/components/chart/renderers/AreaChartRenderer.tsx`
// (or the lean `lib/chart` slice's own file), line by line.

// ---- The log rewrite's spread (transform.ts 219-228) ----------------------------

#[test]
fn the_log_rewrite_keeps_every_column_the_spread_copied() {
    // transform.ts 219-228 (`const clone = { ...point }`): `mapToLogScale`
    // rewrites the series slots but the spread carries every other column —
    // so the X axis (React 60, `dataKey={config.xAxis}`) still reads the label
    // column from the *mapped* rows React 21-24 hand the chart.
    let mut cfg = config(&["a", "b"]);
    cfg.x_axis = Some("label".into());
    let plan = area_chart_plan(
        &log_spanning_data(),
        &cfg,
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert!(plan.log_hint.use_log, "10..2000 spans magnitudes");
    assert_eq!(
        plan.chart_data[0].get("label"),
        Some(&ChartValue::Text("r0".into())),
        "the label column survives the rewrite"
    );
    // And it still feeds the categorical tick formatter (React 67).
    let labels: Vec<&str> = plan.x_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, vec!["r0", "r1"]);
}

// ---- The plain pipeline's gaps (React 105-121, recharts connectNulls: false) -----

#[test]
fn null_and_nan_slots_leave_gaps_in_the_series_geometry() {
    // React 105-121: `<Area dataKey={yKey}>` over a slot recharts cannot plot
    // (null, or NaN — `toNumber` passes NaN through, `typeof NaN === 'number'`)
    // leaves a *gap*: the point is absent, the line breaks (connectNulls stays
    // false), and the domain scan skips it. Rows 1/2 hold a null and a NaN.
    let data = vec![
        point("r0", 10.0, 15.0),
        ChartDataPoint::new()
            .with("label", ChartValue::Text("r1".into()))
            .with("a", ChartValue::Null)
            .with("b", ChartValue::Number(25.0)),
        ChartDataPoint::new()
            .with("label", ChartValue::Text("r2".into()))
            .with("a", ChartValue::Number(f64::NAN))
            .with("b", ChartValue::Number(25.0)),
        point("r3", 20.0, 25.0),
    ];
    let plan = area_chart_plan(
        &data,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    // The domain scan skipped the null and the NaN: [0, 25] over a/b alone.
    assert_eq!(plan.y_domain, [0.0, 25.0]);
    // Series a: row 0 draws, rows 1/2 are gaps, row 3 draws (bands (i+0.5)/4).
    assert_eq!(
        plan.series[0].points,
        vec![
            Some([0.125, 0.6]),
            None,
            None,
            Some([0.875, 0.2]),
        ],
        "null and NaN leave gaps, not centred points"
    );
    // Series b is present everywhere.
    assert_eq!(
        plan.series[1].points,
        vec![
            Some([0.125, 0.4]),
            Some([0.375, 0.0]),
            Some([0.625, 0.0]),
            Some([0.875, 0.0]),
        ]
    );
}

// ---- The dash index counts overlapping series only (seriesOverlap.ts 60-67) ------

#[test]
fn the_dash_index_counts_only_the_overlapping_series() {
    // seriesOverlap.ts 60-67: `dashIdx` increments *inside*
    // `if (overlapping.has(key))` — a distant series sitting before an
    // overlapping pair does not consume a pattern slot: the twins get
    // PATTERNS[0] (solid) and PATTERNS[1] ('8 4'), not '8 4' and '2 4'.
    let data = vec![
        ChartDataPoint::new()
            .with("a", ChartValue::Number(0.0))
            .with("b", ChartValue::Number(100.0))
            .with("c", ChartValue::Number(100.5)),
        ChartDataPoint::new()
            .with("a", ChartValue::Number(1.0))
            .with("b", ChartValue::Number(101.0))
            .with("c", ChartValue::Number(101.5)),
    ];
    let m = build_overlap_dash_map(
        &data,
        &["a".to_string(), "b".to_string(), "c".to_string()],
    );
    assert_eq!(
        m.get("a").and_then(|d| *d),
        None,
        "the distant series never enters the overlapping set"
    );
    assert_eq!(
        m.get("b").and_then(|d| *d),
        None,
        "the first overlapping series stays solid"
    );
    assert_eq!(
        m.get("c").and_then(|d| *d),
        Some(&[8.0_f32, 4.0][..]),
        "the second overlapping series takes '8 4', not '2 4'"
    );
}

// ---- The whole pattern ladder cycles (colors.ts 39-46) ----------------------------

#[test]
fn identical_series_cycle_the_full_pattern_ladder_and_wrap() {
    // colors.ts 39-46: `STROKE_DASH_PATTERNS` is the six-entry ladder
    // [undefined, '8 4', '2 4', '8 4 2 4', '12 4', '4 4'] and the range-0 arm
    // (seriesOverlap.ts 36-44) cycles *every* series through it: seven
    // identical series walk all six and wrap to solid.
    let mut row = ChartDataPoint::new();
    for k in ["a", "b", "c", "d", "e", "f", "g"] {
        row = row.with(k, ChartValue::Number(5.0));
    }
    let keys: Vec<String> = ["a", "b", "c", "d", "e", "f", "g"]
        .iter()
        .map(|s| s.to_string())
        .collect();
    let m = build_overlap_dash_map(&[row], &keys);
    let want: Vec<Option<&'static [f32]>> = vec![
        None,
        Some(&[8.0, 4.0][..]),
        Some(&[2.0, 4.0][..]),
        Some(&[8.0, 4.0, 2.0, 4.0][..]),
        Some(&[12.0, 4.0][..]),
        Some(&[4.0, 4.0][..]),
        None, // 7 % 6 wraps to solid
    ];
    for (key, w) in keys.iter().zip(want) {
        assert_eq!(
            m.get(key).and_then(|d| *d),
            w,
            "series {key} takes ladder[{idx}]",
            idx = keys.iter().position(|k| k == key).unwrap()
        );
    }
}

// ---- The plain tooltip's value display arms (React 77-85) -------------------------

#[test]
fn the_plain_tooltip_shows_js_string_display_for_every_value_kind() {
    // React 77-85: the non-log `<Tooltip>` content is recharts' default —
    // each entry prints the slot through JS `String(v)`: null → '' (the
    // trailing empty value), NaN → 'NaN', ±Infinity → 'Infinity'/
    // '-Infinity', 0 → '0' (js_string's five arms, the T136 precedent).
    let data = vec![
        ChartDataPoint::new()
            .with("label", ChartValue::Text("r0".into()))
            .with("a", ChartValue::Number(10.0))
            .with("b", ChartValue::Number(f64::NAN)),
        ChartDataPoint::new()
            .with("label", ChartValue::Text("r1".into()))
            .with("a", ChartValue::Number(20.0))
            .with("b", ChartValue::Number(f64::INFINITY)),
        ChartDataPoint::new()
            .with("label", ChartValue::Text("r2".into()))
            .with("a", ChartValue::Null)
            .with("b", ChartValue::Number(25.0)),
        ChartDataPoint::new()
            .with("label", ChartValue::Text("r3".into()))
            .with("a", ChartValue::Number(0.0))
            .with("b", ChartValue::Number(f64::NEG_INFINITY)),
    ];
    // -Infinity keeps the hint off (transform.ts 195: v < 0 → hasNegative),
    // so the plain tooltip arm runs.
    let mut sc = area_scenario(data);
    sc.engine = AreaChartRenderer::new();
    let mut rt = runtime(sc, (960.0, 640.0));
    let item = |rt: &Runtime<ChartState>, i: usize| -> Option<String> {
        element(rt, &format!("area-chart-tooltip-item-{i}"))
            .text
            .clone()
    };
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert_eq!(item(&rt, 0).as_deref(), Some("a : 10"));
    assert_eq!(item(&rt, 1).as_deref(), Some("b : NaN"));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}1"));
    assert_eq!(item(&rt, 0).as_deref(), Some("a : 20"));
    assert_eq!(item(&rt, 1).as_deref(), Some("b : Infinity"));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}2"));
    assert_eq!(
        item(&rt, 0).as_deref(),
        Some("a : "),
        "a null slot shows the empty value, not 'null'"
    );
    assert_eq!(item(&rt, 1).as_deref(), Some("b : 25"));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}3"));
    assert_eq!(item(&rt, 0).as_deref(), Some("a : 0"));
    assert_eq!(item(&rt, 1).as_deref(), Some("b : -Infinity"));
}

// ---- The chrome inks (React 61, 70, 82, 89) ----------------------------------------

#[test]
fn ticks_take_the_secondary_ink_and_the_labels_the_strong_ink() {
    // React 61/70 (`fill: 'var(--c-fg-secondary, #999)'` on both axes' ticks)
    // + React 89 (the Legend wrapper's `color: 'var(--c-fg, #eee)'`): the
    // axis ticks read the collapsed secondary ink, the legend names the
    // strong one.
    let rt = runtime(area_scenario(near_data()), (960.0, 640.0));
    let k = kit(true);
    assert_eq!(
        element(&rt, &format!("{Y_AXIS_KEY}-tick-0")).text_color,
        Some(k.c.text_muted),
        "Y ticks: var(--c-fg-secondary)"
    );
    assert_eq!(
        element(&rt, &format!("{X_AXIS_KEY}-tick-0")).text_color,
        Some(k.c.text_muted),
        "X ticks: var(--c-fg-secondary)"
    );
    assert_eq!(
        element(&rt, &format!("{LEGEND_LABEL_PREFIX}a")).text_color,
        Some(k.c.text_strong),
        "legend names: var(--c-fg, #eee)"
    );
}

#[test]
fn the_plain_tooltip_keeps_reacts_contentstyle_chrome_and_anchor() {
    // React 77-85 (`contentStyle`: background var(--c-surface-alt), border
    // 1px var(--c-edge), borderRadius 6, color var(--c-fg, #eee)) on the
    // token mapping the module docs declare (layer_01 / border_base /
    // radius.md / text_strong) — plus the port's declared anchor: the
    // bucket's top-left inset by (2, 8).
    let mut rt = runtime(area_scenario(near_data()), (960.0, 640.0));
    let k = kit(true);
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    let tip = element(&rt, TOOLTIP_KEY);
    assert_eq!(tip.fill, k.c.layer_01, "background: var(--c-surface-alt)");
    assert_eq!(tip.radius, k.radius.md, "borderRadius: 6");
    assert_eq!(
        element(&rt, &format!("{TOOLTIP_KEY}-item-0")).text_color,
        Some(k.c.text_strong),
        "color: var(--c-fg, #eee)"
    );
    let cell = element(&rt, &format!("{HIT_CELL_PREFIX}0")).rect;
    assert!(
        (tip.rect[0] - (cell[0] + 2.0)).abs() <= 0.5,
        "anchored at bucket.x + 2: {:?} in {:?}",
        tip.rect,
        cell
    );
    assert!(
        (tip.rect[1] - (cell[1] + 8.0)).abs() <= 0.5,
        "anchored at bucket.y + 8: {:?} in {:?}",
        tip.rect,
        cell
    );
}

// ---- The X-tick cap's lower edge (recharts auto-interval, MAX_X_TICKS = 8) --------

#[test]
fn eight_rows_still_tick_every_band_before_the_cap_thins() {
    // The lean recharts-interval cap is 8: eight rows take stride
    // ceil(8/8) = 1 — every band ticks (the nine-row stride-2 pin above
    // guards the upper edge; this one guards the lower).
    let data: Vec<ChartDataPoint> = (0..8)
        .map(|i| point(&format!("r{i}"), i as f64, 7.0 - i as f64))
        .collect();
    let plan = area_chart_plan(
        &data,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    let rows: Vec<usize> = plan.x_ticks.iter().map(|t| t.row).collect();
    assert_eq!(rows, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    let want: Vec<f32> = (0..8).map(|r| (r as f32 + 0.5) / 8.0).collect();
    assert_eq!(plan.grid_xs, want);
}

// ---- The legend row's declared geometry (module docs: 22pt row, 8pt swatch) -------

#[test]
fn the_legend_row_declares_its_22pt_row_and_8pt_swatches() {
    // Not React literals (recharts sizes the legend from its content); the
    // port declares the 22pt row (module docs) and the 8pt swatch chip —
    // pinned as the layout regression guards they are.
    let rt = runtime(area_scenario(near_data()), (960.0, 640.0));
    let legend = element(&rt, LEGEND_KEY).rect;
    assert!((legend[3] - 22.0).abs() <= 0.5, "the legend row is 22pt: {legend:?}");
    let swatch = element(&rt, &format!("{LEGEND_SWATCH_PREFIX}a")).rect;
    assert!(
        (swatch[2] - 8.0).abs() <= 0.5 && (swatch[3] - 8.0).abs() <= 0.5,
        "the swatch chip is 8x8: {swatch:?}"
    );
}

// ---- The flat domain's negative arm (recharts' constant-domain extension) ----------

#[test]
fn negative_flat_data_widens_upward_not_to_zero() {
    // The lean domain's flat arm (module docs: recharts' constant-domain
    // extension `max = min + 1`) fires *before* the sign branches: a constant
    // -5 widens to [-5, -4], not to the zero floor the non-positive arm
    // would give.
    let flat = vec![point("r0", -5.0, -5.0)];
    let plan = area_chart_plan(
        &flat,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [-5.0, -4.0]);
    assert!((plan.y_ticks[0].value - (-4.0)).abs() < 1e-12, "the top tick is -4");
}

// ---- The empty time frame and the degenerate domain (React 63-67) -----------------

#[test]
fn an_empty_time_domain_chart_and_a_degenerate_domain_stay_honest() {
    // React 63-67: a time-domain chart with no rows renders the honest empty
    // frame — `ticks={config.timeTicks}` contributes nothing (no ticks to
    // snap), and a zero-width `domain` degenerates to the centre instead of
    // dividing by zero.
    let mut cfg = config(&["a", "b"]);
    cfg.x_axis = Some("t".into());
    cfg.time_domain = Some([0.0, 100.0]);
    cfg.time_ticks = vec![0.0, 50.0];
    let empty = area_chart_plan(&[], &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(empty.row_count, 0);
    assert!(
        empty.x_ticks.is_empty() && empty.grid_xs.is_empty(),
        "no rows → nothing to snap the ticks to"
    );

    // d0 == d1: every row centres (0.5), the tick centres too.
    let mut degenerate = config(&["a", "b"]);
    degenerate.x_axis = Some("t".into());
    degenerate.time_domain = Some([100.0, 100.0]);
    degenerate.time_ticks = vec![150.0];
    let data = vec![
        ChartDataPoint::new()
            .with("t", ChartValue::Number(50.0))
            .with("a", ChartValue::Number(1.0))
            .with("b", ChartValue::Number(2.0)),
        ChartDataPoint::new()
            .with("t", ChartValue::Number(90.0))
            .with("a", ChartValue::Number(3.0))
            .with("b", ChartValue::Number(4.0)),
    ];
    let plan = area_chart_plan(
        &data,
        &degenerate,
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(
        plan.series[0].points,
        vec![Some([0.5, 0.75]), Some([0.5, 0.25])],
        "a zero-width domain centres every row"
    );
    assert_eq!(plan.grid_xs, vec![0.5], "the tick centres");
}

// ---- The axis-tick truncation's exact boundary (format.ts 49) -----------------------

#[test]
fn exactly_twenty_characters_survive_the_axis_tick_seam() {
    // format.ts 49 (`str.length > 20 ? str.slice(0, 18) + '…' : str`):
    // exactly twenty characters is *not* longer than twenty — it stays
    // whole; twenty-one truncates to eighteen plus the ellipsis.
    let seams = FormatSeams::default();
    let axis = |v: &ChartValue| (seams.format_axis_tick)(v);
    let twenty = "y".repeat(20);
    assert_eq!(
        axis(&ChartValue::Text(twenty.clone())),
        twenty,
        "exactly 20 characters stays whole"
    );
    let twenty_one = "y".repeat(21);
    assert_eq!(
        axis(&ChartValue::Text(twenty_one)),
        format!("{}…", "y".repeat(18)),
        "21 characters truncates to 18 + '…'"
    );
}
