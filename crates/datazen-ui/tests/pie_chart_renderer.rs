//! T238 (the ledger's older numbering says T240): the pie chart engine — the
//! wedge geometry (the angles from the values' shares, clockwise from 12
//! o'clock), the palette by row index, the `showValues` label arms with the
//! `toFixed(0)` rounding, the `showLegend` gate, the hit raster's wedge
//! assignment and the tooltip mount.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::chart::renderers::pie_chart_renderer::{
    pie_chart_plan, renderer_factory, slice_at, wedge_polygon, PieChartRenderer,
    HIT_CELL_PREFIX, HIT_GRID, HIT_ZONE_KEY, LABEL_PREFIX, LABELS_KEY, LEGEND_ITEM_PREFIX,
    LEGEND_KEY, LEGEND_NAME_PREFIX, LEGEND_SWATCH_PREFIX, PIE_KEY, ROOT_KEY, TOOLTIP_KEY,
};
use datazen_ui::components::chart::chart_canvas::{
    renderer_note_key, ChartCanvas, ChartConfig, ChartDataPoint, ChartType, ChartValue,
    PlaceholderChartRenderer,
};
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

fn point(label: &str, value: f64) -> ChartDataPoint {
    ChartDataPoint::new()
        .with("label", ChartValue::Text(label.into()))
        .with("v", ChartValue::Number(value))
}

/// Four slices, 1:1:2:4 — the angles 45°, 45°, 90°, 180°.
fn quarter_data() -> Vec<ChartDataPoint> {
    vec![
        point("alpha", 1.0),
        point("beta", 1.0),
        point("gamma", 2.0),
        point("delta", 4.0),
    ]
}

fn config() -> ChartConfig {
    ChartConfig {
        y_axes: vec!["v".to_string()],
        x_axis: Some("label".into()),
        ..ChartConfig::default()
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
    engine: PieChartRenderer,
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
            engine: PieChartRenderer::new(),
            seen: None,
        }
    }
}

#[derive(Default)]
struct ChartState {
    scenario: Scenario,
    clicks: Vec<usize>,
}

