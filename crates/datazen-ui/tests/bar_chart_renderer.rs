//! T192: the bar chart engine — factory dispatch, the log-hint pipeline, bar
//! rectangle geometry, the hidden-series toggle, the LabelList, hit-band
//! routing and the two tooltip mounts.
//!
//! Tree observations come from `rt.elements()` (runtime-measured
//! rects/text/opacity), pressed and hovered through the runtime's own
//! pointer pipeline; the pure pipeline (`bar_chart_plan` over the area
//! engine's shared lean slices) is asserted directly, where the exact
//! geometry lives.

use std::cell::RefCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::chart::renderers::area_chart_renderer::compute_log_scale_hint;
use datazen_ui::components::chart::renderers::bar_chart_renderer::{
    bar_chart_plan, renderer_factory, BarChartRenderer, FormatSeams, AXES_KEY, GRID_KEY,
    HIT_CELL_PREFIX, HIT_ZONE_KEY, LEGEND_ITEM_PREFIX, LEGEND_LABEL_PREFIX, LEGEND_SWATCH_PREFIX,
    LEGEND_KEY, PLOT_KEY, ROOT_KEY, SERIES_KEY_PREFIX, TOOLTIP_KEY, VALUE_PREFIX, VALUES_KEY,
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

/// A drawable rect against its wanted fractions (the fraction arithmetic runs
/// in f32, so the pins compare with slack).
fn assert_rect(actual: Option<[f32; 4]>, want: [f32; 4], label: &str) {
    let a = actual.unwrap_or_else(|| panic!("{label} is a drawable rect, got None"));
    for i in 0..4 {
        assert!(
            (a[i] - want[i]).abs() <= 1e-5,
            "{label} dim {i}: {a:?} vs {want:?}"
        );
    }
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
        ..ChartConfig::default() // chart_type defaults to Bar
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

// ---- Probe view ---------------------------------------------------------------

#[derive(Clone)]
struct Scenario {
    config: ChartConfig,
    data: Vec<ChartDataPoint>,
    dark: bool,
    compact: bool,
    height: Option<f32>,
    engine: BarChartRenderer,
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
            engine: BarChartRenderer::new(),
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

fn bar_view(s: &ChartState, _cx: &mut Cx<ChartState>) -> El {
    let sc = &s.scenario;
    let mut engine = sc.engine.clone();
    if let Some(seams) = &sc.seams {
        engine = engine
            .format_compact(Rc::clone(&seams.format_compact))
            .format_axis_tick(Rc::clone(&seams.format_axis_tick));
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
                ChartType::Bar => Box::new(engine_for_factory.clone()),
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
        bar_view,
        size,
        env(dark),
    );
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
    rt
}

fn scenario(config: ChartConfig, data: Vec<ChartDataPoint>) -> Scenario {
    Scenario {
        config,
        data,
        ..Scenario::default()
    }
}

fn bar_scenario(data: Vec<ChartDataPoint>) -> Scenario {
    scenario(config(&["a", "b"]), data)
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
fn the_factory_routes_bar_to_the_engine_and_other_types_to_placeholders() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut sc = bar_scenario(log_spanning_data());
    sc.seen = Some(seen.clone());
    let rt = runtime(sc, (960.0, 640.0));

    assert!(has_key(&rt, ROOT_KEY), "the engine's frame is mounted");
    assert!(
        !has_key(&rt, renderer_note_key(ChartType::Bar)),
        "the Bar placeholder note is gone"
    );
    assert!(
        seen.borrow().iter().all(|t| *t == ChartType::Bar),
        "every build selected the bar engine: {:?}",
        seen.borrow()
    );

    // The other four types still get their honest placeholders.
    let seen_area = Rc::new(RefCell::new(Vec::new()));
    let mut area_config = config(&["a", "b"]);
    area_config.chart_type = ChartType::Area;
    let mut sc_area = scenario(area_config, log_spanning_data());
    sc_area.seen = Some(seen_area.clone());
    let rt_area = runtime(sc_area, (960.0, 640.0));
    assert!(
        has_key(&rt_area, renderer_note_key(ChartType::Area)),
        "Area keeps its placeholder through the same factory"
    );
    assert!(!has_key(&rt_area, ROOT_KEY), "no bar frame for Area");
}

// ---- The log-hint pipeline ------------------------------------------------------

#[test]
fn the_log_hint_maps_series_that_span_magnitudes_into_log10_space() {
    let compact = Rc::new(RefCell::new(Vec::new()));
    let seams = recording_seams(compact.clone());
    let data = log_spanning_data();
    let cfg = config(&["a", "b"]);
    let plan = bar_chart_plan(&data, &cfg, &BTreeSet::new(), true, &seams);

    // computeLogScaleHint: 2000/10 = 200 ≥ 10, no negatives → use, domainMin 10.
    assert!(plan.log_hint.use_log);
    assert_eq!(plan.log_hint.domain_min, 10.0);

    // mapToLogScale rewrote the slots and stashed the originals.
    let r0 = &plan.chart_data[0];
    assert_eq!(r0.get("a"), Some(&ChartValue::Number(1.0))); // log10(10)
    assert_eq!(r0.get("__orig__a"), Some(&ChartValue::Number(10.0)));
    assert_eq!(r0.get("b"), Some(&ChartValue::Number(3.0))); // log10(1000)
    assert_eq!(r0.get("__orig__b"), Some(&ChartValue::Number(1000.0)));

    // The Y domain is over the mapped values, bottom-clamped at 0.
    let span = 2000.0_f64.log10();
    assert!((plan.y_domain[0] - 0.0).abs() < 1e-12);
    assert!((plan.y_domain[1] - span).abs() < 1e-12);

    // YAxis tickFormatter = logTickFormatter: the compact seam saw 10^v (the
    // same round trip the engine computed, so the spelling is not the assert).
    let top = &plan.y_ticks[0];
    assert!((top.value - span).abs() < 1e-9, "top tick first");
    let want_top = format!("c:{}", 10.0_f64.powf(span));
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

    // The bar rects follow the mapped values through the domain: the max
    // value's bar fills its whole slot from the baseline, the min sits one
    // span-share up.
    assert_rect(plan.series[1].rects[1], [0.75, 0.0, 0.225, 1.0], "b r1 = max");
    let h = (1.0 / span) as f32;
    assert_rect(
        plan.series[0].rects[0],
        [0.025, 1.0 - h, 0.225, h],
        "a r0 = log10(10) of the span",
    );
}

#[test]
fn the_log_hint_leaves_one_magnitude_data_untouched() {
    let compact = Rc::new(RefCell::new(Vec::new()));
    let seams = recording_seams(compact.clone());
    let data = near_data();
    let cfg = config(&["a", "b"]);
    let plan = bar_chart_plan(&data, &cfg, &BTreeSet::new(), true, &seams);

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
    // `toNumber`: strings parse (`Number('10') === 10`), a NaN parse is null
    // (skipped) — the arms the bar geometry reads (the slice is the area
    // engine's, shared; this pins it through the bar plan).
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

    // A NaN parse is toNumber's null: no bar, no domain contribution.
    let nan_text = vec![ChartDataPoint::new()
        .with("a", ChartValue::Text("n/a".into()))
        .with("b", ChartValue::Number(1000.0))];
    let plan = bar_chart_plan(
        &nan_text,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.series[0].rects[0], None, "'n/a' is not a drawable value");
    assert_eq!(plan.series[0].labels[0], None, "and has no label to show");
    assert_eq!(
        plan.y_domain,
        [1000.0, 1001.0],
        "the hint stays off (1000/1000 < 10); b's single value is flat and widens by one"
    );
}

// ---- Bar geometry -----------------------------------------------------------------

#[test]
fn grouped_bars_split_their_band_with_the_category_gap_and_series_slots() {
    // recharts' default grouped layout, lean (module docs): band 1/n, the
    // category content the centred 90% (barCategoryGap '10%'), the series
    // slots equal shares — `[x, y, w, h]` fractions per row.
    let plan = bar_chart_plan(
        &near_data(),
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    // Series a: row 0 [0.025, 0.6, 0.225, 0.4], row 1 [0.525, 0.2, 0.225, 0.8].
    assert_rect(plan.series[0].rects[0], [0.025, 0.6, 0.225, 0.4], "a r0");
    assert_rect(plan.series[0].rects[1], [0.525, 0.2, 0.225, 0.8], "a r1");
    // Series b: row 0 [0.25, 0.4, 0.225, 0.6], row 1 [0.75, 0.0, 0.225, 1.0].
    assert_rect(plan.series[1].rects[0], [0.25, 0.4, 0.225, 0.6], "b r0");
    assert_rect(plan.series[1].rects[1], [0.75, 0.0, 0.225, 1.0], "b r1");

    // A single series takes the whole 45% content slot (and its own domain:
    // only a's values, 10 and 20 → [0, 20]).
    let single = bar_chart_plan(
        &near_data(),
        &config(&["a"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(single.y_domain, [0.0, 20.0]);
    assert_rect(single.series[0].rects[0], [0.025, 0.5, 0.45, 0.5], "a alone r0");
    assert_rect(single.series[0].rects[1], [0.525, 0.0, 0.45, 1.0], "a alone r1");
}

#[test]
fn negative_mixed_and_flat_data_get_their_own_domains_and_zero_baselines() {
    // The lean Y domain (the area engine's slice) times the zero baseline the
    // bars hang from: non-positive data → [min, 0] with the zero line at the
    // plot's top (bars hang downward from it).
    let negatives = vec![point("r0", -5.0, -4.0), point("r1", -2.0, -1.0)];
    let plan = bar_chart_plan(
        &negatives,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [-5.0, 0.0]);
    assert_rect(plan.series[0].rects[0], [0.025, 0.0, 0.225, 1.0], "a r0 = min");
    assert_rect(plan.series[1].rects[1], [0.75, 0.0, 0.225, 0.2], "b r1 = -1");

    // Mixed data: the zero line runs mid-plot; negatives hang below it,
    // positives rise above it.
    let mixed = vec![point("r0", -2.0, 3.0)];
    let plan = bar_chart_plan(
        &mixed,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [-2.0, 3.0]);
    assert_rect(plan.series[0].rects[0], [0.05, 0.6, 0.45, 0.4], "a r0 below zero");
    assert_rect(plan.series[1].rects[0], [0.5, 0.0, 0.45, 0.6], "b r0 above zero");

    // Flat positive data widens by one and clamps the baseline to the bottom:
    // a zero-height rect — the slot and the label stay, the fill does not.
    let flat = vec![point("r0", 5.0, 5.0)];
    let plan = bar_chart_plan(
        &flat,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [5.0, 6.0]);
    assert_rect(plan.series[0].rects[0], [0.05, 1.0, 0.45, 0.0], "flat = no fill");
    assert_eq!(plan.series[0].labels[0], None, "…and showValues is off here");

    let mut values_config = config(&["a", "b"]);
    values_config.show_values = true;
    let plan = bar_chart_plan(&flat, &values_config, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(
        plan.series[0].labels[0],
        Some("5".to_string()),
        "a zero-AREA bar still labels its value"
    );
}

#[test]
fn null_and_nan_slots_leave_gaps_in_the_bar_geometry() {
    // A slot recharts cannot plot (null, or NaN — `toNumber` passes NaN
    // through, the finite gate skips it) paints no bar: `None`, no label, and
    // the domain scan skips it.
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
    let mut cfg = config(&["a", "b"]);
    cfg.show_values = true;
    let plan = bar_chart_plan(&data, &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(plan.y_domain, [0.0, 25.0], "the null and NaN never counted");
    // Bands of 1/4, slots of the 0.45 content split over two series.
    assert_rect(plan.series[0].rects[0], [0.0125, 0.6, 0.1125, 0.4], "a r0");
    assert_eq!(plan.series[0].rects[1], None, "null is a gap");
    assert_eq!(plan.series[0].rects[2], None, "NaN is a gap");
    assert_rect(plan.series[0].rects[3], [0.7625, 0.2, 0.1125, 0.8], "a r3");
    assert_eq!(
        plan.series[0].labels,
        vec![Some("10".to_string()), None, None, Some("20".to_string())]
    );
    assert!(plan.series[1].rects.iter().all(|r| r.is_some()));
}

#[test]
fn hiding_a_series_keeps_the_others_bars_in_their_slots() {
    // recharts' legend toggle hides a series without relayout: the remaining
    // bars keep their slots (and the domain keeps spanning the hidden one).
    let mut hidden = BTreeSet::new();
    hidden.insert("a".to_string());
    let plan = bar_chart_plan(
        &near_data(),
        &config(&["a", "b"]),
        &hidden,
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [0.0, 25.0], "b's max still spans the domain");
    assert!(plan.series[0].hidden && !plan.series[1].hidden);
    assert_rect(plan.series[1].rects[0], [0.25, 0.4, 0.225, 0.6], "b r0 unmoved");

    // Through the tree: hide alpha — its canvas unmounts, beta's bars and
    // label keep their rects.
    let mut cfg = config(&["a", "b"]);
    cfg.show_values = true;
    let engine = BarChartRenderer::new();
    let mut sc = scenario(cfg, near_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));
    let label_b_before = element(&rt, &format!("{VALUE_PREFIX}0-b")).rect;
    assert!(has_key(&rt, &format!("{SERIES_KEY_PREFIX}a")));
    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert!(engine.hidden_series().contains("a"), "engine owns the set");
    assert!(
        !has_key(&rt, &format!("{SERIES_KEY_PREFIX}a")),
        "the hidden series' canvas is gone"
    );
    assert!(has_key(&rt, &format!("{SERIES_KEY_PREFIX}b")));
    assert!(
        !has_key(&rt, &format!("{VALUE_PREFIX}0-a")),
        "the hidden series labels nothing"
    );
    let label_b_after = element(&rt, &format!("{VALUE_PREFIX}0-b")).rect;
    assert!(
        (label_b_before[0] - label_b_after[0]).abs() <= 0.5
            && (label_b_before[2] - label_b_after[2]).abs() <= 0.5,
        "beta's label slot does not reflow: {label_b_before:?} → {label_b_after:?}"
    );
}

// ---- Hit routing -----------------------------------------------------------------

#[test]
fn pressing_inside_a_data_point_band_routes_its_row_index() {
    let mut rt = runtime(bar_scenario(near_data()), (960.0, 640.0));

    // React 91: `<Bar onClick={(_, index) => onDataPointClick?.(index)}` —
    // the row index, not the series.
    click_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}1"));
    assert_eq!(rt.state.clicks, vec![1]);
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

// ---- The legend -------------------------------------------------------------------

#[test]
fn a_legend_press_hides_and_restores_a_series() {
    let engine = BarChartRenderer::new();
    let mut sc = bar_scenario(near_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));

    assert!(has_key(&rt, &format!("{SERIES_KEY_PREFIX}a")));
    assert!(has_key(&rt, &format!("{SERIES_KEY_PREFIX}b")));
    assert!(has_key(&rt, LEGEND_KEY), "multiSeries mounts the legend");

    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert!(engine.hidden_series().contains("a"));
    assert!(!has_key(&rt, &format!("{SERIES_KEY_PREFIX}a")));
    assert!(has_key(&rt, &format!("{SERIES_KEY_PREFIX}b")));
    // React's formatter dims the hidden entry's name to 0.35.
    assert!(
        (element(&rt, &format!("{LEGEND_LABEL_PREFIX}a")).opacity - 0.35).abs() < 1e-3,
        "the hidden label dims"
    );
    assert_eq!(element(&rt, &format!("{LEGEND_LABEL_PREFIX}b")).opacity, 1.0);

    // Press again → restored.
    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert!(has_key(&rt, &format!("{SERIES_KEY_PREFIX}a")));
    assert!(engine.hidden_series().is_empty());
    assert_eq!(element(&rt, &format!("{LEGEND_LABEL_PREFIX}a")).opacity, 1.0);
}

#[test]
fn a_single_series_mounts_no_legend() {
    let rt = runtime(scenario(config(&["a"]), near_data()), (960.0, 640.0));
    assert!(!has_key(&rt, LEGEND_KEY), "React gates the legend on multiSeries");
    assert!(has_key(&rt, &format!("{SERIES_KEY_PREFIX}a")));
}

#[test]
fn the_legend_reads_the_series_and_the_axes_take_the_recharts_defaults() {
    let rt = runtime(bar_scenario(near_data()), (960.0, 640.0));
    let k = kit(true);

    let item = element(&rt, &format!("{LEGEND_ITEM_PREFIX}a"));
    assert_eq!(item.role, "button", "the legend entry is a pressable");
    assert_eq!(item.label.as_deref(), Some("a"));
    assert_eq!(
        element(&rt, &format!("{LEGEND_LABEL_PREFIX}a")).text.as_deref(),
        Some("a")
    );
    assert_eq!(
        element(&rt, &format!("{LEGEND_LABEL_PREFIX}b")).text.as_deref(),
        Some("b")
    );
    // The swatch is the series colour: colors[0] / colors[1] in order.
    assert_eq!(
        element(&rt, &format!("{LEGEND_SWATCH_PREFIX}a")).fill,
        k.c.info.fg
    );
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

#[test]
fn series_colours_come_from_the_kit_palette_in_order_and_wrap_at_five() {
    // React 86 (`fill={colors[i % colors.length]}`): seven series walk the
    // five-colour palette and wrap, in declaration order.
    let mut row = ChartDataPoint::new();
    for (i, c) in "abcdefg".chars().enumerate() {
        row = row.with(c.to_string(), ChartValue::Number(i as f64 + 1.0));
    }
    let keys = ["a", "b", "c", "d", "e", "f", "g"];
    let plan = bar_chart_plan(
        &[row],
        &config(&keys),
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
        k.c.info.fg, // i % 5 wraps
        k.c.success.fg,
    ];
    for (i, (s, w)) in plan.series.iter().zip(want).enumerate() {
        assert_eq!(s.color, w, "series {i} takes palette[{i} % 5]");
    }
}

// ---- The plot frame -----------------------------------------------------------------

#[test]
fn the_grid_axes_and_series_canvases_fill_the_plot_rect() {
    let rt = runtime(bar_scenario(near_data()), (960.0, 640.0));
    let plot = element(&rt, PLOT_KEY).rect;
    assert!(
        plot[2] > 200.0 && plot[3] > 200.0,
        "a real plot region, got {plot:?}"
    );

    for key in [GRID_KEY, AXES_KEY, "bar-chart-series-a", "bar-chart-series-b"] {
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
    let plan = bar_chart_plan(
        &data,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    let rows: Vec<usize> = plan.x_ticks.iter().map(|t| t.row).collect();
    assert_eq!(rows, vec![0, 2, 4, 6, 8], "every ceil(10/8)=2nd band");
}

// ---- The tooltip mounts -----------------------------------------------------------

#[test]
fn hovering_a_row_mounts_the_log_scale_tooltip_and_leaving_clears_it() {
    let compact = Rc::new(RefCell::new(Vec::new()));
    let mut sc = bar_scenario(log_spanning_data());
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

    // The tooltip lives inside the hovered band.
    let cell = element(&rt, &format!("{HIT_CELL_PREFIX}1")).rect;
    let tip = element(&rt, CONTENT_KEY).rect;
    assert!(
        tip[0] >= cell[0] - 0.5 && tip[0] <= cell[0] + cell[2] + 0.5,
        "anchored at the band: {tip:?} in {cell:?}"
    );

    // Leaving the chart clears it (recharts' tooltip deactivates).
    rt.pointer_leave();
    rt.frame(1.0 / 60.0);
    assert!(!has_key(&rt, CONTENT_KEY));
}

#[test]
fn hovering_a_row_mounts_the_plain_tooltip_and_leaving_clears_it() {
    let mut rt = runtime(bar_scenario(near_data()), (960.0, 640.0));

    assert!(!has_key(&rt, TOOLTIP_KEY));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert!(has_key(&rt, TOOLTIP_KEY), "the plain contentStyle box");
    assert_eq!(
        element(&rt, "bar-chart-tooltip-item-0").text.as_deref(),
        Some("a : 10")
    );
    assert_eq!(
        element(&rt, "bar-chart-tooltip-item-1").text.as_deref(),
        Some("b : 15")
    );
    assert!(!has_key(&rt, VALUES_KEY), "showValues stays off");

    rt.pointer_leave();
    rt.frame(1.0 / 60.0);
    assert!(!has_key(&rt, TOOLTIP_KEY));
}

// ---- The LabelList (React 93-101) ---------------------------------------------------

#[test]
fn show_values_mounts_the_label_list_in_the_bars_slots() {
    // `config.showValues && <LabelList position="top" fontSize={11}>`: one
    // label per bar, in the bar's own slot at the top of its band, the
    // default `String(value)` display.
    let mut cfg = config(&["a", "b"]);
    cfg.show_values = true;
    let rt = runtime(scenario(cfg, near_data()), (960.0, 640.0));
    let k = kit(true);

    assert!(has_key(&rt, VALUES_KEY), "the overlay mounts");
    assert_eq!(
        element(&rt, &format!("{VALUE_PREFIX}0-a")).text.as_deref(),
        Some("10")
    );
    assert_eq!(
        element(&rt, &format!("{VALUE_PREFIX}1-b")).text.as_deref(),
        Some("25")
    );
    assert_eq!(
        element(&rt, &format!("{VALUE_PREFIX}0-b")).text_color,
        Some(k.c.text_muted),
        "fill: var(--c-fg-secondary, #999)"
    );

    // The label slot IS the bar slot: row 0's series-a slot starts at the
    // band's 5% inset and is the 45%/2 content share wide; the label rides
    // the plot's top (the documented anchor deviation).
    let plot = element(&rt, PLOT_KEY).rect;
    let label = element(&rt, &format!("{VALUE_PREFIX}0-a")).rect;
    assert!(
        (label[0] - (plot[0] + 0.025 * plot[2])).abs() <= 1.5,
        "slot x = band + 5%: {label:?} vs {plot:?}"
    );
    assert!((label[2] - 0.225 * plot[2]).abs() <= 1.5, "slot w = 45%/2: {label:?} vs {plot:?}");
    assert!((label[1] - plot[1]).abs() <= 2.0, "the label rides the band's top: {label:?} vs {plot:?}");
}

#[test]
fn show_values_under_the_log_hint_labels_the_round_tripped_magnitude() {
    // React 99: `formatter={logHint.use ? (v) => logTickFormatter(Number(v))
    // : undefined}` — under the hint the label is `formatCompact(10^v)` of the
    // mapped slot, through the seam.
    let compact = Rc::new(RefCell::new(Vec::new()));
    let seams = recording_seams(compact.clone());
    let mut cfg = config(&["a", "b"]);
    cfg.show_values = true;
    let plan = bar_chart_plan(&log_spanning_data(), &cfg, &BTreeSet::new(), true, &seams);
    let want_a1 = format!("c:{}", 10.0_f64.powf(20.0_f64.log10()));
    assert_eq!(plan.series[0].labels[1], Some(want_a1), "10^log10(20)");
    let want_b1 = format!("c:{}", 10.0_f64.powf(2000.0_f64.log10()));
    assert_eq!(plan.series[1].labels[1], Some(want_b1), "the round-tripped magnitude");
    let seen = compact.borrow();
    assert!(
        seen.iter().any(|v| (v - 2000.0).abs() < 1e-6),
        "the seam formatted the round-tripped 10^log10(2000), saw {seen:?}"
    );
}

#[test]
fn the_format_seams_see_the_axis_tick_and_compact_values() {
    // React 45-49 + 53: the X axis ticks run `formatAxisTick` over the row's
    // slot under `config.xAxis ?? '__index'`, the Y ticks `formatCompact`.
    let axis_seen = Rc::new(RefCell::new(Vec::new()));
    let axis = {
        let seen = axis_seen.clone();
        Rc::new(move |v: &ChartValue| {
            seen.borrow_mut().push(v.as_display());
            format!("t:{}", v.as_display())
        })
    };
    let compact = Rc::new(RefCell::new(Vec::new()));
    let seams = FormatSeams {
        format_compact: recording_compact(&compact),
        format_axis_tick: axis,
        ..FormatSeams::default()
    };
    let mut cfg = config(&["a", "b"]);
    cfg.x_axis = Some("label".into());
    let plan = bar_chart_plan(&near_data(), &cfg, &BTreeSet::new(), true, &seams);

    assert_eq!(plan.x_key, "label");
    let labels: Vec<&str> = plan.x_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(labels, vec!["t:r0", "t:r1"], "the label column feeds the seam");
    assert_eq!(
        axis_seen.borrow().as_slice(),
        ["r0", "r1"],
        "one call per drawn tick"
    );
    assert!(
        compact.borrow().iter().any(|v| *v == 25.0) && compact.borrow().iter().any(|v| *v == 0.0),
        "the Y ticks formatted their quarter-domain values"
    );
}

// ---- Hidden payload + data shrink (recharts clears activeTooltipIndex) ---------------

#[test]
fn hiding_a_series_keeps_it_out_of_the_tooltip_payload() {
    // React 88 (`hide={hiddenSeries.has(yKey)}`): recharts drops the hidden
    // series from the tooltip payload while the domain and the slots keep
    // spanning it.
    let engine = BarChartRenderer::new();
    let mut sc = bar_scenario(near_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));
    click_key_center(&mut rt, &format!("{LEGEND_ITEM_PREFIX}b"));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert!(has_key(&rt, TOOLTIP_KEY));
    assert_eq!(
        element(&rt, "bar-chart-tooltip-item-0").text.as_deref(),
        Some("a : 10")
    );
    assert!(
        !has_key(&rt, "bar-chart-tooltip-item-1"),
        "the hidden series is not in the payload"
    );
}

#[test]
fn shrinking_the_data_under_a_hover_unmounts_the_tooltip() {
    // The hovered row index is clamped to the rows that still exist: the
    // tooltip unmounts instead of pointing past the data (recharts clears its
    // activeTooltipIndex when the row count drops).
    let mut rt = runtime(bar_scenario(near_data()), (960.0, 640.0));
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}1"));
    assert!(has_key(&rt, TOOLTIP_KEY));

    rt.update(|s: &mut ChartState, _tx| {
        s.scenario.data = vec![point("r0", 10.0, 15.0)];
    });
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
    assert!(!has_key(&rt, TOOLTIP_KEY), "the gone row unmounts the tooltip");
}

// ---- The grid's invisible arm + the empty chart ---------------------------------------

#[test]
fn the_grid_canvas_stays_mounted_at_zero_visibility_when_show_grid_is_false() {
    // React 39-43 (`opacity={config.showGrid ? 0.3 : 0}`): a hidden grid is an
    // invisible one, not an unmounted one — the canvas stays in the tree.
    let mut sc = bar_scenario(near_data());
    sc.config.show_grid = false;
    let rt = runtime(sc, (960.0, 640.0));
    assert!(has_key(&rt, GRID_KEY), "the CartesianGrid stays mounted");
}

#[test]
fn an_empty_chart_keeps_an_honest_frame() {
    // React renders the empty <BarChart> frame: axes and (multiSeries) the
    // legend, no rows. The lean domain is an honest [0, 1].
    let cfg = config(&["a", "b"]);
    let plan = bar_chart_plan(&[], &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(plan.row_count, 0);
    assert_eq!(plan.y_domain, [0.0, 1.0]);
    assert!(plan.x_ticks.is_empty() && plan.grid_xs.is_empty());
    assert_eq!(plan.y_ticks.len(), 5);
    assert_eq!(plan.series[0].rects, Vec::<Option<[f32; 4]>>::new());
    assert_eq!(plan.series[0].labels, Vec::<Option<String>>::new());

    // The frame mounts: axes, the legend (multiSeries), no hit cells.
    let rt = runtime(bar_scenario(Vec::new()), (960.0, 640.0));
    assert!(has_key(&rt, ROOT_KEY));
    assert!(has_key(&rt, Y_AXIS_KEY) && has_key(&rt, X_AXIS_KEY));
    let cells: Vec<String> = keys_with(&rt, HIT_CELL_PREFIX)
        .into_iter()
        .filter(|k| k != HIT_ZONE_KEY)
        .collect();
    assert!(cells.is_empty(), "no rows → no hit cells: {cells:?}");
    assert_eq!(keys_with(&rt, LEGEND_ITEM_PREFIX).len(), 2);
}

// ---- Window fit --------------------------------------------------------------------

#[test]
fn every_part_stays_inside_the_window_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for compact in [false, true] {
                let mut sc = bar_scenario(log_spanning_data());
                sc.config.show_values = true; // the labels ride too
                sc.dark = dark;
                sc.compact = compact;
                sc.height = Some(240.0);
                let rt = runtime(sc, (w, h));
                assert_in_window(
                    &rt,
                    w,
                    h,
                    &format!("bar dark={dark} compact={compact} {w}x{h}"),
                );
                assert!(has_key(&rt, ROOT_KEY));
            }
        }
    }
}

// ==== T192 audit round =============================================================
//
// Every test below pins a seam the port's first 24 tests missed; each one was
// found by a probe mutation that survived the whole trimmed suite (196 green
// with the probe live), and every probe dies on its pin. React positions cite
// `packages/frontend/src/components/chart/renderers/BarChartRenderer.tsx` (or
// the lean `lib/chart` slice's own file), line by line. The probes that
// survived the round and could not be pinned are documented in the module
// header as the equivalent-mutation exemptions.

// ---- The X-tick cap's two edges (recharts auto-interval, MAX_X_TICKS = 8) ----------

#[test]
fn eight_rows_still_tick_every_band_and_nine_rows_start_thinning() {
    // The lean recharts-interval cap is 8: eight rows take stride
    // ceil(8/8) = 1 — every band ticks (the wide-row 10-tick pin above
    // guards the stride-2 body; this one guards both edges of the literal).
    // Probe: MAX_X_TICKS 8 → 7 thins eight rows (stride 2); 8 → 9 leaves
    // nine rows untick-thinned (stride 1).
    let eight: Vec<ChartDataPoint> = (0..8)
        .map(|i| point(&format!("r{i}"), i as f64, 7.0 - i as f64))
        .collect();
    let plan = bar_chart_plan(
        &eight,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    let rows: Vec<usize> = plan.x_ticks.iter().map(|t| t.row).collect();
    assert_eq!(rows, vec![0, 1, 2, 3, 4, 5, 6, 7], "stride 1 at n = 8");
    let want: Vec<f32> = (0..8).map(|r| (r as f32 + 0.5) / 8.0).collect();
    assert_eq!(plan.grid_xs, want, "a hairline under every band");

    let nine: Vec<ChartDataPoint> = (0..9)
        .map(|i| point(&format!("r{i}"), i as f64, 8.0 - i as f64))
        .collect();
    let plan = bar_chart_plan(
        &nine,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    let rows: Vec<usize> = plan.x_ticks.iter().map(|t| t.row).collect();
    assert_eq!(
        rows,
        vec![0, 2, 4, 6, 8],
        "stride ceil(9/8) = 2 at n = 9 — the cap starts thinning"
    );
}

// ---- The X axis' key fallback (React 45) -------------------------------------------

#[test]
fn the_x_key_reads_the_index_fallback_column_and_an_absent_slot_ticks_empty() {
    // React 45 (`dataKey={config.xAxis ?? '__index'}`): no xAxis configured →
    // the axis reads the `__index` column the upstream pipeline writes; a row
    // without it ticks the null arm's empty label. Probe: the fallback spelled
    // `""` instead of `'__index'` survived the 24 pins.
    let indexed = vec![
        ChartDataPoint::new()
            .with("__index", ChartValue::Number(0.0))
            .with("a", ChartValue::Number(10.0))
            .with("b", ChartValue::Number(15.0)),
        ChartDataPoint::new()
            .with("__index", ChartValue::Number(5.0))
            .with("a", ChartValue::Number(20.0))
            .with("b", ChartValue::Number(25.0)),
        ChartDataPoint::new() // no __index slot at all
            .with("a", ChartValue::Number(20.0))
            .with("b", ChartValue::Number(25.0)),
    ];
    let plan = bar_chart_plan(
        &indexed,
        &config(&["a", "b"]), // x_axis stays None
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.x_key, "__index", "React 45's fallback literal");
    let labels: Vec<&str> = plan.x_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["0", "5", ""],
        "the __index slot feeds formatAxisTick; an absent slot is null → ''"
    );
}

// ---- The plain tooltip's value display arms (React 55-67, js_string) ----------------

#[test]
fn the_plain_tooltip_shows_js_string_display_for_every_value_kind() {
    // React 58-66: the non-log `<Tooltip>` content is recharts' default —
    // each entry prints the slot through JS `String(v)`: null → '' (the
    // trailing empty value), NaN → 'NaN', ±Infinity → 'Infinity'/
    // '-Infinity', 0 → '0' (js_string's arms, the T136/T166 precedent).
    // Probe: `js_string(n)` → Rust's `n.to_string()` survived — the number
    // pins print the same, only the infinity spellings differ.
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
    // -Infinity (r3) flips transform.ts's hasNegative → the hint stays off
    // and the plain tooltip arm runs for every row.
    let mut rt = runtime(scenario(config(&["a", "b"]), data), (960.0, 640.0));
    let item = |rt: &Runtime<ChartState>, i: usize| -> Option<String> {
        element(rt, &format!("bar-chart-tooltip-item-{i}"))
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

// ---- Text slots show verbatim (React 58-66, 93-101) ---------------------------------

#[test]
fn text_slots_show_verbatim_in_the_label_list_and_the_plain_tooltip() {
    // React 93-101 (`<LabelList>` with `formatter` undefined — React 99 —
    // shows recharts' default `String(value)` display) + React 58-66 (the
    // plain tooltip's default content): a text slot is its own string,
    // verbatim — the number coercions that *plot* it (toNumber trims) never
    // round-trip the display. Probe: the text arms trimmed/cleared
    // (`s.trim()`, `String::new()`) survived the 24 pins.
    let data = vec![
        ChartDataPoint::new()
            .with("label", ChartValue::Text("r0".into()))
            .with("a", ChartValue::Text(" 12 ".into()))
            .with("b", ChartValue::Number(15.0)),
    ];
    let mut cfg = config(&["a", "b"]);
    cfg.show_values = true;
    let plan = bar_chart_plan(&data, &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert!(!plan.log_hint.use_log, "12 against 15 stays off");
    assert_eq!(
        plan.series[0].labels[0],
        Some(" 12 ".to_string()),
        "the LabelList text is the slot verbatim, spaces included"
    );
    assert_eq!(
        plan.series[1].labels[0],
        Some("15".to_string()),
        "a number slot is js_string's display"
    );
    // The bar still plots the *coerced* 12: the domain is [0, 15], so the
    // a bar tops at 1 - 12/15 = 0.2 with height 0.8 (and the b bar fills).
    assert_rect(plan.series[0].rects[0], [0.05, 0.2, 0.45, 0.8], "a r0 plots 12");
    assert_rect(plan.series[1].rects[0], [0.5, 0.0, 0.45, 1.0], "b r0 fills");

    // And through the tree: the label and the plain tooltip both keep the
    // verbatim text.
    let mut rt = runtime(scenario(cfg, data), (960.0, 640.0));
    assert_eq!(
        element(&rt, &format!("{VALUE_PREFIX}0-a"))
            .text
            .as_deref(),
        Some(" 12 ")
    );
    hover_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0"));
    assert_eq!(
        element(&rt, "bar-chart-tooltip-item-0").text.as_deref(),
        Some("a :  12 "),
        "the tooltip prints the slot verbatim, not the coerced 12"
    );
    assert_eq!(
        element(&rt, "bar-chart-tooltip-item-1").text.as_deref(),
        Some("b : 15")
    );
}

// ---- The chrome inks (React 46, 51, 70, 98) -------------------------------------------

#[test]
fn ticks_take_the_secondary_ink_and_the_legend_names_the_strong_ink() {
    // React 46/51 (`fill: 'var(--c-fg-secondary, #999)'` on both axes' ticks)
    // + React 70 (the Legend wrapper's `color: 'var(--c-fg, #eee)'`): the
    // axis ticks read the collapsed secondary ink, the legend names the
    // strong one. Probe: both axes' tick ink and the legend name ink
    // flipped to the opposite kit token and survived.
    let rt = runtime(bar_scenario(near_data()), (960.0, 640.0));
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

// ---- The plain tooltip's contentStyle chrome + anchor (React 58-66) -------------------

#[test]
fn the_plain_tooltip_keeps_reacts_contentstyle_chrome_and_anchor() {
    // React 58-66 (`contentStyle`: background var(--c-surface-alt), border
    // 1px var(--c-edge), borderRadius 6, color var(--c-fg, #eee)) on the
    // token mapping the module docs declare (layer_01 / border_base /
    // radius.md / text_strong) — plus the port's declared anchor: the
    // hovered band's top-left inset by (2, 8). Probes: fill → layer_02,
    // radius → sm and a 6pt anchor dx all survived.
    let mut rt = runtime(bar_scenario(near_data()), (960.0, 640.0));
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
        "anchored at band.x + 2: {:?} in {:?}",
        tip.rect,
        cell
    );
    assert!(
        (tip.rect[1] - (cell[1] + 8.0)).abs() <= 0.5,
        "anchored at band.y + 8: {:?} in {:?}",
        tip.rect,
        cell
    );
}

// ---- The X axis' band ladder keeps its empty rungs (React 44-49) ----------------------

#[test]
fn the_x_axis_row_lays_every_band_even_where_no_tick_draws() {
    // React 44-49: the X axis ticks *bands*, not ticks — recharts lays each
    // label over its own band centre; the port's rung elements (empty where
    // the stride skips a row) keep that ladder. Probe: dropping the empty
    // rungs survived the 24 pins (every n = 2 row is a tick, so nothing
    // shifted); with ten rows the surviving ticks slid a band left.
    let data: Vec<ChartDataPoint> = (0..10)
        .map(|i| point(&format!("r{i}"), i as f64, 9.0 - i as f64))
        .collect();
    let rt = runtime(bar_scenario(data), (960.0, 640.0));

    let axis = element(&rt, X_AXIS_KEY).rect;
    let drawn: Vec<String> = keys_with(&rt, &format!("{X_AXIS_KEY}-tick-"));
    assert_eq!(drawn.len(), 5, "stride 2 draws r0/r2/r4/r6/r8: {drawn:?}");
    // Tick r2's label centres over band 2's centre — the empty rungs for
    // bands 1 and 3 keep it there.
    for (row, frac) in [(0usize, 0.05f32), (2, 0.25), (8, 0.85)] {
        let t = element(&rt, &format!("{X_AXIS_KEY}-tick-{row}")).rect;
        let want = axis[0] + frac * axis[2];
        assert!(
            (t[0] + t[2] * 0.5 - want).abs() <= 2.0,
            "tick {row} centred at {want}, got {t:?} vs axis {axis:?}"
        );
    }
}

// ---- The legend row's declared geometry (module docs: 22pt row, 8pt swatch) ----------

#[test]
fn the_legend_row_declares_its_22pt_row_and_8pt_swatches() {
    // Not React literals (recharts sizes the legend from its content); the
    // port declares the 22pt row (module docs) and the 8pt swatch chip —
    // pinned as the layout regression guards they are, the T166 precedent.
    // Probe: 22 → 23 and the 8pt chip → 9 both survived.
    let rt = runtime(bar_scenario(near_data()), (960.0, 640.0));
    let legend = element(&rt, LEGEND_KEY).rect;
    assert!((legend[3] - 22.0).abs() <= 0.5, "the legend row is 22pt: {legend:?}");
    let swatch = element(&rt, &format!("{LEGEND_SWATCH_PREFIX}a")).rect;
    assert!(
        (swatch[2] - 8.0).abs() <= 0.5 && (swatch[3] - 8.0).abs() <= 0.5,
        "the swatch chip is 8x8: {swatch:?}"
    );
}

// ---- The flat domain's negative arm (recharts' constant-domain extension) -------------

#[test]
fn negative_flat_data_hangs_its_bars_from_the_clamped_top_zero_line() {
    // The lean domain's flat arm fires *before* the sign branches: a constant
    // -5 widens to [-5, -4], which does not contain zero — the zero baseline
    // recharts hangs every bar from clamps to the plot's *top* edge (the
    // module docs' flat-domain clamp), and both bars hang downward from it,
    // full-height. The mirror of the flat-positive pin ([5, 6] → baseline at
    // the bottom); boundary pin — no probe survived alone here, the adjacent
    // geometry pins catch the zero-line mutations.
    let flat = vec![point("r0", -5.0, -5.0)];
    let plan = bar_chart_plan(
        &flat,
        &config(&["a", "b"]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert_eq!(plan.y_domain, [-5.0, -4.0], "flat widens upward, not to zero");
    assert!((plan.y_ticks[0].value - (-4.0)).abs() < 1e-12, "the top tick is -4");
    assert_eq!(plan.y_ticks[0].label, "-4");
    assert_rect(
        plan.series[0].rects[0],
        [0.05, 0.0, 0.45, 1.0],
        "a r0 hangs from the clamped top zero line",
    );
    assert_rect(plan.series[1].rects[0], [0.5, 0.0, 0.45, 1.0], "b r0 too");

    let mut cfg = config(&["a", "b"]);
    cfg.show_values = true;
    let plan = bar_chart_plan(&flat, &cfg, &BTreeSet::new(), true, &FormatSeams::default());
    assert_eq!(
        plan.series[0].labels[0],
        Some("-5".to_string()),
        "a zero-area hanging bar still labels its value"
    );
}

// ---- No Y axes, no Bars (React 82-103 gates on config.yAxes) -------------------------

#[test]
fn a_bar_chart_with_no_y_axes_keeps_the_frame_and_still_ticks_the_x_bands() {
    // React 82-103: `config.yAxes.map(...)` mounts no `<Bar>` when yAxes is
    // empty — the `<BarChart>` frame (grid, both axes, the hit bands over the
    // rows) still renders, and `multiSeries` (React 35, `length > 1`) stays
    // false so no Legend mounts. Boundary pin for the empty-series config.
    let data = vec![point("r0", 10.0, 15.0), point("r1", 20.0, 25.0)];
    let plan = bar_chart_plan(
        &data,
        &config(&[]),
        &BTreeSet::new(),
        true,
        &FormatSeams::default(),
    );
    assert!(plan.series.is_empty(), "no yAxes → no series plans");
    assert!(!plan.multi_series);
    assert_eq!(plan.row_count, 2, "the rows still exist");
    assert_eq!(plan.y_domain, [0.0, 1.0], "the honest no-data domain");
    let rows: Vec<usize> = plan.x_ticks.iter().map(|t| t.row).collect();
    assert_eq!(rows, vec![0, 1], "the X bands still tick");

    let rt = runtime(scenario(config(&[]), data), (960.0, 640.0));
    assert!(has_key(&rt, ROOT_KEY) && has_key(&rt, GRID_KEY) && has_key(&rt, AXES_KEY));
    assert!(has_key(&rt, Y_AXIS_KEY) && has_key(&rt, X_AXIS_KEY));
    assert!(!has_key(&rt, LEGEND_KEY));
    assert!(keys_with(&rt, SERIES_KEY_PREFIX).is_empty(), "no series canvases");
    let hit_cells: Vec<String> = keys_with(&rt, HIT_CELL_PREFIX)
        .into_iter()
        .filter(|k| k != HIT_ZONE_KEY)
        .collect();
    assert_eq!(hit_cells.len(), 2, "one hit band per row");
}

// ---- The Bar X axis has no time-domain arm (React 44-49 vs Area's 60-67) --------------

#[test]
fn the_bar_x_axis_has_no_time_domain_arm_and_never_calls_the_epoch_seam() {
    // React 44-49: the Bar source's `<XAxis>` carries only `dataKey`,
    // `tick`, `stroke` and `tickFormatter={formatAxisTick}` — unlike the
    // area source (React 60-67) it never imports `formatEpochMs` and has no
    // `domain`/`ticks` props, so `config.timeDomain` contributes nothing:
    // every row ticks its band, in or out of the configured window, and the
    // epoch seam is never called. Guards the module docs' "no time-domain
    // arm" claim and the never-wired `formatEpochMs` slot.
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
    let mut cfg = config(&["a", "b"]);
    cfg.x_axis = Some("t".into());
    cfg.time_domain = Some([0.0, 100.0]);
    cfg.time_ticks = vec![0.0, 50.0];
    // 150 sits outside the [0, 100] window the area engine would hide.
    let data = vec![
        ChartDataPoint::new()
            .with("t", ChartValue::Number(150.0))
            .with("a", ChartValue::Number(10.0))
            .with("b", ChartValue::Number(15.0)),
        ChartDataPoint::new()
            .with("t", ChartValue::Number(200.0))
            .with("a", ChartValue::Number(20.0))
            .with("b", ChartValue::Number(25.0)),
    ];
    let plan = bar_chart_plan(&data, &cfg, &BTreeSet::new(), true, &seams);
    let rows: Vec<usize> = plan.x_ticks.iter().map(|t| t.row).collect();
    assert_eq!(rows, vec![0, 1], "the window does not hide any row");
    let labels: Vec<&str> = plan.x_ticks.iter().map(|t| t.label.as_str()).collect();
    assert_eq!(
        labels,
        vec!["150", "200"],
        "formatAxisTick of the t slot — plain numbers, no clock"
    );
    assert!(
        epoch_seen.borrow().is_empty(),
        "the Bar XAxis never imports formatEpochMs: {:?}",
        epoch_seen.borrow()
    );
}
