//! Port of `packages/frontend/src/components/chart/ChartCanvas.tsx` (Wave 2, T008).
//!
//! The React component is a thin dispatcher: it reads `config.chartType`, wraps
//! the content in `recharts`' `ResponsiveContainer` (width/height 100%), and
//! renders one of five renderer components. The five renderers are **separate**
//! migration tasks and are intentionally NOT implemented here: each chart type
//! gets a placeholder that renders an honest "renderer not ported" note with a
//! stable marker key, so a caller/tests can observe exactly which renderer was
//! selected without any fake graphics.
//!
//! Rune has no `ResponsiveContainer`. Its equivalent is a bounded, growing
//! column with a runtime-measured rect: `ChartCanvas` owns that container
//! (`{CANVAS_CONTAINER_KEY}`), measures it through `El::on_layout_tx` (surfaced
//! via `.on_measured(...)`), and lets a bounded `.height(..)` override the
//! available-space fill (the React tile passes a fixed-height parent).
//!
//! The dispatcher accepts a renderer factory so the real renderers can be
//! injected later by the chart-view track without editing this file. The
//! default factory returns the placeholders.
//!
//! ## Keys
//!
//! - `chart-canvas` — the outer padded container (React `absolute inset-0 p-4|p-1`).
//! - `chart-canvas-container` — the measured, width-filling chart viewport.
//! - `chart-renderer-note-{bar|line|pie|scatter|area}` — the honest placeholder note.
//!
//! All colors/fonts come from `kit(dark)` tokens; no raw `oklch`, no recharts.

use std::collections::BTreeMap;
use std::rc::Rc;

use rune::{col, text, El, Tx};

use crate::ui::kit::kit;

/// Outer padded container, mirroring React `absolute inset-0` + `p-4`/`p-1`.
pub(crate) const CANVAS_KEY: &str = "chart-canvas";
/// The measured chart viewport, Rune's `ResponsiveContainer` (width/height 100%).
pub(crate) const CANVAS_CONTAINER_KEY: &str = "chart-canvas-container";

/// Padding for a standalone canvas (React `p-4` = 16pt).
pub(crate) const PAD_FULL: f32 = 16.0;
/// Padding for a compact tile (React `p-1` = 4pt).
pub(crate) const PAD_COMPACT: f32 = 4.0;

// ---- View models mirroring `packages/frontend/src/types/chart.ts` ----------

/// Mirror of `ChartType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable VM; consumed by the chart-view track
pub(crate) enum ChartType {
    Bar,
    Line,
    Pie,
    Scatter,
    Area,
}

#[allow(dead_code)] // reusable VM; consumed by the chart-view track
impl ChartType {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Bar => "bar",
            Self::Line => "line",
            Self::Pie => "pie",
            Self::Scatter => "scatter",
            Self::Area => "area",
        }
    }

    /// Parse a TS `ChartType` string; unknown values fall back to the React
    /// default (`bar`) so a malformed payload cannot select a phantom renderer.
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "line" => Self::Line,
            "pie" => Self::Pie,
            "scatter" => Self::Scatter,
            "area" => Self::Area,
            _ => Self::Bar,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Bar => "Bar",
            Self::Line => "Line",
            Self::Pie => "Pie",
            Self::Scatter => "Scatter",
            Self::Area => "Area",
        }
    }
}

/// Every renderer the dispatcher knows about, in React declaration order.
#[allow(dead_code)] // reusable VM; consumed by the chart-view track
pub(crate) const ALL_CHART_TYPES: [ChartType; 5] = [
    ChartType::Bar,
    ChartType::Line,
    ChartType::Pie,
    ChartType::Scatter,
    ChartType::Area,
];

/// Stable marker key for the placeholder note of a given chart type.
pub(crate) fn renderer_note_key(chart_type: ChartType) -> &'static str {
    match chart_type {
        ChartType::Bar => "chart-renderer-note-bar",
        ChartType::Line => "chart-renderer-note-line",
        ChartType::Pie => "chart-renderer-note-pie",
        ChartType::Scatter => "chart-renderer-note-scatter",
        ChartType::Area => "chart-renderer-note-area",
    }
}

/// Mirror of `AggregationType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable VM; consumed by the chart-view track
pub(crate) enum AggregationType {
    None,
    Sum,
    Avg,
    Count,
    Min,
    Max,
    DistinctCount,
}

#[allow(dead_code)] // reusable VM; consumed by the chart-view track
impl AggregationType {
    fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Sum => "sum",
            Self::Avg => "avg",
            Self::Count => "count",
            Self::Min => "min",
            Self::Max => "max",
            Self::DistinctCount => "distinct_count",
        }
    }
}

/// Mirror of `InferredFieldType`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable VM; consumed by the chart-view track
pub(crate) enum InferredFieldType {
    Numeric,
    Datetime,
    Categorical,
    Boolean,
    Unknown,
}

