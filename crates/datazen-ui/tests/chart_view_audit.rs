//! T102 independent audit: the `ChartView` container port, round two.
//!
//! The porter's own `chart_view.rs` suite (23 tests) was mutation-tested
//! first: every mutant below survived it red-handed, and each pin here is
//! written to kill its mutant. The observations stay tree-observed
//! (`rt.elements()` rects, text, labels, fill, radius, text colour) plus the
//! fake pipeline's call logs — never a value the view reports about itself.
//!
//! Pinned React behaviours (all anchors are
//! `packages/frontend/src/components/chart/ChartView.tsx`):
//!
//! * `MAX_CHART_ROWS` is the literal 1000 (line 43) — the transform sees
//!   exactly 1000 rows from 1000/1001/1002 input rows (96-100), while
//!   `recommendChart` sees the UNCAPPED count (55-58), and the capped result
//!   keeps the result's columns (`...result`, 97-99).
//! * the early-return ladder's ORDER: `rows.length === 0` (131-133) outranks
//!   the no-numeric-field check (135-137), and the memoised
//!   `inferAllFields`/`recommendChart`/`transformData` calls happen above the
//!   returns (54-108) — so even the empty arm asks the pipeline once.
//! * an expanded view with an empty result renders only the empty pane: the
//!   early return replaces the whole tree, portal included (131-137, 190-205).
//! * the overlay canvas receives the FOLDED `renderConfig` (110-113, 281)
//!   and NO `onDataPointClick` (281) — while the main canvas keeps the click
//!   (180-185); every canvas receives the same full `data` (169-186, 281).
//! * the overlay paints `bg-surface/95` (242), its bar is `px-4 py-2 gap-2`
//!   (244) with the `text-sm font-medium` `text-fg` title (245), the literal
//!   `PNG`/`SVG` captions in `text-xs text-fg-muted` (247-260) and the
//!   `mx-1 h-4 w-px` hairline (261).
//! * the split grid is `gap-2 p-2 md:grid-cols-2` (161) with `rounded` tiles
//!   (165) and the `text-[11px] font-medium text-fg-muted` chip (166-168).
//! * the axis panel slot receives the LIVE `config` — grouping intact — not
//!   the folded render config, in both mounts (150-155, 274-279).
//! * the toolbar is fed the live `splitView` (146) and the export dropdown's
//!   dismissal routes through the view's own seam (the T059 strip's
//!   `onClose` wiring).

use std::cell::RefCell;
use std::rc::Rc;

use rune::{col, text, Cx, El, Env, Runtime};

use datazen_ui::components::chart::chart_canvas::{
    AggregationType, ChartConfig, ChartDataPoint, ChartField, ChartRenderContext, ChartRenderer,
    ChartType, ChartValue, InferredFieldType,
};
use datazen_ui::components::chart::chart_empty_state::{EMPTY_KEY, EMPTY_TEXT_KEY};
use datazen_ui::components::chart::chart_toolbar::ExportFormat;
use datazen_ui::components::chart::chart_view::{
    AxisSlot, AxisSlotFn, ChartPipeline, ChartRecommendation, ChartView, ColumnInfo, StatementCell,
    StatementResult, TransformResult, AXIS_SLOT_KEY, AXIS_SLOT_NOTE_KEY, AREA_KEY, OVERLAY_KEY,
    OVERLAY_PNG_KEY, OVERLAY_SEP_KEY, OVERLAY_SVG_KEY, OVERLAY_TITLE_KEY, ROOT_KEY, SPLIT_GRID_KEY,
};
use datazen_ui::i18n::{has, t};
use datazen_ui::ui::kit::kit;

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
    rt.elements()
        .iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(0.0);
    rt.frame(0.0);
}

/// One renderer observation: what the canvas handed a renderer, including the
/// data slice — the audit's addition over the porter's `RenderObs`.
#[derive(Clone, Debug, PartialEq)]
struct RenderObs {
    chart_type: ChartType,
    y_axes: Vec<String>,
    group_by: Option<String>,
    compact: bool,
    has_click: bool,
    data_len: usize,
}

/// One axis-slot observation: the React props the slot received, including the
/// grouping — the audit's addition over the porter's `AxisObs`.
#[derive(Clone, Debug, PartialEq)]
struct AxisObs {
    field_count: usize,
    y_axes: Vec<String>,
    group_by: Option<String>,
    recommendation: bool,
    change_wired: bool,
}

