//! T102: the `ChartView` container port.
//!
//! Observations come from `rt.elements()` — runtime-measured rects, labels,
//! text and paint — never from what the components report about themselves.
//! The probe owns exactly the state slice React's ChartView kept in
//! `useState` (config, `splitView`, `expanded`, `paletteEpoch` plus the
//! toolbar's lifted `exportOpen`) and the callbacks that mutate it, so a
//! press drives the state and the next frame rebuilds the view from it, as
//! the production chart tab will. The `lib/chart` pipeline is the required
//! trait seam: a fake pipeline stands in for the unported
//! field-inference/recommend/transform tasks and logs what it was asked.

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, text, Cx, El, Env, Runtime};

use crate::components::chart::chart_canvas::{
    AggregationType, ChartConfig, ChartDataPoint, ChartField, ChartRenderContext, ChartRenderer,
    ChartType, InferredFieldType,
};
use crate::components::chart::chart_empty_state::{EMPTY_KEY, EMPTY_TEXT_KEY};
use crate::components::chart::chart_toolbar::ExportFormat;
use crate::components::chart::chart_view::{
    AxisSlot, AxisSlotFn, ChartPipeline, ChartRecommendation, ChartView, ColumnInfo,
    StatementCell, StatementResult, TransformResult, MAX_CHART_ROWS, ROOT_KEY,
};
use crate::components::chart::chart_view::{
    AXIS_SLOT_KEY, AXIS_SLOT_NOTE_KEY, AREA_KEY, OVERLAY_AREA_KEY, OVERLAY_CLOSE_KEY,
    OVERLAY_KEY, OVERLAY_PNG_KEY, OVERLAY_SEP_KEY, OVERLAY_SVG_KEY, OVERLAY_TITLE_KEY,
    SPLIT_GRID_KEY,
};
use crate::i18n::{has, t};
use crate::ui::kit::kit;

// ---- Harness ----------------------------------------------------------------

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
    // An element playing its exit transition is still listed for tools to
    // watch, but it is not mounted (the repo-wide `!e.exiting` convention).
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn count_key<S>(rt: &Runtime<S>, key: &str) -> usize {
    rt.elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .count()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
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

// ---- The fake lib/chart pipeline ---------------------------------------------

/// One renderer observation: what the canvas handed a renderer.
#[derive(Clone, Debug, PartialEq)]
struct RenderObs {
    chart_type: ChartType,
    y_axes: Vec<String>,
    group_by: Option<String>,
    compact: bool,
    has_click: bool,
}

/// One axis-slot observation: the React props the slot received.
#[derive(Clone, Debug, PartialEq)]
struct AxisObs {
    field_count: usize,
    y_axes: Vec<String>,
    recommendation: bool,
    change_wired: bool,
}

/// The stand-in for the three unported `lib/chart` functions: canned outputs
/// plus logs of the calls the view made.
struct FakePipeline {
    fields: Vec<ChartField>,
    recommendation: Option<ChartRecommendation>,
    series_keys: Vec<String>,
    data: Vec<ChartDataPoint>,
    transform_rows: RefCell<Vec<usize>>,
    transform_configs: RefCell<Vec<ChartConfig>>,
    renders: RefCell<Vec<RenderObs>>,
}

impl FakePipeline {
    fn new(fields: Vec<ChartField>) -> Self {
        Self {
            fields,
            recommendation: None,
            series_keys: Vec::new(),
            data: Vec::new(),
            transform_rows: RefCell::new(Vec::new()),
            transform_configs: RefCell::new(Vec::new()),
            renders: RefCell::new(Vec::new()),
        }
    }

    fn with_recommendation(mut self, rec: ChartRecommendation) -> Self {
        self.recommendation = Some(rec);
        self
    }
}

impl ChartPipeline for FakePipeline {
    fn infer_fields(&self, _result: &StatementResult) -> Vec<ChartField> {
        self.fields.clone()
    }

    fn recommend(&self, _fields: &[ChartField], _row_count: usize) -> Option<ChartRecommendation> {
        self.recommendation.clone()
    }

    fn transform(&self, result: &StatementResult, config: &ChartConfig) -> TransformResult {
        self.transform_rows.borrow_mut().push(result.rows.len());
        self.transform_configs.borrow_mut().push(config.clone());
        TransformResult {
            data: self.data.clone(),
            series_keys: self.series_keys.clone(),
        }
    }
}

/// A renderer that records what it was handed and paints an honest marker.
struct RecordingRenderer {
    pipeline: Rc<FakePipeline>,
}

impl<S: 'static> ChartRenderer<S> for RecordingRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        self.pipeline.renders.borrow_mut().push(RenderObs {
            chart_type: ctx.config.chart_type,
            y_axes: ctx.config.y_axes.clone(),
            group_by: ctx.config.group_by.clone(),
            compact: ctx.compact,
            has_click: ctx.on_data_point_click.is_some(),
        });
        let k = kit(ctx.dark);
        col()
            .key("probe-renderer")
            .w_full()
            .h_full()
            .child(
                text("probe")
                    .key("probe-renderer-text")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
    }
}

// ---- The probe ----------------------------------------------------------------

