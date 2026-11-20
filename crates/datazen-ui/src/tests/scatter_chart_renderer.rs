//! T261: the scatter chart engine — the numeric X/Y domains and the point
//! mapping into the view box, null/NaN gaps, both `formatCompact` axes, the
//! single palette colour, the plain tooltip/cursor, the hit raster's point
//! routing, and the 960×640 / 1440×816 window fit.
//!
//! Tree observations come from `rt.elements()` (runtime-measured
//! rects/text/colour), pressed and hovered through the runtime's own pointer
//! pipeline; the pure plan (`scatter_chart_plan` over the area engine's shared
//! lean slices) is asserted directly, where the exact geometry lives.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use crate::components::chart::chart_canvas::{
    renderer_note_key, ChartCanvas, ChartConfig, ChartDataPoint, ChartType, ChartValue,
    PlaceholderChartRenderer,
};
use crate::components::chart::renderers::scatter_chart_renderer::{
    dot_at, gon, renderer_factory, scatter_chart_plan, FormatSeams, ScatterChartPlan,
    ScatterChartRenderer, AXES_KEY, CURSOR_KEY, DOTS_KEY,
    DOT_GON_SIDES, DOT_RADIUS, GRID_KEY, HIT_CELL_PREFIX, HIT_GRID, HIT_RADIUS, HIT_ZONE_KEY,
    LEGEND_KEY, LEGEND_NAME_KEY, LEGEND_SWATCH_KEY, NUM_TICKS, PLOT_KEY, ROOT_KEY, TOOLTIP_KEY,
    X_AXIS_KEY, Y_AXIS_KEY,
};
use crate::ui::kit::kit;

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

/// A drawable point against its wanted view-box fractions (f32 arithmetic, so
/// the pins compare with slack).
fn assert_point(actual: Option<[f32; 2]>, want: [f32; 2], label: &str) {
    let a = actual.unwrap_or_else(|| panic!("{label} is a drawable point, got None"));
    for i in 0..2 {
        assert!(
            (a[i] - want[i]).abs() <= 1e-5,
            "{label} dim {i}: {a:?} vs {want:?}"
        );
    }
}

// ---- Fixtures ----------------------------------------------------------------

fn point(x: f64, y: f64) -> ChartDataPoint {
    ChartDataPoint::new()
        .with("x", ChartValue::Number(x))
        .with("y", ChartValue::Number(y))
}

/// Three points on a 0..10 × 0..20 grid — the domain/mapping fixture.
fn grid_data() -> Vec<ChartDataPoint> {
    vec![point(0.0, 0.0), point(5.0, 10.0), point(10.0, 20.0)]
}

/// Well-separated points whose mapped centres own distinct raster cells.
fn spread_data() -> Vec<ChartDataPoint> {
    vec![point(2.0, 3.0), point(8.0, 9.0), point(15.0, 14.0)]
}

fn config() -> ChartConfig {
    ChartConfig {
        chart_type: ChartType::Scatter,
        x_axis: Some("x".into()),
        y_axes: vec!["y".into()],
        ..ChartConfig::default()
    }
}

/// A recording `formatCompact` seam: every value it sees is pushed and echoed
/// as `c:{value}` — the observer for which numbers reached the formatter.
fn recording_compact(seen: &Rc<RefCell<Vec<f64>>>) -> Rc<dyn Fn(f64) -> String> {
    let seen = seen.clone();
    Rc::new(move |v: f64| {
        seen.borrow_mut().push(v);
        format!("c:{v}")
    })
}

fn plan(data: &[ChartDataPoint], cfg: &ChartConfig) -> ScatterChartPlan {
    scatter_chart_plan(data, cfg, true, &FormatSeams::default())
}

