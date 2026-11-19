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

use crate::components::chart::renderers::area_chart_renderer::{
    area_chart_plan, build_overlap_dash_map, compute_log_scale_hint, dashed_segments,
    map_to_log_scale, renderer_factory, AreaChartRenderer, FormatSeams, AXES_KEY, GRID_KEY,
    HIT_CELL_PREFIX, HIT_ZONE_KEY, LEGEND_ITEM_PREFIX, LEGEND_LABEL_PREFIX, LEGEND_KEY, PLOT_KEY,
    ROOT_KEY, X_AXIS_KEY, Y_AXIS_KEY,
};
use crate::components::chart::chart_canvas::{
    renderer_note_key, ChartCanvas, ChartConfig, ChartDataPoint, ChartType, ChartValue,
    PlaceholderChartRenderer,
};
use crate::components::chart::log_scale_tooltip::CONTENT_KEY;

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
    rt.frame(0.0);
    rt.frame(0.0);
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