/// Mirror of `ChartField` (`sampleValues?: unknown[]` is carried as display-safe
/// scalar cells; nested/unknown JSON does not cross into the Rune VM).
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // reusable VM; consumed by the chart-view track
pub(crate) struct ChartField {
    pub(crate) name: String,
    pub(crate) data_type: String,
    pub(crate) inferred_type: InferredFieldType,
    pub(crate) distinct_count: Option<u64>,
    pub(crate) sample_values: Vec<ChartValue>,
}

/// The value space of `ChartDataPoint` (`string | number | null`).
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // reusable VM; consumed by the chart-view track
pub(crate) enum ChartValue {
    Text(String),
    Number(f64),
    Null,
}

#[allow(dead_code)] // reusable VM; consumed by the chart-view track
impl ChartValue {
    pub(crate) fn as_display(&self) -> String {
        match self {
            Self::Text(s) => s.clone(),
            Self::Number(n) => n.to_string(),
            Self::Null => String::new(),
        }
    }
}

/// Mirror of `ChartDataPoint` (`{ [key: string]: string | number | null }`).
/// A `BTreeMap` keeps field iteration deterministic (stable keys/order).
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)] // reusable VM; consumed by the chart-view track
pub(crate) struct ChartDataPoint {
    values: BTreeMap<String, ChartValue>,
}

#[allow(dead_code)] // reusable VM; consumed by the chart-view track
impl ChartDataPoint {
    pub(crate) fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    pub(crate) fn with(mut self, key: impl Into<String>, value: ChartValue) -> Self {
        self.values.insert(key.into(), value);
        self
    }

    pub(crate) fn insert(&mut self, key: impl Into<String>, value: ChartValue) {
        self.values.insert(key.into(), value);
    }

    pub(crate) fn get(&self, key: &str) -> Option<&ChartValue> {
        self.values.get(key)
    }

    pub(crate) fn len(&self) -> usize {
        self.values.len()
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.values.is_empty()
    }
}

/// Mirror of `ChartConfig['sortBy']`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // reusable VM; consumed by the chart-view track
pub(crate) enum ChartSort {
    XAsc,
    XDesc,
    YAsc,
    YDesc,
    None,
}

#[allow(dead_code)] // reusable VM; consumed by the chart-view track
impl ChartSort {
    fn as_str(self) -> &'static str {
        match self {
            Self::XAsc => "x_asc",
            Self::XDesc => "x_desc",
            Self::YAsc => "y_asc",
            Self::YDesc => "y_desc",
            Self::None => "none",
        }
    }
}

/// Mirror of `ChartConfig`.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // reusable VM; consumed by the chart-view track
pub(crate) struct ChartConfig {
    pub(crate) chart_type: ChartType,
    pub(crate) x_axis: Option<String>,
    pub(crate) y_axes: Vec<String>,
    pub(crate) group_by: Option<String>,
    pub(crate) aggregation: AggregationType,
    pub(crate) sort_by: ChartSort,
    pub(crate) show_legend: bool,
    pub(crate) show_grid: bool,
    pub(crate) show_values: bool,
    pub(crate) color_scheme: String,
    pub(crate) time_domain: Option<[f64; 2]>,
    pub(crate) time_ticks: Vec<f64>,
}

/// Mirror of React `DEFAULT_CHART_CONFIG`.
#[allow(dead_code)] // reusable VM; consumed by the chart-view track
impl Default for ChartConfig {
    fn default() -> Self {
        Self {
            chart_type: ChartType::Bar,
            x_axis: None,
            y_axes: Vec::new(),
            group_by: None,
            aggregation: AggregationType::None,
            sort_by: ChartSort::None,
            show_legend: true,
            show_grid: true,
            show_values: false,
            color_scheme: "default".to_string(),
            time_domain: None,
            time_ticks: Vec::new(),
        }
    }
}

#[allow(dead_code)] // reusable VM; consumed by the chart-view track
impl ChartConfig {
    /// Set the chart type (builder for tests and the future view track).
    pub(crate) fn chart_type(mut self, chart_type: ChartType) -> Self {
        self.chart_type = chart_type;
        self
    }
}

// ---- Renderer seam ---------------------------------------------------------

/// Everything a renderer needs to build its element. Kept generic over the app
/// state `S` so a real renderer can attach `on_data_point_click` callbacks.
#[allow(dead_code)] // reusable API; consumed by the chart-view track
pub(crate) struct ChartRenderContext<'a, S: 'static> {
    pub(crate) data: &'a [ChartDataPoint],
    pub(crate) config: &'a ChartConfig,
    pub(crate) compact: bool,
    pub(crate) dark: bool,
    pub(crate) on_data_point_click: Option<Rc<dyn Fn(&mut S, usize) + 'static>>,
}

/// The renderer abstraction selected by `config.chart_type`.
#[allow(dead_code)] // reusable API; consumed by the chart-view track
pub(crate) trait ChartRenderer<S: 'static> {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El;
}