fn pie_view(s: &ChartState, _cx: &mut Cx<ChartState>) -> El {
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
                ChartType::Pie => Box::new(engine.clone()),
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
        pie_view,
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

fn pie_scenario(data: Vec<ChartDataPoint>) -> Scenario {
    let mut config = config();
    config.chart_type = ChartType::Pie;
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
fn the_factory_routes_pie_to_the_engine_and_other_types_to_placeholders() {
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut sc = pie_scenario(quarter_data());
    sc.seen = Some(seen.clone());
    let rt = runtime(sc, (960.0, 640.0));

    assert!(has_key(&rt, ROOT_KEY), "the engine's frame is mounted");
    assert!(
        !has_key(&rt, renderer_note_key(ChartType::Pie)),
        "the Pie placeholder note is gone"
    );
    assert!(
        seen.borrow().iter().all(|t| *t == ChartType::Pie),
        "every build selected the pie engine: {:?}",
        seen.borrow()
    );

    // Another type still gets its honest placeholder through the same factory.
    let seen_line = Rc::new(RefCell::new(Vec::new()));
    let mut line_config = config();
    line_config.chart_type = ChartType::Line;
    let mut sc_line = scenario(line_config, quarter_data());
    sc_line.seen = Some(seen_line.clone());
    let rt_line = runtime(sc_line, (960.0, 640.0));
    assert!(
        has_key(&rt_line, renderer_note_key(ChartType::Line)),
        "Line keeps its placeholder through the pie factory"
    );
    assert!(!has_key(&rt_line, ROOT_KEY), "no pie frame for Line");
}

// ---- The wedge geometry ----------------------------------------------------------

#[test]
fn the_wedge_angles_follow_the_values_shares_from_twelve_oclock_clockwise() {
    let plan = pie_chart_plan(&quarter_data(), &config(), true);

    assert_eq!(plan.y_key.as_deref(), Some("v"), "yAxes[0] is the one series");
    assert_eq!(plan.total, 8.0);

    // 1:1:2:4 over 8 → 45°, 45°, 90°, 180° (radians), clockwise from 12.
    let tau = std::f64::consts::TAU;
    let quarters = [0.125, 0.125, 0.25, 0.5];
    let mut start = 0.0;
    for (i, slice) in plan.slices.iter().enumerate() {
        assert_eq!(slice.row, i);
        assert!((slice.start - start).abs() < 1e-12, "slice {i} starts where the last ended");
        assert!((slice.weight - quarters[i] * tau).abs() < 1e-12, "slice {i} sweep");
        assert!((slice.value.unwrap() - [1.0, 1.0, 2.0, 4.0][i]).abs() < 1e-12);
        start += quarters[i] * tau;
    }

    // The palette by ROW index: five colours wrap.
    let k = kit(true);
    let palette = datazen_ui::components::chart::renderers::area_chart_renderer::series_palette(k);
    assert_eq!(plan.slices[0].color, palette[0]);
    assert_eq!(plan.slices[3].color, palette[3]);

    // slice_at: the angle test over the same spans (probes inside the 0.35
    // radius — the painted pie's extent).
    // 12 o'clock → the first slice's span.
    assert_eq!(slice_at(&plan, 0.5, 0.2), Some(0));
    // 3 o'clock → after 45+45=90° → the third slice (gamma).
    assert_eq!(slice_at(&plan, 0.8, 0.5), Some(2));
    // 6 o'clock → after 45+45+90=180° → the fourth (delta).
    assert_eq!(slice_at(&plan, 0.5, 0.8), Some(3));
    // Outside the 0.35 radius → no Cell.
    assert_eq!(slice_at(&plan, 0.99, 0.99), None);
    assert_eq!(slice_at(&plan, 0.9, 0.5), None, "past the outer radius");
}

#[test]
fn null_and_nan_rows_draw_no_wedge_but_keep_their_palette_slot() {
    let data = vec![
        point("alpha", 1.0),
        ChartDataPoint::new()
            .with("label", ChartValue::Text("null".into()))
            .with("v", ChartValue::Null),
        ChartDataPoint::new()
            .with("label", ChartValue::Text("nan".into()))
            .with("v", ChartValue::Text("n/a".into())),
        point("delta", 3.0),
    ];
    let plan = pie_chart_plan(&data, &config(), true);

    // The null and NaN-parse rows draw no wedge...
    assert_eq!(plan.slices[1].weight, 0.0);
    assert_eq!(plan.slices[1].value, None);
    assert_eq!(plan.slices[2].weight, 0.0);
    // ...but consume their palette slots (React colours by row index).
    let k = kit(true);
    let palette = datazen_ui::components::chart::renderers::area_chart_renderer::series_palette(k);
    assert_eq!(plan.slices[1].color, palette[1]);
    assert_eq!(plan.slices[3].color, palette[3]);

    // The wedge angles skip them: alpha and delta split the circle 1:3.
    assert_eq!(plan.total, 4.0);
    assert!((plan.slices[0].weight - 0.25 * std::f64::consts::TAU).abs() < 1e-12);
    assert!((plan.slices[3].weight - 0.75 * std::f64::consts::TAU).abs() < 1e-12);
    assert_eq!(slice_at(&plan, 0.5, 0.25), Some(0), "12 o'clock is alpha");
    assert_eq!(
        slice_at(&plan, 0.45, 0.75),
        Some(3),
        "the lower-left is delta's 270-degree span"
    );
}

#[test]
fn the_name_reads_the_configured_column_and_the_index_fallback() {
    let mut cfg = config();
    cfg.x_axis = Some("label".into());
    let plan = pie_chart_plan(&quarter_data(), &cfg, true);
    assert_eq!(plan.slices[0].name, "alpha");

    // No xAxis → the '__index' column the transform writes (absent → the
    // empty name, same as the family's tick fallback).
    let no_x = ChartConfig {
        y_axes: vec!["v".to_string()],
        ..ChartConfig::default()
    };
    let plan = pie_chart_plan(&quarter_data(), &no_x, true);
    assert_eq!(plan.name_key, "__index");
    assert_eq!(plan.slices[0].name, "");

    // A numeric nameKey slot reads the JS number display.
    let data = vec![ChartDataPoint::new().with("n", ChartValue::Number(7.0)).with("v", ChartValue::Number(1.0))];
    let mut cfg = config();
    cfg.x_axis = Some("n".into());
    let plan = pie_chart_plan(&data, &cfg, true);
    assert_eq!(plan.slices[0].name, "7");
}

#[test]
fn a_zero_total_or_a_missing_y_axis_is_an_honest_empty_pie() {
    // All-zero values: no wedge, but the rows keep their slots and labels.
    let zeros = vec![point("a", 0.0), point("b", 0.0)];
    let plan = pie_chart_plan(&zeros, &config(), true);
    assert_eq!(plan.total, 0.0);
    assert!(plan.slices.iter().all(|s| s.weight == 0.0));

    // No y axes: no series to plot — every value is the yKey miss.
    let mut cfg = ChartConfig::default();
    cfg.chart_type = ChartType::Pie;
    let plan = pie_chart_plan(&quarter_data(), &cfg, true);
    assert_eq!(plan.y_key, None);
    assert!(plan.slices.iter().all(|s| s.value.is_none()));
    assert_eq!(plan.total, 0.0);

    // Empty data: the honest frame.
    let plan = pie_chart_plan(&[], &config(), true);
    assert_eq!(plan.row_count, 0);
    assert!(plan.slices.is_empty());
}

// ---- The label arms (React 26-32) ---------------------------------------------------

#[test]
fn the_labels_read_the_name_and_the_rounded_percent() {
    let mut cfg = config();
    cfg.show_values = true;
    let plan = pie_chart_plan(&quarter_data(), &cfg, true);
    // 1/8, 1/8, 2/8, 4/8 → 12.5%, 12.5%, 25%, 50%. `toFixed(0)`'s rounding
    // follows Rust (round-half-even: 12.5 → 12), not JS's away-from-zero —
    // the documented T079 stance.
    let labels: Vec<String> = plan.slices.iter().map(|s| s.label.clone().unwrap_or_default()).collect();
    assert_eq!(labels[0], "alpha: 12%");
    assert_eq!(labels[1], "beta: 12%");
    assert_eq!(labels[2], "gamma: 25%");
    assert_eq!(labels[3], "delta: 50%");

    // The toggle off: no labels.
    let plan = pie_chart_plan(&quarter_data(), &config(), true);
    assert!(plan.slices.iter().all(|s| s.label.is_none()));

    // A zero-weight slice still labels with its zero percent.
    let zeros = vec![point("a", 0.0), point("b", 3.0)];
    let mut cfg = config();
    cfg.show_values = true;
    let plan = pie_chart_plan(&zeros, &cfg, true);
    assert_eq!(plan.slices[0].label.as_deref(), Some("a: 0%"));
    assert_eq!(plan.slices[1].label.as_deref(), Some("b: 100%"));

    // The label row mounts under the showValues gate.
    let mut sc = pie_scenario(quarter_data());
    sc.config.show_values = true;
    let rt = runtime(sc, (960.0, 640.0));
    assert!(has_key(&rt, LABELS_KEY));
    assert_eq!(keys_with(&rt, LABEL_PREFIX).len(), 4);
    assert_eq!(element(&rt, "pie-chart-label-0").text.as_deref(), Some("alpha: 12%"));

    let rt_off = runtime(pie_scenario(quarter_data()), (960.0, 640.0));
    assert!(!has_key(&rt_off, LABELS_KEY), "no overlay without showValues");
}

// ---- The legend gate (React 52) ------------------------------------------------------

#[test]
fn the_legend_is_gated_on_show_legend_not_multi_series() {
    let rt = runtime(pie_scenario(quarter_data()), (960.0, 640.0));
    assert!(
        has_key(&rt, LEGEND_KEY),
        "showLegend defaults true and the pie has one series — the gate is not multiSeries"
    );
    assert_eq!(keys_with(&rt, LEGEND_ITEM_PREFIX).len(), 4, "one entry per slice");
    assert_eq!(
        element(&rt, "pie-chart-legend-name-0").text.as_deref(),
        Some("alpha")
    );
    // The swatch carries the slice colour.
    let k = kit(true);
    let palette = datazen_ui::components::chart::renderers::area_chart_renderer::series_palette(k);
    assert_eq!(element(&rt, "pie-chart-legend-swatch-0").fill, palette[0]);

    // The gate off: no legend row.
    let mut sc = pie_scenario(quarter_data());
    sc.config.show_legend = false;
    let rt = runtime(sc, (960.0, 640.0));
    assert!(!has_key(&rt, LEGEND_KEY));
    assert!(has_key(&rt, PIE_KEY), "the pie itself stays");
}

// ---- Geometry ------------------------------------------------------------------------

#[test]
fn the_pie_canvas_and_the_hit_raster_share_the_plot_rect() {
    let rt = runtime(pie_scenario(quarter_data()), (960.0, 640.0));
    let plot = element(&rt, "pie-chart-plot").rect;
    assert!(plot[2] > 200.0 && plot[3] > 200.0, "a real plot region, got {plot:?}");

    let pie = element(&rt, PIE_KEY).rect;
    for i in 0..4 {
        assert!((pie[i] - plot[i]).abs() <= 0.5, "the pie canvas dim {i}: {pie:?} vs {plot:?}");
    }
    let zone = element(&rt, HIT_ZONE_KEY).rect;
    for i in 0..4 {
        assert!((zone[i] - plot[i]).abs() <= 0.5, "the raster dim {i}: {zone:?} vs {plot:?}");
    }

    // The raster tiles the plot: 16 rows of 16 cells (the zone and the row
    // keys share the prefix — the count excludes them).
    let cells: Vec<String> = keys_with(&rt, HIT_CELL_PREFIX)
        .into_iter()
        .filter(|k| {
            !k.ends_with("zone") && !k.contains("hit-row-")
        })
        .collect();
    assert_eq!(cells.len(), HIT_GRID * HIT_GRID, "the 16x16 raster: {cells:?}");
    let row_height = element(&rt, "pie-chart-hit-row-0").rect[3];
    assert!(
        (row_height - plot[3] / HIT_GRID as f32).abs() <= 1.0,
        "the rows grow equally: {row_height:?} vs {plot:?}"
    );
    let cell = element(&rt, "pie-chart-hit-0-0").rect;
    assert!((cell[2] - plot[2] / HIT_GRID as f32).abs() <= 1.0, "{cell:?} vs {plot:?}");
}

#[test]
fn every_part_stays_inside_the_window_in_both_themes() {
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            for compact in [false, true] {
                let mut sc = pie_scenario(quarter_data());
                sc.dark = dark;
                sc.compact = compact;
                sc.height = Some(240.0);
                let rt = runtime(sc, (w, h));
                assert_in_window(
                    &rt,
                    w,
                    h,
                    &format!("pie dark={dark} compact={compact} {w}x{h}"),
                );
                assert!(has_key(&rt, ROOT_KEY));
            }
        }
    }
}