/// The raster cell whose centre is nearest a drawable point, with an assertion
/// that the point really owns that cell (so a stray HIT_RADIUS mutation dies
/// here rather than making a routing test flaky).
fn cell_for_point(plan: &ScatterChartPlan, row: usize) -> String {
    let [px, py] = plan.points[row].unwrap_or_else(|| panic!("row {row} is drawable"));
    let c = ((px * HIT_GRID as f32) as usize).min(HIT_GRID - 1);
    let r = ((py * HIT_GRID as f32) as usize).min(HIT_GRID - 1);
    let centre = [
        (c as f32 + 0.5) / HIT_GRID as f32,
        (r as f32 + 0.5) / HIT_GRID as f32,
    ];
    assert_eq!(
        dot_at(plan, centre[0], centre[1]),
        Some(row),
        "row {row}'s cell owns it: centre {centre:?}, point {px},{py}"
    );
    format!("{HIT_CELL_PREFIX}{r}-{c}")
}

// ---- Probe view ---------------------------------------------------------------

#[derive(Clone)]
struct Scenario {
    config: ChartConfig,
    data: Vec<ChartDataPoint>,
    dark: bool,
    compact: bool,
    height: Option<f32>,
    engine: ScatterChartRenderer,
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
            engine: ScatterChartRenderer::new(),
            seen: None,
        }
    }
}

#[derive(Default)]
struct ChartState {
    scenario: Scenario,
    clicks: Vec<usize>,
}