/// A factory the canvas calls once per build to obtain the renderer for the
/// selected chart type. Inject the real renderers here later.
pub(crate) type RendererFactory<S> =
    dyn Fn(ChartType) -> Box<dyn ChartRenderer<S> + 'static> + 'static;

/// Honest placeholder for an unported chart engine. Renders only a note with a
/// stable marker key; no fake bars/lines/points.
#[allow(dead_code)] // reusable API; consumed by the chart-view track
pub(crate) struct PlaceholderChartRenderer {
    chart_type: ChartType,
}

#[allow(dead_code)] // reusable API; consumed by the chart-view track
impl PlaceholderChartRenderer {
    pub(crate) fn new(chart_type: ChartType) -> Self {
        Self { chart_type }
    }
}

impl<S: 'static> ChartRenderer<S> for PlaceholderChartRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        let k = kit(ctx.dark);
        let note_key = renderer_note_key(self.chart_type);
        col()
            .key(note_key)
            .w_full()
            .h_full()
            .min_w(0.0)
            .min_h(0.0)
            .center()
            .child(
                text(format!("{} renderer not ported", self.chart_type.label()))
                    .key(format!("{note_key}-text"))
                    .font(k.ty.text_small)
                    .color(k.c.text_muted)
                    .wrap(),
            )
    }
}

/// Default factory: every chart type gets its honest placeholder.
pub(crate) fn placeholder_renderer<S: 'static>(chart_type: ChartType) -> Box<dyn ChartRenderer<S>> {
    Box::new(PlaceholderChartRenderer::new(chart_type))
}

// ---- Canvas ----------------------------------------------------------------

/// The `ResponsiveContainer` equivalent plus the thin dispatcher.
#[allow(dead_code)] // reusable API; wired by the chart-view track
pub(crate) struct ChartCanvas<'a, S: 'static> {
    config: &'a ChartConfig,
    data: &'a [ChartDataPoint],
    dark: bool,
    compact: bool,
    height: Option<f32>,
    on_data_point_click: Option<Rc<dyn Fn(&mut S, usize) + 'static>>,
    on_measured: Option<Rc<dyn Fn(&mut S, [f32; 4]) + 'static>>,
    factory: Option<Box<RendererFactory<S>>>,
}

#[allow(dead_code)] // reusable API; wired by the chart-view track
impl<'a, S: 'static> ChartCanvas<'a, S> {
    pub(crate) fn new(config: &'a ChartConfig, data: &'a [ChartDataPoint]) -> Self {
        Self {
            config,
            data,
            dark: true,
            compact: false,
            height: None,
            on_data_point_click: None,
            on_measured: None,
            factory: None,
        }
    }

    pub(crate) fn dark(mut self, dark: bool) -> Self {
        self.dark = dark;
        self
    }

    pub(crate) fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    /// Bound the viewport to an explicit height instead of filling the space
    /// the parent leaves. This is the tile/fixed-height case.
    pub(crate) fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    pub(crate) fn on_data_point_click(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_data_point_click = Some(Rc::new(f));
        self
    }

    /// Observe the runtime-measured rect `[x, y, w, h]` of the viewport. This
    /// is the observable channel for "the container has a measurable rect".
    pub(crate) fn on_measured(mut self, f: impl Fn(&mut S, [f32; 4]) + 'static) -> Self {
        self.on_measured = Some(Rc::new(f));
        self
    }

    /// Inject the five concrete renderers later without touching this file.
    pub(crate) fn renderer_factory(
        mut self,
        f: impl Fn(ChartType) -> Box<dyn ChartRenderer<S> + 'static> + 'static,
    ) -> Self {
        self.factory = Some(Box::new(f));
        self
    }

    pub(crate) fn build(self) -> El {
        let mut viewport = col()
            .key(CANVAS_CONTAINER_KEY)
            .w_full()
            .min_w(0.0)
            .min_h(0.0);
        viewport = match self.height {
            Some(h) => viewport.h(h),
            None => viewport.grow(1.0),
        };
        if let Some(on_measured) = self.on_measured {
            viewport = viewport
                .on_layout_tx(move |s: &mut S, r: [f32; 4], _tx: &mut Tx| on_measured(s, r));
        }

        let default_factory: &RendererFactory<S> = &placeholder_renderer::<S>;
        let factory = self.factory.as_deref().unwrap_or(default_factory);
        let renderer = factory(self.config.chart_type);
        let ctx = ChartRenderContext {
            data: self.data,
            config: self.config,
            compact: self.compact,
            dark: self.dark,
            on_data_point_click: self.on_data_point_click,
        };
        let renderer_el = renderer.render(&ctx);

        let pad = if self.compact { PAD_COMPACT } else { PAD_FULL };
        col()
            .key(CANVAS_KEY)
            .w_full()
            .h_full()
            .min_w(0.0)
            .padding(pad)
            .child(viewport.child(renderer_el))
    }
}