/// The stand-in pipeline: canned outputs plus logs of every call — including
/// the row counts `recommend` was asked about, and the column count the
/// transform's result carried.
struct AuditPipeline {
    fields: Vec<ChartField>,
    recommendation: Option<ChartRecommendation>,
    series_keys: Vec<String>,
    data: Vec<ChartDataPoint>,
    recommend_rows: RefCell<Vec<usize>>,
    transform_rows: RefCell<Vec<usize>>,
    transform_columns: RefCell<Vec<usize>>,
    renders: RefCell<Vec<RenderObs>>,
}

impl ChartPipeline for AuditPipeline {
    fn infer_fields(&self, _result: &StatementResult) -> Vec<ChartField> {
        self.fields.clone()
    }

    fn recommend(&self, _fields: &[ChartField], row_count: usize) -> Option<ChartRecommendation> {
        self.recommend_rows.borrow_mut().push(row_count);
        self.recommendation.clone()
    }

    fn transform(&self, result: &StatementResult, _config: &ChartConfig) -> TransformResult {
        self.transform_rows.borrow_mut().push(result.rows.len());
        self.transform_columns
            .borrow_mut()
            .push(result.columns.len());
        TransformResult {
            data: self.data.clone(),
            series_keys: self.series_keys.clone(),
        }
    }
}

/// A renderer that records what it was handed and paints an honest marker.
struct RecordingRenderer {
    pipeline: Rc<AuditPipeline>,
}