#[derive(Clone)]
struct ViewProbe {
    result: StatementResult,
    pipeline: Rc<FakePipeline>,
    config: ChartConfig,
    dark: bool,
    expanded: bool,
    split_view: bool,
    palette_epoch: u32,
    export_open: bool,
    wire_renderer: bool,
    axis_slot: Option<AxisSlotFn<ViewProbe>>,
    axis_log: Rc<RefCell<Vec<AxisObs>>>,
    changes: Vec<ChartConfig>,
    split_toggles: usize,
    expands: usize,
    closes: usize,
    export_toggles: usize,
    export_closes: usize,
    exports: Vec<ExportFormat>,
    clicks: Vec<usize>,
}

fn field(name: &str, kind: InferredFieldType) -> ChartField {
    ChartField {
        name: name.to_string(),
        data_type: "text".to_string(),
        inferred_type: kind,
        distinct_count: None,
        sample_values: Vec::new(),
    }
}

/// The plottable default: a categorical label and two numeric columns.
fn default_fields() -> Vec<ChartField> {
    vec![
        field("label", InferredFieldType::Categorical),
        field("count", InferredFieldType::Numeric),
        field("sum", InferredFieldType::Numeric),
    ]
}

/// A result with `row_count` rows over the default columns.
fn rows_result(row_count: usize) -> StatementResult {
    StatementResult {
        columns: vec![
            ColumnInfo {
                name: "label".to_string(),
                data_type: "text".to_string(),
            },
            ColumnInfo {
                name: "count".to_string(),
                data_type: "int".to_string(),
            },
        ],
        rows: (0..row_count)
            .map(|i| {
                vec![
                    StatementCell::Text("a".to_string()),
                    StatementCell::Number(i as f64),
                ]
            })
            .collect(),
    }
}

fn make_axis_slot(log: Rc<RefCell<Vec<AxisObs>>>) -> AxisSlotFn<ViewProbe> {
    Rc::new(move |slot: &AxisSlot<ViewProbe>| {
        log.borrow_mut().push(AxisObs {
            field_count: slot.fields.len(),
            y_axes: slot.config.y_axes.clone(),
            recommendation: slot.recommendation.is_some(),
            change_wired: slot.on_change.is_some(),
        });
        let k = kit(true);
        col()
            .key("probe-axis-slot")
            .w_full()
            .h_full()
            .child(
                text("slot")
                    .key("probe-axis-slot-text")
                    .font(k.ty.text_small)
                    .color(k.c.text_muted),
            )
    })
}

fn probe(result: StatementResult, fields: Vec<ChartField>) -> ViewProbe {
    // The caller wiring: the no-saved-config derivation of the default
    // fields (the first numeric column) — `sync_config`'s default path.
    let derived = crate::components::chart::chart_view::default_config(&default_fields());
    let axis_log = Rc::new(RefCell::new(Vec::new()));
    ViewProbe {
        result,
        pipeline: Rc::new(FakePipeline::new(fields)),
        config: derived,
        dark: true,
        expanded: false,
        split_view: false,
        palette_epoch: 0,
        export_open: false,
        wire_renderer: false,
        axis_slot: None,
        axis_log,
        changes: Vec::new(),
        split_toggles: 0,
        expands: 0,
        closes: 0,
        export_toggles: 0,
        export_closes: 0,
        exports: Vec::new(),
        clicks: Vec::new(),
    }
}

fn chart_probe_view(s: &ViewProbe, _cx: &mut Cx<ViewProbe>) -> El {
    let mut layer = Vec::new();
    let mut view = ChartView::<ViewProbe>::new(&s.result, &*s.pipeline, &s.config)
        .expanded(s.expanded)
        .split_view(s.split_view)
        .palette_epoch(s.palette_epoch)
        .export_open(s.export_open)
        .on_change(|s: &mut ViewProbe, c| {
            // React's setConfig: the caller sets its state and forwards the
            // onConfigChange notification in the same closure.
            s.config = c.clone();
            s.changes.push(c);
        })
        .on_toggle_split(|s: &mut ViewProbe| {
            s.split_view = !s.split_view;
            s.split_toggles += 1;
        })
        .on_expand(|s: &mut ViewProbe| {
            s.expanded = true;
            s.expands += 1;
        })
        .on_close(|s: &mut ViewProbe| {
            s.expanded = false;
            s.closes += 1;
        })
        .on_export_toggle(|s: &mut ViewProbe| {
            s.export_open = !s.export_open;
            s.export_toggles += 1;
        })
        .on_export_close(|s: &mut ViewProbe| {
            s.export_open = false;
            s.export_closes += 1;
        })
        .on_export(|s: &mut ViewProbe, f| s.exports.push(f))
        .on_data_point_click(|s: &mut ViewProbe, i| s.clicks.push(i));
    if let Some(slot) = s.axis_slot.clone() {
        view = view.axis_configurator(move |ctx: &AxisSlot<ViewProbe>| slot(ctx));
    }
    if s.wire_renderer {
        let pipeline = Rc::clone(&s.pipeline);
        view = view.renderer_factory(move |_t| {
            Box::new(RecordingRenderer {
                pipeline: Rc::clone(&pipeline),
            })
        });
    }
    let page = view.build(s.dark, &mut layer);
    rune_kit::root(page, layer)
}