// ---- Hit routing and the tooltip ------------------------------------------------------

#[test]
fn pressing_a_slice_region_routes_its_row_index() {
    // The raster routes by the cell centre's wedge: three cells whose
    // centres sit inside alpha, gamma and delta's spans, and one corner cell
    // outside the pie's radius that routes nothing.
    let mut rt = runtime(pie_scenario(quarter_data()), (960.0, 640.0));

    click_key_center(&mut rt, "pie-chart-hit-2-8"); // the 12 o'clock cell: alpha's span
    assert_eq!(rt.state.clicks, vec![0]);
    click_key_center(&mut rt, "pie-chart-hit-8-13"); // the 3 o'clock cell: gamma's span
    assert_eq!(rt.state.clicks, vec![0, 2]);
    click_key_center(&mut rt, "pie-chart-hit-12-7"); // the lower-left cell: delta's span
    assert_eq!(rt.state.clicks, vec![0, 2, 3]);

    // A cell outside the 0.35 radius has no owner: the press routes nothing.
    let before = rt.state.clicks.len();
    click_key_center(&mut rt, "pie-chart-hit-0-0");
    assert_eq!(rt.state.clicks.len(), before, "the corner cell routes nothing");
}

#[test]
fn hovering_a_slice_mounts_the_tooltip_and_leaving_clears_it() {
    let engine = PieChartRenderer::new();
    let mut sc = pie_scenario(quarter_data());
    sc.engine = engine.clone();
    let mut rt = runtime(sc, (960.0, 640.0));

    assert!(!has_key(&rt, TOOLTIP_KEY), "no tooltip before the pointer arrives");
    hover_key_center(&mut rt, "pie-chart-hit-8-13"); // the gamma cell
    assert!(has_key(&rt, TOOLTIP_KEY), "the plain contentStyle box is mounted");
    assert_eq!(
        element(&rt, "pie-chart-tooltip-item-0").text.as_deref(),
        Some("gamma : 2")
    );
    assert_eq!(engine.hovered_slice(), Some(2), "recharts' activeIndex");

    // The tooltip lives inside the hovered cell (the 3 o'clock point's
    // fractions 0.815/0.5 land in column 13, row 8), at the declared inset.
    let cell = element(&rt, &format!("{HIT_CELL_PREFIX}8-13")).rect;
    let tip = element(&rt, TOOLTIP_KEY).rect;
    assert!(
        tip[0] >= cell[0] - 0.5 && tip[0] <= cell[0] + cell[2] + 0.5,
        "anchored at the cell: {tip:?} in {cell:?}"
    );
    assert!(
        (tip[1] - (cell[1] + 8.0)).abs() <= 0.5,
        "the 8pt vertical inset: {tip:?} in {cell:?}"
    );

    // Leaving the chart clears it.
    rt.pointer_leave();
    rt.frame(1.0 / 60.0);
    assert!(!has_key(&rt, TOOLTIP_KEY));
    assert_eq!(engine.hovered_slice(), None);
}