fn scatter_view(s: &ChartState, _cx: &mut Cx<ChartState>) -> El {
    let sc = &s.scenario;
    let engine = sc.engine.clone();
    let mut canvas = ChartCanvas::<ChartState>::new(&sc.config, &sc.data)
        .dark(sc.dark)
        .compact(sc.compact)
        .on_data_point_click(|st: &mut ChartState, i: usize| st.clicks.push(i));
    if let Some(h) = sc.height {
        canvas = canvas.height(h);
    }
    if let Some(seen) = &sc.seen {
        let seen = seen.clone();
        canvas = canvas.renderer_factory(move |t| {
            seen.borrow_mut().push(t);
            match t {
                ChartType::Scatter => Box::new(engine.clone()),
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
        scatter_view,
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

fn scatter_scenario(data: Vec<ChartDataPoint>) -> Scenario {
    scenario(config(), data)
}

fn click_key_center(rt: &mut Runtime<ChartState>, key: &str) {
    let r = element(rt, key).rect;
    rt.pointer_down(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.pointer_up(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.frame(1.0 / 60.0);
}

fn hover_key_center(rt: &mut Runtime<ChartState>, key: &str) {
    let r = element(rt, key).rect;
    rt.pointer_move(r[0] + r[2] * 0.5, r[1] + r[3] * 0.5);
    rt.frame(1.0 / 60.0);
}

// ---- Factory dispatch ---------------------------------------------------------

#[test]
fn the_factory_routes_scatter_to_the_engine_and_other_types_to_placeholders() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut sc = scatter_scenario(grid_data());
    sc.seen = Some(seen.clone());
    let rt = runtime(sc, (960.0, 640.0));

    assert!(has_key(&rt, ROOT_KEY), "the engine's frame is mounted");
    assert!(
        !has_key(&rt, renderer_note_key(ChartType::Scatter)),
        "the Scatter placeholder note is gone"
    );
    assert!(
        seen.borrow().iter().all(|t| *t == ChartType::Scatter),
        "every build selected the scatter engine: {:?}",
        seen.borrow()
    );

    let seen_bar = Rc::new(RefCell::new(Vec::new()));
    let mut bar_cfg = config();
    bar_cfg.chart_type = ChartType::Bar;
    let mut sc_bar = scenario(bar_cfg, grid_data());
    sc_bar.seen = Some(seen_bar.clone());
    let rt_bar = runtime(sc_bar, (960.0, 640.0));
    assert!(
        has_key(&rt_bar, renderer_note_key(ChartType::Bar)),
        "Bar keeps its placeholder through the same factory"
    );
    assert!(!has_key(&rt_bar, ROOT_KEY), "no scatter frame for Bar");
}

// ---- Domains + point mapping ---------------------------------------------------

#[test]
fn points_map_both_numeric_domains_into_the_view_box() {
    let p = plan(&grid_data(), &config());
    assert_eq!(p.row_count, 3);
    assert_eq!(p.points.len(), 3, "one slot per data row");
    assert_eq!(p.x_domain, [0.0, 10.0]);
    assert_eq!(p.y_domain, [0.0, 20.0]);
    // x = (xv - x0) / xspan; y = 1 - (yv - y0) / yspan.
    assert_point(p.points[0], [0.0, 1.0], "row 0 = domain min corner");
    assert_point(p.points[1], [0.5, 0.5], "row 1 = domain mid");
    assert_point(p.points[2], [1.0, 0.0], "row 2 = domain max corner");
}

#[test]
fn each_axis_derives_its_own_domain_from_its_own_finite_values() {
    // X is all positive → [0, max]; Y spans zero → [min, max].
    let data = vec![point(10.0, -5.0), point(20.0, 0.0), point(30.0, 5.0)];
    let p = plan(&data, &config());
    assert_eq!(p.x_domain, [0.0, 30.0]);
    assert_eq!(p.y_domain, [-5.0, 5.0]);
    assert_point(p.points[0], [10.0 / 30.0, 1.0], "x=10 of 30, y=min");
    assert_point(p.points[2], [1.0, 0.0], "x=max, y=max");

    // A flat column widens by one (recharts' default-domain lean slice).
    let flat = vec![point(5.0, 5.0), point(5.0, 5.0)];
    let p = plan(&flat, &config());
    assert_eq!(p.x_domain, [5.0, 6.0]);
    assert_eq!(p.y_domain, [5.0, 6.0]);
    assert_point(p.points[0], [0.0, 1.0], "flat domain still maps inside [0,1]");
}

#[test]
fn null_nan_absent_and_text_nan_rows_leave_gaps_without_touching_the_domains() {
    let data = vec![
        point(0.0, 0.0),
        ChartDataPoint::new()
            .with("x", ChartValue::Number(4.0))
            .with("y", ChartValue::Null), // null y
        ChartDataPoint::new()
            .with("x", ChartValue::Number(5.0))
            .with("y", ChartValue::Number(f64::NAN)), // NaN y
        ChartDataPoint::new()
            .with("x", ChartValue::Text("n/a".into())) // NaN-parse x
            .with("y", ChartValue::Number(10.0)),
        ChartDataPoint::new().with("x", ChartValue::Number(6.0)), // absent y
        point(10.0, 20.0),
    ];
    let p = plan(&data, &config());
    // Only the two fully-finite rows contribute to either domain.
    assert_eq!(p.x_domain, [0.0, 10.0]);
    assert_eq!(p.y_domain, [0.0, 20.0]);
    assert!(p.points[0].is_some() && p.points[5].is_some());
    for row in 1..=4 {
        assert_eq!(p.points[row], None, "row {row} is a gap");
    }
}

// ---- The formatCompact axes ----------------------------------------------------

#[test]
fn both_axes_ticks_are_five_formatcompact_values_and_the_grid_follows_them() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let seams = FormatSeams {
        format_compact: recording_compact(&seen),
        ..FormatSeams::default()
    };
    let p = scatter_chart_plan(&grid_data(), &config(), true, &seams);

    assert_eq!(p.x_ticks.len(), NUM_TICKS);
    assert_eq!(p.y_ticks.len(), NUM_TICKS);
    let xs: Vec<f64> = p.x_ticks.iter().map(|t| t.value).collect();
    let ys: Vec<f64> = p.y_ticks.iter().map(|t| t.value).collect();
    assert_eq!(xs, vec![0.0, 2.5, 5.0, 7.5, 10.0], "X ticks left to right");
    assert_eq!(ys, vec![20.0, 15.0, 10.0, 5.0, 0.0], "Y ticks top first");
    assert_eq!(p.x_ticks[0].label, "c:0");
    assert_eq!(p.y_ticks[0].label, "c:20");
    assert_eq!(p.grid_xs, vec![0.0, 0.25, 0.5, 0.75, 1.0]);
    assert_eq!(p.grid_ys, vec![0.0, 0.25, 0.5, 0.75, 1.0]);

    // Exactly the two ladders' values reached the one seam (no formatAxisTick).
    let mut wanted = xs;
    wanted.extend(ys);
    let mut got = seen.borrow().clone();
    got.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    wanted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    assert_eq!(got, wanted, "each tick value was formatted once");
}

// ---- Colour + legend -----------------------------------------------------------

#[test]
fn every_dot_shares_the_single_palette_slot_and_the_legend_gate_is_show_legend() {
    let p = plan(&grid_data(), &config());
    let k = kit(true);
    let palette = crate::components::chart::renderers::area_chart_renderer::series_palette(k);
    assert_eq!(p.color, palette[0], "React 67: fill={{colors[0]}} for all dots");
    assert_eq!(p.legend_name, "y", "the single entry is labelled by yAxes[0]");

    // A different colour scheme still maps to the same lean palette slot.
    let mut other = config();
    other.color_scheme = "ocean".into();
    assert_eq!(plan(&grid_data(), &other).color, palette[0]);

    let rt = runtime(scatter_scenario(grid_data()), (960.0, 640.0));
    assert!(has_key(&rt, LEGEND_KEY), "showLegend defaults true");
    assert_eq!(element(&rt, LEGEND_SWATCH_KEY).fill, palette[0]);
    assert_eq!(element(&rt, LEGEND_NAME_KEY).text.as_deref(), Some("y"));

    let mut off = scatter_scenario(grid_data());
    off.config.show_legend = false;
    let rt = runtime(off, (960.0, 640.0));
    assert!(!has_key(&rt, LEGEND_KEY), "the showLegend gate hides the row");
    assert!(has_key(&rt, DOTS_KEY), "the dots themselves stay");
}

#[test]
fn the_legend_is_a_plain_entry_not_a_hidden_series_toggle() {
    let engine = ScatterChartRenderer::new();
    let mut sc = scatter_scenario(grid_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));

    assert_ne!(
        element(&rt, LEGEND_NAME_KEY).role,
        "button",
        "the scatter legend has no onClick / button role"
    );
    assert!(!element(&rt, LEGEND_NAME_KEY).focusable, "the plain entry is not focusable");
    click_key_center(&mut rt, LEGEND_SWATCH_KEY);
    assert!(has_key(&rt, DOTS_KEY), "a legend press hides nothing");
    assert_eq!(engine.hovered_point(), None);
}

// ---- Hit routing + tooltip ------------------------------------------------------

#[test]
fn pressing_a_point_cell_routes_its_data_row_index() {
    let p = plan(&spread_data(), &config());
    let mut rt = runtime(scatter_scenario(spread_data()), (960.0, 640.0));

    // React 70: onClick(_, index) hands the data row, not the coordinate.
    click_key_center(&mut rt, &cell_for_point(&p, 1));
    assert_eq!(rt.state.clicks, vec![1]);
    click_key_center(&mut rt, &cell_for_point(&p, 2));
    assert_eq!(rt.state.clicks, vec![1, 2]);
    click_key_center(&mut rt, &cell_for_point(&p, 0));
    assert_eq!(rt.state.clicks, vec![1, 2, 0]);

    // The raster tiles the plot: 16 rows × 16 cells.
    // The zone, the 16 row keys and the 16×16 cells all share the prefix.
    assert_eq!(
        keys_with(&rt, HIT_CELL_PREFIX).len(),
        HIT_GRID * HIT_GRID + HIT_GRID + 1
    );
    assert!(has_key(&rt, HIT_ZONE_KEY));
}

#[test]
fn a_cell_between_dots_routes_nothing() {
    let p = plan(&spread_data(), &config());
    // The top-left corner of the plot is far from every point of this fixture.
    assert_eq!(dot_at(&p, 0.03125, 0.03125), None);
    let mut rt = runtime(scatter_scenario(spread_data()), (960.0, 640.0));
    let before = rt.state.clicks.len();
    click_key_center(&mut rt, &format!("{HIT_CELL_PREFIX}0-0"));
    assert_eq!(rt.state.clicks.len(), before, "no owner → no onClick");
}

#[test]
fn hovering_a_point_mounts_both_coordinate_lines_the_cursor_and_leaving_clears() {
    let engine = ScatterChartRenderer::new();
    let mut sc = scatter_scenario(spread_data());
    sc.engine = engine.clone();
    let p = plan(&sc.data, &sc.config);
    let cell = cell_for_point(&p, 1);
    let mut rt = runtime(sc, (960.0, 640.0));

    assert!(!has_key(&rt, TOOLTIP_KEY), "no tooltip before the pointer arrives");
    assert!(!has_key(&rt, CURSOR_KEY), "no cursor before the pointer arrives");
    hover_key_center(&mut rt, &cell);
    assert!(has_key(&rt, TOOLTIP_KEY), "the plain contentStyle box");
    assert!(has_key(&rt, CURSOR_KEY), "the dashed cursor → solid crosshair");
    assert_eq!(element(&rt, "scatter-chart-tooltip-item-0").text.as_deref(), Some("x : 8"));
    assert_eq!(element(&rt, "scatter-chart-tooltip-item-1").text.as_deref(), Some("y : 9"));
    assert_eq!(engine.hovered_point(), Some(1), "recharts' activeTooltipIndex");

    // The box lives in the hovered cell, at the declared (2, 8) inset.
    let cell_rect = element(&rt, &cell).rect;
    let tip = element(&rt, TOOLTIP_KEY);
    assert!(
        tip.rect[0] >= cell_rect[0] - 0.5 && tip.rect[0] <= cell_rect[0] + cell_rect[2] + 0.5,
        "anchored at the cell: {:?} in {:?}",
        tip.rect,
        cell_rect
    );
    // Absolute literals (not TOOLTIP_DX / the 8pt DY themselves): a probe
    // flipping either inset must turn this test red.
    assert!(
        (tip.rect[0] - (cell_rect[0] + 2.0)).abs() <= 0.5,
        "the 2pt horizontal inset: {:?} in {:?}",
        tip.rect,
        cell_rect
    );
    assert!((tip.rect[1] - (cell_rect[1] + 8.0)).abs() <= 0.5, "the 8pt inset");

    rt.pointer_leave();
    rt.frame(1.0 / 60.0);
    assert!(!has_key(&rt, TOOLTIP_KEY));
    assert!(!has_key(&rt, CURSOR_KEY));
    assert_eq!(engine.hovered_point(), None);
}

#[test]
fn the_tooltip_uses_the_raw_slot_display_not_the_tick_formatter() {
    let data = vec![point(1_234_567.0, 5.0)];
    let p = plan(&data, &config());
    let cell = cell_for_point(&p, 0);
    let mut rt = runtime(scatter_scenario(data), (960.0, 640.0));
    hover_key_center(&mut rt, &cell);
    assert_eq!(
        element(&rt, "scatter-chart-tooltip-item-0").text.as_deref(),
        Some("x : 1234567"),
        "js_string, not formatCompact's 1.2M"
    );
    assert_eq!(element(&rt, "scatter-chart-tooltip-item-1").text.as_deref(), Some("y : 5"));
}

#[test]
fn an_empty_x_key_column_makes_the_x_tooltip_line_the_value_alone() {
    // config.xAxis = Some("") with an empty-name slot: entry_line drops the
    // ' : ' separator (the mutation that makes the join unconditional dies).
    let data = vec![ChartDataPoint::new()
        .with("", ChartValue::Number(7.0))
        .with("y", ChartValue::Number(3.0))];
    let mut cfg = config();
    cfg.x_axis = Some("".into());
    let p = plan(&data, &cfg);
    let cell = cell_for_point(&p, 0);
    let mut rt = runtime(scenario(cfg, data), (960.0, 640.0));
    hover_key_center(&mut rt, &cell);
    assert_eq!(element(&rt, "scatter-chart-tooltip-item-0").text.as_deref(), Some("7"));
    assert_eq!(element(&rt, "scatter-chart-tooltip-item-1").text.as_deref(), Some("y : 3"));
}

#[test]
fn data_shrinking_under_a_hover_unmounts_the_tooltip() {
    let data = spread_data();
    let p = plan(&data, &config());
    let cell = cell_for_point(&p, 1);
    let mut rt = runtime(scatter_scenario(data), (960.0, 640.0));
    hover_key_center(&mut rt, &cell);
    assert!(has_key(&rt, TOOLTIP_KEY));

    rt.update(|s: &mut ChartState, _tx| {
        s.scenario.data = vec![point(1.0, 1.0)];
    });
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
    assert!(!has_key(&rt, TOOLTIP_KEY), "the gone row unmounts the tooltip");
    assert!(!has_key(&rt, CURSOR_KEY));
}

// ---- Plot frame ------------------------------------------------------------------

#[test]
fn the_grid_axes_dots_and_raster_share_the_plot_rect_and_ticks_hit_their_fractions() {
    let rt = runtime(scatter_scenario(grid_data()), (960.0, 640.0));
    let plot = element(&rt, PLOT_KEY).rect;
    assert!(plot[2] > 200.0 && plot[3] > 200.0, "a real plot: {plot:?}");
    for key in [GRID_KEY, AXES_KEY, DOTS_KEY, HIT_ZONE_KEY] {
        let r = element(&rt, key).rect;
        for i in 0..4 {
            assert!((r[i] - plot[i]).abs() <= 0.5, "{key} dim {i}: {r:?} vs {plot:?}");
        }
    }
    let y_axis = element(&rt, Y_AXIS_KEY).rect;
    assert!((y_axis[2] - 60.0).abs() <= 0.5, "recharts' default YAxis width");
    assert!((y_axis[0] + y_axis[2] - plot[0]).abs() <= 0.5);
    let x_axis = element(&rt, X_AXIS_KEY).rect;
    assert!((x_axis[3] - 30.0).abs() <= 0.5, "recharts' default XAxis height");

    // Inner X ticks (fractions 0.25/0.5/0.75) land exactly at those fractions;
    // the two edge labels are inset half a gap to stay inside the plot.
    let ticks: Vec<rune::ElementInfo> = (0..NUM_TICKS)
        .map(|t| element(&rt, &format!("{X_AXIS_KEY}-tick-{t}")))
        .collect();
    for (t, frac) in [(1usize, 0.25f32), (2, 0.5), (3, 0.75)] {
        assert!(
            (ticks[t].rect[0] + ticks[t].rect[2] * 0.5 - (x_axis[0] + frac * x_axis[2])).abs() <= 2.0,
            "tick {t} centred at {frac}: {:?} vs {x_axis:?}",
            ticks[t].rect
        );
    }
    // Y ticks run top to bottom, the first is the domain max.
    let y_ticks: Vec<rune::ElementInfo> = (0..NUM_TICKS)
        .map(|t| element(&rt, &format!("{Y_AXIS_KEY}-tick-{t}")))
        .collect();
    for w in y_ticks.windows(2) {
        assert!(w[0].rect[1] < w[1].rect[1], "Y ticks run top to bottom");
    }
    assert_eq!(y_ticks[0].text.as_deref(), Some("20"), "the top tick is the Y max");
}

#[test]
fn the_grid_canvas_stays_mounted_at_zero_visibility_when_show_grid_is_false() {
    let mut sc = scatter_scenario(grid_data());
    sc.config.show_grid = false;
    let rt = runtime(sc, (960.0, 640.0));
    assert!(has_key(&rt, GRID_KEY), "a hidden grid is invisible, not unmounted");
}

#[test]
fn an_empty_chart_keeps_an_honest_frame() {
    let p = plan(&[], &config());
    assert_eq!(p.row_count, 0);
    assert!(p.points.is_empty());
    assert_eq!(p.x_domain, [0.0, 1.0]);
    assert_eq!(p.y_domain, [0.0, 1.0]);
    assert_eq!(p.x_ticks.len(), NUM_TICKS);
    assert_eq!(dot_at(&p, 0.5, 0.5), None, "no dots to hit");

    let rt = runtime(scatter_scenario(Vec::new()), (960.0, 640.0));
    assert!(has_key(&rt, ROOT_KEY));
    assert!(has_key(&rt, Y_AXIS_KEY) && has_key(&rt, X_AXIS_KEY));
    assert!(has_key(&rt, LEGEND_KEY), "the single entry still shows");
}

// ==== T261 audit round: mutation-friendly literal pins ==========================

#[test]
fn the_dot_gon_and_the_hit_raster_literals_are_pinned_by_their_geometry() {
    // Absolute literal pins (a relative `disc.len() == DOT_GON_SIDES` would
    // survive a 16→12 flip): a probe changing any of these four constants
    // turns this test red. The generated polygon's opening vertex is checked
    // against the pinned radius as an observable-geometry cross-check.
    let disc = gon([0.5, 0.5], DOT_RADIUS, DOT_GON_SIDES);
    assert_eq!(DOT_GON_SIDES, 16, "16-gon for ZAxis range [40,40]");
    assert_eq!(disc.len(), 16, "the painted disc has 16 vertices");
    assert!((disc[0][0] - (0.5 + DOT_RADIUS)).abs() < 1e-6, "the gon opens at +x");
    assert!((disc[0][1] - 0.5).abs() < 1e-6);
    assert_eq!(DOT_RADIUS, 0.02, "≈20px radius for ZAxis range [40,40]");
    assert_eq!(HIT_GRID, 16);
    assert_eq!(HIT_RADIUS, 0.05, "a forgiving marker pick radius");

    let rt = runtime(scatter_scenario(spread_data()), (960.0, 640.0));
    let cells: Vec<String> = keys_with(&rt, HIT_CELL_PREFIX)
        .into_iter()
        .filter(|k| !k.ends_with("zone") && !k.contains("hit-row-"))
        .collect();
    assert_eq!(cells.len(), HIT_GRID * HIT_GRID, "16×16 raster: {cells:?}");
}

#[test]
fn a_null_row_owns_no_hit_cell_even_near_a_drawable_neighbour() {
    // A point at (2,2) and a null-y row at (2.5, ·): the null row has no dot,
    // so a cell near its x still routes to the drawable point, never a phantom
    // gap row. Probing "null rows still get a point" dies here.
    let data = vec![
        point(2.0, 2.0),
        ChartDataPoint::new()
            .with("x", ChartValue::Number(2.5))
            .with("y", ChartValue::Null),
        point(15.0, 15.0),
    ];
    let p = plan(&data, &config());
    assert_eq!(p.points[1], None);
    for (row, dot) in p.points.iter().enumerate() {
        if let Some([px, py]) = dot {
            assert_ne!(row, 1);
            assert_eq!(dot_at(&p, *px, *py), Some(row));
        }
    }
    // The null row's own x fraction sits between the two dots; it is never
    // returned as an owner (only existing drawable rows are).
    let owners: Vec<usize> = (0..HIT_GRID)
        .flat_map(|r| {
            let p = &p;
            (0..HIT_GRID).filter_map(move |c| {
                dot_at(p, (c as f32 + 0.5) / HIT_GRID as f32, (r as f32 + 0.5) / HIT_GRID as f32)
            })
        })
        .collect();
    assert!(owners.iter().all(|o| *o != 1), "the null row is never hit: {owners:?}");
}

#[test]
fn every_part_stays_inside_the_window_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for compact in [false, true] {
                let mut sc = scatter_scenario(spread_data());
                sc.dark = dark;
                sc.compact = compact;
                sc.height = Some(240.0);
                let rt = runtime(sc, (w, h));
                assert_in_window(
                    &rt,
                    w,
                    h,
                    &format!("scatter dark={dark} compact={compact} {w}x{h}"),
                );
                assert!(has_key(&rt, ROOT_KEY));
            }
        }
    }
}