fn chart_runtime(probe: ViewProbe, size: (f32, f32)) -> Runtime<ViewProbe> {
    let mut rt = Runtime::new(probe, chart_probe_view, size, env(true));
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

/// A probe whose default derivation has already been applied by "the caller"
/// (the derivation's notify forwarding is `sync_config`'s own tests; the
/// `changes` log stays reserved for presses).
fn mounted_probe() -> ViewProbe {
    let mut p = probe(rows_result(3), default_fields());
    let sync = crate::components::chart::chart_view::sync_config(
        None,
        p.pipeline.recommendation.as_ref(),
        &p.pipeline.fields,
    );
    p.config = sync.config;
    p
}

// ---- The three early-return branches ------------------------------------------

#[test]
fn empty_rows_render_the_no_data_pane_and_nothing_else() {
    for dark in [true, false] {
        let mut p = probe(StatementResult::default(), default_fields());
        p.dark = dark;
        let rt = chart_runtime(p, (960.0, 640.0));
        assert!(has_key(&rt, EMPTY_KEY), "the noData pane is the whole view");
        assert_eq!(
            element(&rt, EMPTY_TEXT_KEY).text.as_deref(),
            Some("无数据可视化"),
            "the reason line is the zh pack's noData copy"
        );
        assert_eq!(t("chart.empty.noData"), "无数据可视化");
        // React's early return replaces the whole tree: no root, no toolbar,
        // no axis slot, no chart area, no overlay.
        assert!(!has_key(&rt, ROOT_KEY), "no root strip on early return");
        assert!(!has_key(&rt, "chart-toolbar"));
        assert!(!has_key(&rt, AXIS_SLOT_KEY));
        assert!(!has_key(&rt, AREA_KEY));
        assert!(!has_key(&rt, OVERLAY_KEY));
        assert_in_window(&rt, 960.0, 640.0, "noData pane");
    }
}

#[test]
fn a_result_without_numeric_columns_renders_the_no_numeric_field_pane() {
    let mut p = probe(
        rows_result(2),
        vec![field("label", InferredFieldType::Categorical)],
    );
    p.config = crate::components::chart::chart_view::default_config(&p.pipeline.fields);
    let rt = chart_runtime(p, (960.0, 640.0));
    assert!(has_key(&rt, EMPTY_KEY));
    assert_eq!(
        element(&rt, EMPTY_TEXT_KEY).text.as_deref(),
        Some("需要至少一个数值列才能生成图表"),
        "the reason line is the zh pack's noNumericField copy"
    );
    assert_eq!(t("chart.empty.noNumericField"), "需要至少一个数值列才能生成图表");
    assert!(!has_key(&rt, "chart-toolbar"), "the early return has no toolbar");
    assert!(!has_key(&rt, AXIS_SLOT_KEY));
    assert!(!has_key(&rt, AREA_KEY));
}

#[test]
fn an_empty_y_axis_config_renders_the_no_config_pane_inside_the_chart_area() {
    let mut p = mounted_probe();
    p.config.y_axes = Vec::new(); // React `showEmptyCanvas`
    let rt = chart_runtime(p, (960.0, 640.0));
    assert!(has_key(&rt, ROOT_KEY), "the view keeps its shell");
    assert!(has_key(&rt, "chart-toolbar"), "the toolbar stays mounted");
    assert!(has_key(&rt, AREA_KEY), "the chart area stays mounted");
    assert!(has_key(&rt, EMPTY_KEY), "the noConfig pane mounts in the area");
    assert_eq!(
        element(&rt, EMPTY_TEXT_KEY).text.as_deref(),
        Some("请在左侧配置面板中选择 Y 轴字段"),
        "the reason line is the zh pack's noConfig copy"
    );
    assert_eq!(t("chart.empty.noConfig"), "请在左侧配置面板中选择 Y 轴字段");
    assert!(!has_key(&rt, "chart-view-canvas-0"), "no canvas without a series");
    assert!(!has_key(&rt, SPLIT_GRID_KEY));
    assert_in_window(&rt, 960.0, 640.0, "noConfig pane");
}

#[test]
fn the_early_return_arms_still_evaluate_the_transform_seam_once() {
    // React's memos evaluate above the early returns — hooks must precede
    // them (source #310) — so an empty-arm render still hands the pipeline
    // exactly one transform call, over the capped chartResult, with the
    // live config. The audit pinned `recommendChart`'s call (its pin #5);
    // this pin completes the protocol for `transformData`, which the port
    // skipped on these arms before the fix.
    //
    // Empty rows → the noData arm: the transform sees the empty result.
    let p = probe(StatementResult::default(), default_fields());
    let rt = chart_runtime(p, (960.0, 640.0));
    assert!(has_key(&rt, EMPTY_KEY), "the noData pane is the whole view");
    assert_eq!(
        rt.state.pipeline.transform_rows.borrow().len(),
        1,
        "the transform memo ran once above the early return, as React's hooks must"
    );
    assert_eq!(
        rt.state.pipeline.transform_rows.borrow().last().copied(),
        Some(0),
        "the transform saw the truncated chartResult (0 rows here)"
    );
    assert_eq!(
        rt.state.pipeline.transform_configs.borrow().last().cloned(),
        Some(rt.state.config.clone()),
        "the transform carried the live config the view rendered with"
    );

    // No numeric column, 1001 rows → the noNumericField arm: the cap ran
    // above the return too, so the transform sees 1000, not the uncapped
    // 1001 (the literal, not the imported constant, per the audit's ruling).
    let p = probe(
        rows_result(1001),
        vec![field("label", InferredFieldType::Categorical)],
    );
    let rt = chart_runtime(p, (960.0, 640.0));
    assert!(has_key(&rt, EMPTY_KEY), "the noNumericField pane is the whole view");
    assert_eq!(
        element(&rt, EMPTY_TEXT_KEY).text.as_deref(),
        Some("需要至少一个数值列才能生成图表"),
        "the arm is the noNumericField one"
    );
    assert_eq!(
        rt.state.pipeline.transform_rows.borrow().len(),
        1,
        "one transform call on this arm too, not zero"
    );
    assert_eq!(
        rt.state.pipeline.transform_rows.borrow().last().copied(),
        Some(1000),
        "the transform saw the capped chartResult (1001 rows → 1000)"
    );
    assert_eq!(
        rt.state.pipeline.transform_configs.borrow().last().cloned(),
        Some(rt.state.config.clone()),
        "the transform carried the live config on this arm too"
    );
}

// ---- The config initialisation paths -------------------------------------------

#[test]
fn sync_config_prefers_a_saved_config_without_notifying() {
    let saved = ChartConfig {
        chart_type: ChartType::Pie,
        y_axes: vec!["saved".to_string()],
        ..ChartConfig::default()
    };
    let rec = ChartRecommendation {
        chart_type: ChartType::Line,
        x_axis: Some("label".to_string()),
        y_axes: vec!["count".to_string()],
        group_by: None,
        aggregation: AggregationType::Sum,
    };
    let sync = crate::components::chart::chart_view::sync_config(
        Some(&saved),
        Some(&rec),
        &default_fields(),
    );
    assert_eq!(sync.config, saved, "the saved config wins outright");
    assert!(sync.notify.is_none(), "React's effect never notifies a saved config");
}

#[test]
fn sync_config_derives_from_the_recommendation_and_notifies() {
    let rec = ChartRecommendation {
        chart_type: ChartType::Line,
        x_axis: Some("label".to_string()),
        y_axes: vec!["count".to_string(), "sum".to_string()],
        group_by: Some("region".to_string()),
        aggregation: AggregationType::Avg,
    };
    let sync = crate::components::chart::chart_view::sync_config(None, Some(&rec), &default_fields());
    let derived = sync.config;
    // recommendationToConfig: the recommendation's five fields over the default.
    assert_eq!(derived.chart_type, ChartType::Line);
    assert_eq!(derived.x_axis.as_deref(), Some("label"));
    assert_eq!(
        derived.y_axes,
        vec!["count".to_string(), "sum".to_string()]
    );
    assert_eq!(derived.group_by.as_deref(), Some("region"));
    assert_eq!(derived.aggregation, AggregationType::Avg);
    // ...and the default config's untouched tail.
    assert!(derived.show_legend);
    assert!(derived.show_grid);
    assert!(!derived.show_values);
    assert_eq!(derived.color_scheme, "default");
    let notified = sync.notify.as_ref().unwrap_or_else(|| panic!("a derivation notifies"));
    assert_eq!(*notified, derived, "the notify payload is the derived config");
}

#[test]
fn sync_config_falls_back_to_the_first_numeric_column_and_notifies() {
    let fields = vec![
        field("label", InferredFieldType::Categorical),
        field("first", InferredFieldType::Numeric),
        field("second", InferredFieldType::Numeric),
    ];
    let sync = crate::components::chart::chart_view::sync_config(None, None, &fields);
    assert_eq!(
        sync.config.y_axes,
        vec!["first".to_string()],
        "defaultConfig plots the first numeric column"
    );
    assert_eq!(sync.config.chart_type, ChartType::Bar, "React's default chartType");
    assert!(sync.notify.is_some());

    // No numeric column at all: no series, still a notified derivation.
    let none = crate::components::chart::chart_view::sync_config(
        None,
        None,
        &[field("label", InferredFieldType::Categorical)],
    );
    assert!(none.config.y_axes.is_empty());
    assert!(none.notify.is_some());
}

#[test]
fn the_derived_recommendation_drives_the_rendered_toolbar() {
    // The caller wiring on mount: sync_config over the pipeline's derivation.
    let mut p = probe(rows_result(4), default_fields());
    p.pipeline = Rc::new(
        FakePipeline::new(default_fields()).with_recommendation(ChartRecommendation {
            chart_type: ChartType::Line,
            x_axis: Some("label".to_string()),
            y_axes: vec!["count".to_string()],
            group_by: None,
            aggregation: AggregationType::None,
        }),
    );
    let sync = crate::components::chart::chart_view::sync_config(
        None,
        p.pipeline.recommendation.as_ref(),
        &p.pipeline.fields,
    );
    assert!(sync.notify.is_some(), "the effect path notifies the derivation");
    p.config = sync.config;
    let mut rt = chart_runtime(p, (960.0, 640.0));
    rt.settle_all();
    rt.frame(0.0);
    let k = kit(true);
    // The recommended type is the toolbar's selected chip.
    assert_eq!(
        element(&rt, "chart-type-line").fill,
        k.alpha(k.c.bg_accent, 0.2),
        "the recommended chart type renders as the selected chip"
    );
    assert_eq!(rt.state.changes.len(), 0, "the probe logged the notify itself");
}

// ---- The row cap ----------------------------------------------------------------

#[test]
fn the_row_cap_keeps_1000_rows_and_truncates_1001() {
    for (rows, seen) in [(1000, 1000), (1001, MAX_CHART_ROWS)] {
        let mut p = probe(rows_result(rows), default_fields());
        p.wire_renderer = true;
        let rt = chart_runtime(p, (960.0, 640.0));
        assert!(
            has_key(&rt, "chart-view-canvas-0"),
            "{rows} rows still render a canvas"
        );
        assert_eq!(
            rt.state.pipeline.transform_rows.borrow().last().copied(),
            Some(seen),
            "the transform sees the capped row count ({rows} rows → {seen})"
        );
        assert_eq!(
            rt.state.pipeline.transform_rows.borrow().len(),
            1,
            "one transform per build"
        );
    }
}

// ---- The renderConfig fold -------------------------------------------------------

#[test]
fn a_grouped_config_folds_to_the_pivot_series_for_rendering() {
    // Grouped: the canvas receives the pivot series with the grouping cleared.
    let mut p = mounted_probe();
    p.config.group_by = Some("region".to_string());
    let pipeline = Rc::get_mut(&mut p.pipeline)
        .unwrap_or_else(|| panic!("the probe owns the only pipeline ref"));
    pipeline.series_keys = vec!["A·count".to_string(), "B·count".to_string()];
    p.wire_renderer = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let renders = rt.state.pipeline.renders.borrow();
    assert_eq!(renders.len(), 1, "one canvas render");
    assert_eq!(
        renders[0].y_axes,
        vec!["A·count".to_string(), "B·count".to_string()],
        "renderConfig folds yAxes to the pivot seriesKeys"
    );
    assert_eq!(renders[0].group_by, None, "renderConfig clears the grouping");
    assert!(!renders[0].compact, "the main canvas is not compact");
    assert!(
        renders[0].has_click,
        "the main canvas carries the data-point click"
    );
    drop(renders);

    // Ungrouped: the config passes through untouched.
    let mut p = mounted_probe();
    p.wire_renderer = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let renders = rt.state.pipeline.renders.borrow();
    assert_eq!(
        renders[0].y_axes,
        vec!["count".to_string()],
        "an ungrouped config renders as itself"
    );
    assert_eq!(renders[0].group_by, None);
}

#[test]
fn split_tiles_fold_groupings_to_their_single_original_axis() {
    let mut p = mounted_probe();
    p.config.y_axes = vec!["count".to_string(), "sum".to_string()];
    p.config.group_by = Some("region".to_string());
    let pipeline = Rc::get_mut(&mut p.pipeline)
        .unwrap_or_else(|| panic!("the probe owns the only pipeline ref"));
    pipeline.series_keys = vec!["A·count".to_string(), "B·count".to_string()];
    p.split_view = true;
    p.wire_renderer = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let renders = rt.state.pipeline.renders.borrow();
    assert_eq!(renders.len(), 2, "one render per split tile");
    // Each tile: the original Y axis, single, with the grouping cleared —
    // React's splitConfigs spread renderConfig then set yAxes to [yKey].
    for (i, axis) in ["count", "sum"].iter().enumerate() {
        assert_eq!(renders[i].y_axes, vec![axis.to_string()]);
        assert_eq!(renders[i].group_by, None);
        assert!(renders[i].compact, "the split tiles are compact");
        assert!(renders[i].has_click, "the split tiles carry the click");
    }
}

// ---- The split view --------------------------------------------------------------

fn two_axis_probe() -> ViewProbe {
    let mut p = mounted_probe();
    p.config.y_axes = vec!["count".to_string(), "sum".to_string()];
    let pipeline = Rc::get_mut(&mut p.pipeline)
        .unwrap_or_else(|| panic!("the probe owns the only pipeline ref"));
    pipeline.series_keys = vec!["count".to_string(), "sum".to_string()];
    p
}

#[test]
fn split_view_renders_one_tile_per_y_axis() {
    for dark in [true, false] {
        let mut p = two_axis_probe();
        p.dark = dark;
        p.split_view = true;
        let rt = chart_runtime(p, (960.0, 640.0));
        assert!(has_key(&rt, SPLIT_GRID_KEY), "the split grid mounts");
        assert!(!has_key(&rt, "chart-view-canvas-0"), "no single canvas when split");
        // One tile per Y axis, keyed by the axis (React `key={sc.yAxes[0]}`).
        for axis in ["count", "sum"] {
            let tile = element(&rt, &format!("chart-view-tile-{axis}"));
            assert!(
                (tile.rect[3] - 224.0).abs() <= 0.5,
                "h-56 tiles are 224pt tall: {:?}",
                tile.rect
            );
            // The chip is the axis name at React's `left-2 top-1`.
            let chip = element(&rt, &format!("chart-view-tile-chip-{axis}"));
            assert_eq!(chip.text.as_deref(), Some(axis), "the chip is the axis name");
            assert!(
                (chip.rect[0] - (tile.rect[0] + 8.0)).abs() <= 0.5
                    && (chip.rect[1] - (tile.rect[1] + 4.0)).abs() <= 0.5,
                "left-2 top-1: the chip sits 8pt/4pt inside the tile: tile {:?} chip {:?}",
                tile.rect,
                chip.rect
            );
            // A compact canvas mount per tile, carrying the epoch and axis.
            assert!(has_key(&rt, &format!("chart-view-tile-canvas-0-{axis}")));
        }
        let first = element(&rt, "chart-view-tile-count");
        let second = element(&rt, "chart-view-tile-sum");
        assert!(
            second.rect[0] >= first.rect[0] + first.rect[2] - 0.5,
            "md:grid-cols-2: two axes share one row side by side: {:?} {:?}",
            first.rect,
            second.rect
        );
        let grid = element(&rt, SPLIT_GRID_KEY);
        assert!(
            (grid.rect[3] - 400.0).abs() <= 0.5,
            "minHeight = splitConfigs.length * 200 (2 tiles → 400pt): {:?}",
            grid.rect
        );
        // The grid fills the chart area beside the 208pt axis panel.
        assert!(
            (grid.rect[2] - 752.0).abs() <= 1.0,
            "the grid fills the width beside the 208pt axis panel: {:?}",
            grid.rect
        );
        assert_eq!(
            count_key(&rt, "chart-canvas"),
            2,
            "one canvas element per tile"
        );
        assert_in_window(&rt, 960.0, 640.0, "split view");
    }
}

#[test]
fn split_view_with_a_single_y_axis_stays_unsplit() {
    let p = mounted_probe(); // one y axis: count
    let mut p = p;
    p.split_view = true; // React's `showSplit` gate needs yAxes.length > 1
    let rt = chart_runtime(p, (960.0, 640.0));
    assert!(!has_key(&rt, SPLIT_GRID_KEY), "no tiles for a single axis");
    assert!(has_key(&rt, "chart-view-canvas-0"), "the single canvas stays");
}

#[test]
fn an_unsplit_view_renders_the_single_canvas_with_the_whole_series() {
    let p = two_axis_probe();
    let rt = chart_runtime(p, (960.0, 640.0));
    assert!(!has_key(&rt, SPLIT_GRID_KEY));
    assert!(has_key(&rt, "chart-view-canvas-0"));
    assert_eq!(count_key(&rt, "chart-canvas"), 1);
}

#[test]
fn pressing_the_split_toggle_flips_the_tile_grid() {
    let p = two_axis_probe();
    let mut rt = chart_runtime(p, (960.0, 640.0));
    assert!(!has_key(&rt, SPLIT_GRID_KEY));
    click(&mut rt, "chart-toolbar-split");
    assert_eq!(rt.state.split_toggles, 1, "one press, one toggle");
    assert!(rt.state.split_view, "the caller's state flipped");
    rt.frame(0.0);
    rt.frame(0.0);
    assert!(has_key(&rt, SPLIT_GRID_KEY), "the rebuild picked the tiles up");
    assert_eq!(count_key(&rt, "chart-canvas"), 2);
}

// ---- The expanded overlay --------------------------------------------------------

#[test]
fn the_expanded_overlay_covers_the_window_below_the_top_band() {
    for dark in [true, false] {
        let mut p = two_axis_probe();
        p.dark = dark;
        p.expanded = true;
        let rt = chart_runtime(p, (960.0, 640.0));
        let overlay = element(&rt, OVERLAY_KEY);
        assert!(
            (overlay.rect[1] - 40.0).abs() <= 0.5,
            "top-10: the overlay starts 40pt down: {:?}",
            overlay.rect
        );
        assert!(
            (overlay.rect[1] + overlay.rect[3] - 640.0).abs() <= 0.5,
            "inset-0: the overlay reaches the window's bottom: {:?}",
            overlay.rect
        );
        assert!(
            (overlay.rect[2] - 960.0).abs() <= 0.5,
            "inset-0: the overlay spans the window's width: {:?}",
            overlay.rect
        );
        // The shell below stays mounted, and the overlay canvas (with the
        // renderConfig) joins it.
        assert!(has_key(&rt, ROOT_KEY));
        assert!(has_key(&rt, "chart-view-canvas-0"), "the main canvas stays");
        assert!(has_key(&rt, "chart-view-expand-canvas-0"), "the overlay canvas mounts");
        assert_eq!(count_key(&rt, "chart-canvas"), 2);

        // The title and its key.
        assert!(has("chart.expandTitle"));
        assert_eq!(t("chart.expandTitle"), "图表预览");
        assert_eq!(
            element(&rt, OVERLAY_TITLE_KEY).text.as_deref(),
            Some("图表预览"),
            "the overlay title is the zh pack's expandTitle"
        );
        // The two export buttons and their literal labels.
        assert_eq!(element(&rt, OVERLAY_PNG_KEY).label.as_deref(), Some("PNG"));
        assert_eq!(element(&rt, OVERLAY_SVG_KEY).label.as_deref(), Some("SVG"));
        // The hairline between the exports and the close button.
        let sep = element(&rt, OVERLAY_SEP_KEY);
        assert!(
            (sep.rect[2] - 1.0).abs() <= 0.5 && (sep.rect[3] - 16.0).abs() <= 0.5,
            "h-4 w-px: a 16pt-tall hairline: {:?}",
            sep.rect
        );
        // The close button carries the collapse copy and the Minimize2 glyph.
        assert!(has("chart.collapse"));
        assert_eq!(t("chart.collapse"), "退出放大");
        let close = element(&rt, OVERLAY_CLOSE_KEY);
        assert_eq!(close.label.as_deref(), Some("退出放大"));
        let glyph = element(&rt, &format!("{OVERLAY_CLOSE_KEY}-icon"));
        assert!(
            (glyph.rect[2] - 14.0).abs() <= 0.5 && (glyph.rect[3] - 14.0).abs() <= 0.5,
            "h-3.5 w-3.5: the Minimize2 glyph is 14pt: {:?}",
            glyph.rect
        );
        // The overlay's own chart area and the y-order of the bar row.
        let bar = element(&rt, "chart-view-expand-bar");
        let png = element(&rt, OVERLAY_PNG_KEY);
        let svg = element(&rt, OVERLAY_SVG_KEY);
        let close = element(&rt, OVERLAY_CLOSE_KEY);
        assert!(png.rect[0] < svg.rect[0], "PNG precedes SVG");
        assert!(svg.rect[0] < sep.rect[0], "the hairline follows the exports");
        assert!(sep.rect[0] < close.rect[0], "the close button is last");
        assert!(
            png.rect[1] >= bar.rect[1] && png.rect[1] + png.rect[3] <= bar.rect[1] + bar.rect[3] + 0.5,
            "the buttons sit inside the bar row"
        );
        assert!(has_key(&rt, OVERLAY_AREA_KEY), "the overlay chart area mounts");
        assert_in_window(&rt, 960.0, 640.0, "expanded overlay");
    }
}

#[test]
fn the_overlay_export_buttons_route_to_the_export_seam() {
    let mut p = two_axis_probe();
    p.expanded = true;
    let mut rt = chart_runtime(p, (960.0, 640.0));
    click(&mut rt, OVERLAY_PNG_KEY);
    assert_eq!(rt.state.exports, vec![ExportFormat::Png], "PNG pressed once");
    click(&mut rt, OVERLAY_SVG_KEY);
    assert_eq!(
        rt.state.exports,
        vec![ExportFormat::Png, ExportFormat::Svg],
        "the overlay's two buttons both route to the view's export seam"
    );
    // The seam is request-only: nothing else in the probe moved.
    assert_eq!(rt.state.export_toggles, 0, "the toolbar dropdown was not involved");
}

#[test]
fn the_overlay_close_button_unmounts_the_overlay() {
    let mut p = two_axis_probe();
    p.expanded = true;
    let mut rt = chart_runtime(p, (960.0, 640.0));
    assert!(has_key(&rt, OVERLAY_KEY));
    click(&mut rt, OVERLAY_CLOSE_KEY);
    assert_eq!(rt.state.closes, 1, "one press, one close");
    assert!(!rt.state.expanded, "the caller's state collapsed");
    assert!(!has_key(&rt, OVERLAY_KEY), "the overlay unmounted");
    assert!(has_key(&rt, ROOT_KEY), "the view behind stays mounted");
}

#[test]
fn the_toolbar_expand_button_mounts_the_overlay() {
    let p = two_axis_probe();
    let mut rt = chart_runtime(p, (960.0, 640.0));
    assert!(!has_key(&rt, OVERLAY_KEY));
    click(&mut rt, "chart-toolbar-expand");
    assert_eq!(rt.state.expands, 1, "one press, one expand");
    assert!(rt.state.expanded, "the caller's state expanded");
    assert!(has_key(&rt, OVERLAY_KEY), "the overlay mounted");
}

// ---- The palette epoch -----------------------------------------------------------

#[test]
fn the_palette_epoch_flows_into_every_canvas_mount_key() {
    // Single canvas.
    let mut p = mounted_probe();
    p.palette_epoch = 0;
    let rt = chart_runtime(p, (960.0, 640.0));
    assert!(has_key(&rt, "chart-view-canvas-0"));
    assert!(!has_key(&rt, "chart-view-canvas-1"));

    let mut p = mounted_probe();
    p.palette_epoch = 1;
    let rt = chart_runtime(p, (960.0, 640.0));
    assert!(
        has_key(&rt, "chart-view-canvas-1"),
        "React's key={{paletteEpoch}} remount is the mount key's epoch"
    );
    assert!(!has_key(&rt, "chart-view-canvas-0"), "the old epoch's mount is gone");

    // Split tiles carry the epoch and the axis (React's composite key).
    let mut p = two_axis_probe();
    p.split_view = true;
    p.palette_epoch = 1;
    let rt = chart_runtime(p, (960.0, 640.0));
    assert!(has_key(&rt, "chart-view-tile-canvas-1-count"));
    assert!(has_key(&rt, "chart-view-tile-canvas-1-sum"));
    assert!(!has_key(&rt, "chart-view-tile-canvas-0-count"));

    // The overlay canvas carries the epoch too.
    let mut p = two_axis_probe();
    p.expanded = true;
    p.palette_epoch = 2;
    let rt = chart_runtime(p, (960.0, 640.0));
    assert!(has_key(&rt, "chart-view-expand-canvas-2"));
    assert!(has_key(&rt, "chart-view-canvas-2"), "the main canvas keeps the epoch");
}

// ---- The axis-configurator slot ---------------------------------------------------

#[test]
fn the_default_axis_slot_is_an_honest_placeholder() {
    let p = mounted_probe();
    let rt = chart_runtime(p, (960.0, 640.0));
    let slot = element(&rt, AXIS_SLOT_KEY);
    assert!(
        (slot.rect[2] - 208.0).abs() <= 0.5,
        "w-52: the slot reserves the React panel's 208pt: {:?}",
        slot.rect
    );
    assert_eq!(
        element(&rt, AXIS_SLOT_NOTE_KEY).text.as_deref(),
        Some("AxisConfigurator not ported"),
        "the slot says the configurator is unported, in the canvas family's voice"
    );
    assert!(!has_key(&rt, "probe-axis-slot"));
}

#[test]
fn the_axis_slot_receives_the_react_props_in_both_mounts() {
    let mut p = two_axis_probe();
    let pipeline = Rc::get_mut(&mut p.pipeline)
        .unwrap_or_else(|| panic!("the probe owns the only pipeline ref"));
    pipeline.recommendation = Some(ChartRecommendation {
        chart_type: ChartType::Bar,
        x_axis: Some("label".to_string()),
        y_axes: vec!["count".to_string()],
        group_by: None,
        aggregation: AggregationType::None,
    });
    let log = Rc::clone(&p.axis_log);
    p.axis_slot = Some(make_axis_slot(log));
    let rt = chart_runtime(p, (960.0, 640.0));
    assert_eq!(count_key(&rt, "probe-axis-slot"), 1, "the injected slot renders");
    let logged = rt.state.axis_log.borrow();
    assert_eq!(logged.len(), 1, "one slot call per mount");
    assert_eq!(logged[0].field_count, 3, "the fields the pipeline inferred");
    assert_eq!(logged[0].y_axes, vec!["count".to_string(), "sum".to_string()]);
    assert!(logged[0].recommendation, "the recommendation arrives");
    assert!(logged[0].change_wired, "the setConfig seam arrives with it");
    drop(logged);

    // The overlay mounts the same slot (React mounts AxisConfigurator twice).
    let mut p = two_axis_probe();
    let log = Rc::clone(&p.axis_log);
    p.axis_slot = Some(make_axis_slot(log));
    p.expanded = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    assert_eq!(
        count_key(&rt, "probe-axis-slot"),
        2,
        "the body row and the overlay each mount the configurator"
    );
    let logged = rt.state.axis_log.borrow();
    assert_eq!(logged.len(), 2, "two slot calls, both with the props");
}

// ---- Wiring through the toolbar ----------------------------------------------------

#[test]
fn a_toolbar_type_press_routes_the_replacement_config_to_on_change() {
    let p = mounted_probe();
    let mut rt = chart_runtime(p, (960.0, 640.0));
    click(&mut rt, "chart-type-line");
    assert_eq!(rt.state.changes.len(), 1, "one press, one setConfig");
    let new = rt
        .state
        .changes
        .first()
        .cloned()
        .unwrap_or_else(|| panic!("the press produced a replacement config"));
    assert_eq!(new.chart_type, ChartType::Line);
    assert_eq!(new.y_axes, vec!["count".to_string()], "the rest carries over");
    assert_eq!(rt.state.config.chart_type, ChartType::Line, "the live config followed");
    // And the paint followed: the pressed type is the selected chip.
    rt.settle_all();
    rt.frame(0.0);
    let k = kit(true);
    assert_eq!(
        element(&rt, "chart-type-line").fill,
        k.alpha(k.c.bg_accent, 0.2),
        "the newly selected type paints the accent chip"
    );
}

#[test]
fn the_toolbar_export_dropdown_routes_through_the_view_seam() {
    let p = two_axis_probe();
    let mut rt = chart_runtime(p, (960.0, 640.0));
    click(&mut rt, "chart-export-button");
    assert_eq!(rt.state.export_toggles, 1, "the trigger opened the dropdown");
    assert!(rt.state.export_open);
    click(&mut rt, "chart-export-item0");
    assert_eq!(
        rt.state.exports,
        vec![ExportFormat::Png],
        "the dropdown's PNG entry routes to the view's export seam"
    );
}