#[test]
fn data_shrinking_under_a_hover_unmounts_the_tooltip() {
    let mut rt = runtime(pie_scenario(quarter_data()), (960.0, 640.0));
    let plot = element(&rt, "pie-chart-plot").rect;
    let (cx, cy) = (plot[0] + plot[2] * 0.5, plot[1] + plot[3] * 0.5);
    let radius = plot[2].min(plot[3]) * 0.7 * 0.5;
    hover_at(&mut rt, cx + radius * 0.9, cy);
    assert!(has_key(&rt, TOOLTIP_KEY));

    // The data drops to one row: the hovered row 2 is gone.
    rt.update(|s: &mut ChartState, _tx| {
        s.scenario.data = vec![point("solo", 1.0)];
    });
    for _ in 0..24 {
        rt.frame(1.0 / 60.0);
    }
    assert!(!has_key(&rt, TOOLTIP_KEY), "the gone row unmounts the tooltip");
}

// ==== T238 audit round ==========================================================
//
// The pins below close mutation survivors: every probe mutation survived the
// port's first 12 tests red-handed and now dies on its pin. The React
// positions cite
// `packages/frontend/src/components/chart/renderers/PieChartRenderer.tsx`.

#[test]
fn the_wedge_arc_starts_at_twelve_oclock_and_sweeps_clockwise() {
    // React 22-24 (cx/cy 50%, outerRadius 70%) with recharts' default
    // startAngle 90/sweep: the arc's first vertex sits at 12 o'clock and the
    // sweep runs clockwise. The probe mutation dropping the -90° rotation
    // survived (the painted rotation is canvas-interior); the polygon is the
    // observable geometry.
    let tau = std::f64::consts::TAU;
    let full = wedge_polygon(0.0, tau);
    assert_eq!(full[0], [0.5, 0.5], "the centre vertex");
    assert!((full[1][0] - 0.5).abs() < 1e-6 && (full[1][1] - 0.15).abs() < 1e-6,
        "the arc opens at 12 o'clock (0.5, 0.5-0.35): {:?}", full[1]);
    assert_eq!(full[full.len() - 1], full[1], "the full circle closes where it opened");

    // A quarter wedge sweeps clockwise: from 12 to 3 o'clock.
    let quarter = wedge_polygon(0.0, tau / 4.0);
    let last = quarter[quarter.len() - 1];
    assert!((last[0] - 0.85).abs() < 1e-6 && (last[1] - 0.5).abs() < 1e-6,
        "the quarter ends at 3 o'clock (0.5+0.35, 0.5): {last:?}");

    // A mid-pie wedge starts where the last ended (the 45° offset).
    let second = wedge_polygon(tau / 8.0, tau / 8.0);
    assert!((second[1][0] - 0.7474874_f32).abs() < 1e-4,
        "the second wedge's opener sits at 1:30 (cos 45°): {:?}", second[1]);
}