impl<S: 'static> ChartRenderer<S> for RecordingRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        self.pipeline.renders.borrow_mut().push(RenderObs {
            chart_type: ctx.config.chart_type,
            y_axes: ctx.config.y_axes.clone(),
            group_by: ctx.config.group_by.clone(),
            compact: ctx.compact,
            has_click: ctx.on_data_point_click.is_some(),
            data_len: ctx.data.len(),
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
struct AuditProbe {
    result: StatementResult,
    pipeline: Rc<AuditPipeline>,
    config: ChartConfig,
    dark: bool,
    expanded: bool,
    split_view: bool,
    palette_epoch: u32,
    export_open: bool,
    wire_renderer: bool,
    axis_slot: Option<AxisSlotFn<AuditProbe>>,
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

/// Five plot rows — the data handoff pins need real points to count.
fn data_points(n: usize) -> Vec<ChartDataPoint> {
    (0..n)
        .map(|i| {
            ChartDataPoint::new()
                .with("label", ChartValue::Text(format!("r{i}")))
                .with("count", ChartValue::Number(i as f64))
                .with("sum", ChartValue::Number(i as f64 * 2.0))
        })
        .collect()
}

fn make_axis_slot(log: Rc<RefCell<Vec<AxisObs>>>) -> AxisSlotFn<AuditProbe> {
    Rc::new(move |slot: &AxisSlot<AuditProbe>| {
        log.borrow_mut().push(AxisObs {
            field_count: slot.fields.len(),
            y_axes: slot.config.y_axes.clone(),
            group_by: slot.config.group_by.clone(),
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

fn probe(result: StatementResult, fields: Vec<ChartField>) -> AuditProbe {
    let derived = datazen_ui::components::chart::chart_view::default_config(&default_fields());
    let axis_log = Rc::new(RefCell::new(Vec::new()));
    AuditProbe {
        result,
        pipeline: Rc::new(AuditPipeline {
            fields,
            recommendation: None,
            series_keys: Vec::new(),
            data: Vec::new(),
            recommend_rows: RefCell::new(Vec::new()),
            transform_rows: RefCell::new(Vec::new()),
            transform_columns: RefCell::new(Vec::new()),
            renders: RefCell::new(Vec::new()),
        }),
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

/// A probe whose default derivation has already been applied by "the caller".
fn mounted_probe() -> AuditProbe {
    let mut p = probe(rows_result(3), default_fields());
    let sync = datazen_ui::components::chart::chart_view::sync_config(
        None,
        p.pipeline.recommendation.as_ref(),
        &p.pipeline.fields,
    );
    p.config = sync.config;
    p
}

/// Two Y axes, the split-ready default.
fn two_axis_probe() -> AuditProbe {
    let mut p = mounted_probe();
    p.config.y_axes = vec!["count".to_string(), "sum".to_string()];
    let pipeline = Rc::get_mut(&mut p.pipeline)
        .unwrap_or_else(|| panic!("the probe owns the only pipeline ref"));
    pipeline.series_keys = vec!["count".to_string(), "sum".to_string()];
    p
}

/// A grouped two-axis probe: `groupBy: "region"` with two pivot series keys
/// and five data points — the config-fold and data-handoff pins' fixture.
fn grouped_probe() -> AuditProbe {
    let mut p = mounted_probe();
    p.config.y_axes = vec!["count".to_string(), "sum".to_string()];
    p.config.group_by = Some("region".to_string());
    let pipeline = Rc::get_mut(&mut p.pipeline)
        .unwrap_or_else(|| panic!("the probe owns the only pipeline ref"));
    pipeline.series_keys = vec!["A·count".to_string(), "B·count".to_string()];
    pipeline.data = data_points(5);
    pipeline.recommendation = Some(ChartRecommendation {
        chart_type: ChartType::Bar,
        x_axis: Some("label".to_string()),
        y_axes: vec!["count".to_string()],
        group_by: Some("region".to_string()),
        aggregation: AggregationType::Sum,
    });
    p
}

fn audit_probe_view(s: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    let mut layer = Vec::new();
    let mut view = ChartView::<AuditProbe>::new(&s.result, &*s.pipeline, &s.config)
        .expanded(s.expanded)
        .split_view(s.split_view)
        .palette_epoch(s.palette_epoch)
        .export_open(s.export_open)
        .on_change(|s: &mut AuditProbe, c| {
            s.config = c.clone();
            s.changes.push(c);
        })
        .on_toggle_split(|s: &mut AuditProbe| {
            s.split_view = !s.split_view;
            s.split_toggles += 1;
        })
        .on_expand(|s: &mut AuditProbe| {
            s.expanded = true;
            s.expands += 1;
        })
        .on_close(|s: &mut AuditProbe| {
            s.expanded = false;
            s.closes += 1;
        })
        .on_export_toggle(|s: &mut AuditProbe| {
            s.export_open = !s.export_open;
            s.export_toggles += 1;
        })
        .on_export_close(|s: &mut AuditProbe| {
            s.export_open = false;
            s.export_closes += 1;
        })
        .on_export(|s: &mut AuditProbe, f| s.exports.push(f))
        .on_data_point_click(|s: &mut AuditProbe, i| s.clicks.push(i));
    if let Some(slot) = s.axis_slot.clone() {
        view = view.axis_configurator(move |ctx: &AxisSlot<AuditProbe>| slot(ctx));
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

fn chart_runtime(probe: AuditProbe, size: (f32, f32)) -> Runtime<AuditProbe> {
    let mut rt = Runtime::new(probe, audit_probe_view, size, env(true));
    rt.frame(0.0);
    rt.frame(0.0);
    rt
}

// ---- The row cap, its arguments and its payload ---------------------------------

#[test]
fn the_row_cap_is_the_literal_1000_and_recommend_sees_the_uncapped_count() {
    // React `MAX_CHART_ROWS = 1000` (ChartView.tsx:43) slices `rows` to
    // exactly 1000 (96-100); `recommendChart(fields, result.rows.length)`
    // runs over the UNCAPPED length (55-58); the capped result spreads
    // `...result`, so its columns survive (97-99).
    for rows in [1000, 1001, 1002] {
        let p = probe(rows_result(rows), default_fields());
        let rt = chart_runtime(p, (960.0, 640.0));
        let recommend_rows = rt.state.pipeline.recommend_rows.borrow();
        assert_eq!(
            recommend_rows.last().copied(),
            Some(rows),
            "recommendChart sees the uncapped row count ({rows} rows)"
        );
        assert_eq!(
            recommend_rows.len(),
            1,
            "one recommend call per mount, like the transform"
        );
        let transform_rows = rt.state.pipeline.transform_rows.borrow();
        assert_eq!(
            transform_rows.last().copied(),
            Some(1000),
            "transformData sees exactly 1000 rows ({rows} input rows → the literal cap)"
        );
        let transform_columns = rt.state.pipeline.transform_columns.borrow();
        assert_eq!(
            transform_columns.last().copied(),
            Some(2),
            "the capped result keeps the result's columns"
        );
    }
}

// ---- The early-return ladder ----------------------------------------------------

#[test]
fn empty_rows_outrank_a_missing_numeric_column() {
    // React checks `result.rows.length === 0` first (131-133) and the
    // no-numeric-field branch second (135-137): a result that trips BOTH
    // renders the noData pane. The memoised pipeline calls sit above the
    // returns (54-58), so the empty arm still asks the pipeline once.
    let mut p = probe(StatementResult::default(), vec![field("label", InferredFieldType::Categorical)]);
    p.config = datazen_ui::components::chart::chart_view::default_config(&p.pipeline.fields);
    let rt = chart_runtime(p, (960.0, 640.0));
    assert_eq!(
        element(&rt, EMPTY_TEXT_KEY).text.as_deref(),
        Some("无数据可视化"),
        "both gates trip: the noData arm wins React's ladder order"
    );
    assert!(!has_key(&rt, ROOT_KEY));
    assert!(!has_key(&rt, "chart-toolbar"));
    assert!(!has_key(&rt, AREA_KEY));
    assert_eq!(
        rt.state.pipeline.recommend_rows.borrow().len(),
        1,
        "the recommendation memo ran above the early returns, as React's hooks must"
    );
    assert_eq!(
        t("chart.empty.noData"),
        "无数据可视化",
        "the pane is the noData copy, not the noNumericField one"
    );
    assert_ne!(
        t("chart.empty.noNumericField"),
        t("chart.empty.noData"),
        "the two copies differ, so the assert above really discriminates"
    );
}

#[test]
fn an_expanded_empty_result_mounts_no_overlay() {
    // React's early return replaces the whole tree — the expand portal is
    // inside the main return (190-205), so an expanded-but-empty view renders
    // only the empty pane, portal included. Both empty arms behave this way.
    let arms: Vec<(
        StatementResult,
        Vec<ChartField>,
        &str,
    )> = vec![
        (
            StatementResult::default(),
            default_fields(),
            "无数据可视化",
        ),
        (
            rows_result(2),
            vec![field("label", InferredFieldType::Categorical)],
            "需要至少一个数值列才能生成图表",
        ),
    ];
    for (result, fields, copy) in arms {
        let mut p = probe(result, fields);
        p.expanded = true;
        let rt = chart_runtime(p, (960.0, 640.0));
        assert!(has_key(&rt, EMPTY_KEY), "the empty pane is the whole view");
        assert_eq!(element(&rt, EMPTY_TEXT_KEY).text.as_deref(), Some(copy));
        assert!(
            !has_key(&rt, OVERLAY_KEY),
            "the early return swallows the expanded overlay"
        );
        assert!(!has_key(&rt, ROOT_KEY));
    }
}

// ---- The overlay canvas seams ----------------------------------------------------

#[test]
fn the_overlay_canvas_renders_the_folded_config_without_a_click_handler() {
    // React's overlay canvas is `<ChartCanvas key={paletteEpoch} data={data}
    // config={renderConfig} />` (281): the FOLDED config (grouping cleared,
    // yAxes = the pivot series, 110-113) and no `onDataPointClick` — while
    // the main canvas behind it carries the click (180-185).
    let mut p = grouped_probe();
    p.expanded = true;
    p.wire_renderer = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let renders = rt.state.pipeline.renders.borrow();
    assert_eq!(renders.len(), 2, "the main canvas and the overlay canvas");
    assert_eq!(
        renders[0].y_axes,
        vec!["A·count".to_string(), "B·count".to_string()],
        "the main canvas renders the folded series"
    );
    assert_eq!(renders[0].group_by, None);
    assert!(renders[0].has_click, "the main canvas carries the click");
    assert_eq!(
        renders[1].y_axes,
        vec!["A·count".to_string(), "B·count".to_string()],
        "the overlay canvas also renders the FOLDED config — the live config's \
         [\"count\", \"sum\"] with the grouping would be a deviation"
    );
    assert_eq!(renders[1].group_by, None);
    assert!(
        !renders[1].has_click,
        "React's overlay canvas wires no onDataPointClick"
    );
    assert!(!renders[1].compact, "the overlay canvas is not compact");
}

#[test]
fn every_canvas_mounts_the_full_transformed_data() {
    // React hands `data={data}` — the one transformed series — to the split
    // tiles (169-175), the single canvas (180-185) and the overlay (281):
    // every canvas sees the same rows.
    let mut p = grouped_probe();
    p.split_view = true;
    p.expanded = true;
    p.wire_renderer = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let renders = rt.state.pipeline.renders.borrow();
    assert_eq!(renders.len(), 3, "two tiles plus the overlay canvas");
    for (i, r) in renders.iter().enumerate() {
        assert_eq!(r.data_len, 5, "canvas {i} sees the full transformed data");
    }
    assert!(renders[0].compact && renders[1].compact, "tiles are compact");
    assert!(!renders[2].compact, "the overlay canvas is not compact");
}

// ---- The overlay's paint ----------------------------------------------------------

#[test]
fn the_overlay_paints_the_95_percent_surface_over_the_window() {
    // React `bg-surface/95` (242): the overlay's fill is the base surface at
    // 95% alpha, in both appearances (the `backdrop-blur` half of the class
    // is the port's documented drop).
    for dark in [true, false] {
        let mut p = two_axis_probe();
        p.dark = dark;
        p.expanded = true;
        let rt = chart_runtime(p, (960.0, 640.0));
        let k = kit(dark);
        assert_eq!(
            element(&rt, OVERLAY_KEY).fill,
            k.alpha(k.c.bg_base, 0.95),
            "bg-surface/95: the base surface at 95% alpha ({dark})"
        );
    }
}

#[test]
fn the_overlay_bar_geometry_and_typography_are_verbatim() {
    // React's overlay bar (244): `px-4 py-2 gap-2 border-b border-edge` with
    // the `text-sm font-medium` `text-fg` title (245) and the `mx-1 h-4 w-px`
    // hairline before the close button (261).
    let mut p = two_axis_probe();
    p.expanded = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let k = kit(true);
    let bar = element(&rt, "chart-view-expand-bar");
    let title = element(&rt, OVERLAY_TITLE_KEY);
    let png = element(&rt, OVERLAY_PNG_KEY);
    let svg = element(&rt, OVERLAY_SVG_KEY);
    let sep = element(&rt, OVERLAY_SEP_KEY);
    let close = element(&rt, "chart-view-expand-close");
    // px-4: the row's 16pt side padding, both sides.
    assert!(
        (title.rect[0] - bar.rect[0] - 16.0).abs() <= 0.5,
        "px-4: the title starts 16pt inside the bar: {:?} {:?}",
        bar.rect,
        title.rect
    );
    assert!(
        ((bar.rect[0] + bar.rect[2]) - (close.rect[0] + close.rect[2]) - 16.0).abs() <= 0.5,
        "px-4: the close button ends 16pt before the bar's right edge"
    );
    // py-2: the row's 8pt top/bottom padding. The tallest child is the
    // captioned export button (`py-1` + the 18pt text-xs line = 26pt).
    assert!(
        (png.rect[1] - bar.rect[1] - 8.0).abs() <= 0.5,
        "py-2: the tallest child starts 8pt below the bar's top: bar {:?} png {:?}",
        bar.rect,
        png.rect
    );
    assert!(
        ((bar.rect[1] + bar.rect[3]) - (png.rect[1] + png.rect[3]) - 8.0).abs() <= 0.5,
        "py-2: the tallest child ends 8pt above the bar's bottom"
    );
    // The export button's own `py-1`: 4pt + 18pt caption + 4pt = 26pt.
    assert!(
        (png.rect[3] - 26.0).abs() <= 0.5,
        "py-1 + text-xs: the captioned button is 26pt tall: {:?}",
        png.rect
    );
    // The close button's own `py-1` around the 14pt glyph = 22pt.
    assert!(
        (close.rect[3] - 22.0).abs() <= 0.5,
        "py-1 + h-3.5: the glyph-only close button is 22pt tall: {:?}",
        close.rect
    );
    // gap-2 between the bar's children, and mx-1 around the hairline.
    assert!(
        (svg.rect[0] - (png.rect[0] + png.rect[2]) - 8.0).abs() <= 0.5,
        "gap-2: 8pt between the PNG and SVG buttons"
    );
    // mx-1 around the hairline: the hairline's border box sits its own 4pt
    // margin past the row's 8pt gap (12pt total), and the close button
    // another 4pt margin + 8pt gap past it.
    assert!(
        (sep.rect[0] - (svg.rect[0] + svg.rect[2]) - 12.0).abs() <= 0.5,
        "gap-2 + mx-1: the hairline starts 12pt past the SVG button: svg {:?} sep {:?}",
        svg.rect,
        sep.rect
    );
    assert!(
        (close.rect[0] - (sep.rect[0] + sep.rect[2]) - 12.0).abs() <= 0.5,
        "mx-1 + gap-2: the close button starts 12pt past the hairline"
    );
    // `text-sm font-medium` = the 14pt/500 token with its 21pt line, `text-fg`.
    assert!(
        (title.rect[3] - 21.0).abs() <= 0.5,
        "text-sm font-medium: the title's 21pt line: {:?}",
        title.rect
    );
    assert_eq!(
        title.text_color,
        Some(k.c.text_strong),
        "text-fg: the title paints the strong foreground"
    );
}

#[test]
fn the_overlay_export_buttons_paint_their_literal_captions() {
    // React's two buttons render the literal `PNG`/`SVG` captions in
    // `text-xs text-fg-muted` (247-260) — the a11y labels and the painted
    // text are both the literal.
    let mut p = two_axis_probe();
    p.expanded = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let k = kit(true);
    for (key, caption) in [
        (OVERLAY_PNG_KEY, "PNG"),
        (OVERLAY_SVG_KEY, "SVG"),
    ] {
        let label = element(&rt, &format!("{key}-label"));
        assert_eq!(
            label.text.as_deref(),
            Some(caption),
            "the {caption} button paints its literal caption"
        );
        assert!(
            (label.rect[3] - 18.0).abs() <= 0.5,
            "text-xs: the caption's 18pt line: {:?}",
            label.rect
        );
        assert_eq!(
            label.text_color,
            Some(k.c.text_muted),
            "text-fg-muted: the caption paints the muted foreground"
        );
    }
}

// ---- The split grid's paint --------------------------------------------------------

#[test]
fn the_split_grid_is_two_padded_gap_8_columns_of_rounded_tiles() {
    // React `grid grid-cols-1 gap-2 p-2 md:grid-cols-2` (161): the grid
    // reserves 8pt padding, 8pt gutters and exactly two equal columns (the
    // `md:` breakpoint ruling), and each tile is `rounded` (165).
    let mut p = two_axis_probe();
    p.split_view = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let k = kit(true);
    let grid = element(&rt, SPLIT_GRID_KEY);
    let first = element(&rt, "chart-view-tile-count");
    let second = element(&rt, "chart-view-tile-sum");
    // p-2: the tiles start 8pt inside the grid, and end 8pt before its edge.
    assert!(
        (first.rect[0] - grid.rect[0] - 8.0).abs() <= 0.5,
        "p-2: the first tile starts 8pt inside the grid: {:?} {:?}",
        grid.rect,
        first.rect
    );
    assert!(
        ((grid.rect[0] + grid.rect[2]) - (second.rect[0] + second.rect[2]) - 8.0).abs() <= 0.5,
        "p-2: the last tile ends 8pt before the grid's right edge"
    );
    // gap-2: the 8pt gutter between the two tiles.
    assert!(
        (second.rect[0] - (first.rect[0] + first.rect[2]) - 8.0).abs() <= 0.5,
        "gap-2: an 8pt gutter between the tiles: {:?} {:?}",
        first.rect,
        second.rect
    );
    // md:grid-cols-2: two equal columns — each tile is half of the content
    // width beside the 208pt axis panel (the grid spans 960 - 208 = 752pt).
    assert!(
        (first.rect[2] - 364.0).abs() <= 1.0,
        "md:grid-cols-2: each tile is (752 - 2*8 - 8)/2 = 364pt: {:?}",
        first.rect
    );
    assert!(
        (first.rect[2] - second.rect[2]).abs() <= 1.0,
        "md:grid-cols-2: the two columns are equal"
    );
    // `rounded` on the tile.
    assert_eq!(
        element(&rt, "chart-view-tile-count").radius,
        k.radius.sm,
        "rounded: the tile carries the kit's small radius"
    );
}

#[test]
fn the_tile_chip_is_the_11px_medium_muted_axis_label() {
    // React's tile chip (166-168): `text-[11px] font-medium text-fg-muted`
    // — the 11pt/500 token with its 12pt line, in the muted foreground.
    let mut p = two_axis_probe();
    p.split_view = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let k = kit(true);
    let chip = element(&rt, "chart-view-tile-chip-count");
    assert!(
        (chip.rect[3] - 12.0).abs() <= 0.5,
        "text-[11px]: the chip's 12pt line: {:?}",
        chip.rect
    );
    assert_eq!(
        chip.text_color,
        Some(k.c.text_muted),
        "text-fg-muted: the chip paints the muted foreground"
    );
}

// ---- The axis-configurator slot -----------------------------------------------------

#[test]
fn the_axis_slot_placeholder_paints_the_panel_surface() {
    // The placeholder keeps the React AxisConfigurator panel's chrome
    // (`flex w-52 shrink-0 flex-col border-r border-edge bg-surface-alt`):
    // the `bg-surface-alt` fill and the note's muted foreground.
    let p = mounted_probe();
    let rt = chart_runtime(p, (960.0, 640.0));
    let k = kit(true);
    assert_eq!(
        element(&rt, AXIS_SLOT_KEY).fill,
        k.c.layer_01,
        "bg-surface-alt: the slot panel paints the alternate surface"
    );
    assert_eq!(
        element(&rt, AXIS_SLOT_NOTE_KEY).text_color,
        Some(k.c.text_muted),
        "the honest note paints the muted foreground"
    );
}

#[test]
fn the_axis_slot_receives_the_live_config_with_the_grouping_intact() {
    // React hands AxisConfigurator the LIVE `config` — grouping intact, the
    // user's own y axes — in both mounts (150-155, 274-279); the folded
    // renderConfig is the canvas's business alone (110-113).
    let mut p = grouped_probe();
    let log = Rc::clone(&p.axis_log);
    p.axis_slot = Some(make_axis_slot(log));
    p.expanded = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let logged = rt.state.axis_log.borrow();
    assert_eq!(logged.len(), 2, "the body row and the overlay both mount it");
    for (i, obs) in logged.iter().enumerate() {
        assert_eq!(
            obs.field_count, 3,
            "mount {i}: the fields the pipeline inferred"
        );
        assert_eq!(
            obs.y_axes,
            vec!["count".to_string(), "sum".to_string()],
            "mount {i}: the LIVE y axes, not the pivot series"
        );
        assert_eq!(
            obs.group_by.as_deref(),
            Some("region"),
            "mount {i}: the live grouping survives — the folded render config \
             would have cleared it"
        );
        assert!(obs.recommendation, "mount {i}: the recommendation arrives");
        assert!(obs.change_wired, "mount {i}: the setConfig seam arrives");
    }
    assert!(has("chart.expandTitle"));
}

// ---- Wiring through the toolbar -------------------------------------------------------

#[test]
fn the_view_forwards_its_split_state_to_the_toolbar_button() {
    // React `splitView={splitView}` (146): the toolbar's split button paints
    // its pressed chip from the state the caller owns.
    let mut p = two_axis_probe();
    p.split_view = true;
    let rt = chart_runtime(p, (960.0, 640.0));
    let k = kit(true);
    assert_eq!(
        element(&rt, "chart-toolbar-split").fill,
        k.alpha(k.c.bg_accent, 0.2),
        "the split button paints the accent chip while split"
    );

    // Unsplit: the same button paints no accent fill.
    let p = two_axis_probe();
    let rt = chart_runtime(p, (960.0, 640.0));
    assert_ne!(
        element(&rt, "chart-toolbar-split").fill,
        k.alpha(k.c.bg_accent, 0.2),
        "the split button is unpressed while unsplit"
    );
}

#[test]
fn selecting_an_export_item_closes_the_dropdown_through_the_view_seam() {
    // React's dropdown closes after an entry is chosen — and the view owns
    // the dismissal seam the strip calls (the T059 `onClose` wiring). One
    // press opens the panel, the entry's select routes the export AND the
    // close back through the view, and the rebuild drops the panel.
    let p = two_axis_probe();
    let mut rt = chart_runtime(p, (960.0, 640.0));
    click(&mut rt, "chart-export-button");
    assert_eq!(rt.state.export_toggles, 1, "the trigger opened the dropdown");
    assert!(has_key(&rt, "chart-export-menu"), "the export panel is mounted");
    click(&mut rt, "chart-export-item0");
    assert_eq!(
        rt.state.exports,
        vec![ExportFormat::Png],
        "the PNG entry routed through the view's export seam"
    );
    assert_eq!(
        rt.state.export_closes, 1,
        "the selection closed the dropdown through the view's export-close seam"
    );
    assert!(
        !rt.state.export_open,
        "the caller's dropdown state is closed again"
    );
    assert!(
        !has_key(&rt, "chart-export-menu"),
        "the rebuild dropped the export panel"
    );
}