#[test]
fn the_legend_declares_its_22pt_row_and_8x8_swatches() {
    // The port's declared chrome literals (React's wrapperStyle fontSize 12
    // row; the recharts default swatch chip) — the probe mutations LEGEND_H
    // 22→30 and SWATCH 8→12 both survived the suite.
    let rt = runtime(pie_scenario(quarter_data()), (960.0, 640.0));
    let legend = element(&rt, LEGEND_KEY).rect;
    assert!((legend[3] - 22.0).abs() <= 0.5, "the legend row is 22pt: {legend:?}");
    let swatch = element(&rt, "pie-chart-legend-swatch-0").rect;
    assert!(
        (swatch[2] - 8.0).abs() <= 0.5 && (swatch[3] - 8.0).abs() <= 0.5,
        "the swatch chip is 8x8: {swatch:?}"
    );
}

#[test]
fn an_unnamed_slice_tooltip_shows_the_value_alone() {
    // recharts' default tooltip line is the name and value joined by
    // ' : ' — a nameless slice (the __index fallback's absent column) is the
    // value alone (the probe mutation making the join unconditional
    // survived: every fixture had a name).
    let mut cfg = config();
    cfg.x_axis = None; // the __index fallback: no such column → empty names
    cfg.chart_type = ChartType::Pie;
    let mut sc = scenario(cfg, quarter_data());
    sc.engine = PieChartRenderer::new();
    let mut rt = runtime(sc, (960.0, 640.0));
    hover_key_center(&mut rt, "pie-chart-hit-8-13"); // the gamma cell
    assert_eq!(
        element(&rt, "pie-chart-tooltip-item-0").text.as_deref(),
        Some("2"),
        "the unnamed slice shows the value alone"
    );
}
