//! Port of `packages/frontend/src/components/chart/renderers/LineChartRenderer.tsx` (T216).
//!
//! The React component is a `recharts` `<LineChart>` composition — the third
//! engine on the `ChartRenderer` seam (`chart_canvas.rs`), after the area
//! (T166) and bar (T192) engines. The memo chain is the family's: the log-scale
//! hint decides whether `mapToLogScale` rewrites the Y values (stashing the
//! originals under `__orig__${key}` for the T136 tooltip to read back), a
//! `useState<Set<string>>` hides series from the legend, `buildOverlapDashMap`
//! dashes series that would overlap, and `<Line>` paints each series — plus
//! the `<CartesianGrid>` hairlines, the two axes with their tick formatters,
//! the `<Legend>` that toggles, and `<LogScaleTooltip>`/`<Tooltip>` content.
//! What makes the line engine its own file is the `<Line>` body: every drawn
//! point carries a `dot` disc, the hovered row carries an `activeDot` disc,
//! and `config.showValues` mounts a `<LabelList position="top">` per series.
//!
//! ## How the recharts composition maps onto Rune
//!
//! * **Real geometry, not an approximation.** `rune::Shape` strokes
//!   polylines and fills polygons, so each series paints its `strokeWidth: 2`
//!   line inside one `rune::canvas` element — no area fill; the `<Line>` of
//!   the source has none. Grid hairlines and the axis frame are stroke/line
//!   ops, the two dot layers are filled n-gons. Coordinates are written in
//!   the `view_box(1, 1)` fractions of the plot, so one shape stretches to
//!   whatever rect layout gives the plot.
//! * **The plot frame** is the flexbox rebuild of the recharts margin box the
//!   area engine established: a 60pt Y-axis column (`justify_between` over the
//!   five ticks — recharts' default YAxis width), the growing plot stack, a
//!   30pt X-axis row (recharts' default XAxis height) of per-band tick slots,
//!   and the 22pt legend row when `multiSeries`. Overlapping canvases stack
//!   through `.absolute(0.0, 0.0)` plus `.w_full().h_full()` — the shimmer
//!   overlay pattern in Rune's own tests — so grid, axes, every series, the
//!   dots, the value labels and the hit layer share one rect.
//!
//! ## The seams (the split discipline)
//!
//! | React import | owner | shape here |
//! |--------------|-------|------------|
//! | `getColorPalette` (`lib/chart/colors.ts`) | T352 | [`series_palette`] — the five-colour kit palette the area engine sliced; every scheme name maps to it until T352 lands |
//! | `formatAxisTick` / `formatCompact` / `formatEpochMs` (`lib/chart/format.ts`) | T079 | [`FormatSeams`] — the three injected `Rc<dyn Fn>`, re-exported from the area engine |
//! | `computeLogScaleHint` / `mapToLogScale` (`lib/chart/transform.ts`) | T182 | the area engine's slices, imported |
//! | `buildOverlapDashMap` (`lib/chart/seriesOverlap.ts`) | T153 | the area engine's slice, imported |
//! | `LogScaleTooltip` (T136, landed) | T136 | `super::super::log_scale_tooltip`'s `format_entry`/`build`, mounted by this engine |
//!
//! ## Interaction (immediate mode)
//!
//! * The `useState<Set<string>>` becomes [`EngineState`] behind an
//!   `Rc<RefCell<..>>` shared by every clone of the engine: the legend entry's
//!   press calls React's insert-or-remove, and because the app's factory
//!   clones the same engine each build, the set survives frames — exactly the
//!   retained state React kept.
//! * recharts' `onClick → activeTooltipIndex` routing becomes one hit cell per
//!   data row: a `w_frac(1/n)` band over the plot whose press hands
//!   `onDataPointClick(row_index)`. The cells' rects are the observable hit
//!   rectangles (equidistant band buckets, matching recharts' band-scale
//!   positions `(i + 0.5) / n`; the time-domain rows keep the epoch scale).
//! * The tooltip follows recharts' `activeTooltipIndex` semantics the same
//!   way: entering a cell (`on_hover`) sets the hovered row, the box mounts
//!   inside that cell (anchored like the floating content), and leaving the
//!   chart (`on_hover_change(false)` on the hit layer) clears it. The
//!   hovered row also mounts the `activeDot` layer.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | root | `line-chart` |
//! | plot row / plot stack | `line-chart-plot-row` / `line-chart-plot` |
//! | grid canvas / axes canvas | `line-chart-grid` / `line-chart-axes` |
//! | series canvas | `line-chart-series-{key}` |
//! | the per-point `dot` discs | `line-chart-dots` |
//! | the hovered row's `activeDot` discs | `line-chart-active-dots` |
//! | values overlay / band / label | `line-chart-values` / `line-chart-values-band-{row}` / `line-chart-value-{row}-{key}` |
//! | hit layer / hit cell *i* | `line-chart-hit-zone` / `line-chart-hit-{i}` |
//! | Y axis / Y tick *k* | `line-chart-yaxis` / `line-chart-yaxis-tick-{k}` |
//! | X axis / X tick *i* | `line-chart-xaxis` / `line-chart-xaxis-tick-{i}` |
//! | legend / item / swatch / label | `line-chart-legend` / `line-chart-legend-item-{key}` / `…-swatch-{key}` / `…-label-{key}` |
//! | the plain tooltip box | `line-chart-tooltip` |
//! | the log-scale tooltip box | `chart-log-scale-tooltip` (T136's `CONTENT_KEY`) |
//!
//! ## Deliberate deviations
//!
//! * recharts' tick picking ("nice" numbers, auto interval) is not ported: Y
//!   ticks are five quarter-domain values, X ticks are every `ceil(n/8)`-th
//!   band; time-axis ticks come from `config.timeTicks` snapped to the nearest
//!   band (Rune's flexbox has no percent-offset positioning for a label).
//! * `<Line type="monotone">`'s Fritsch–Carlson smoothing is not ported: the
//!   stroke runs through the same straight segments. The monotone spline
//!   exists to keep curves from overshooting between points; the polyline
//!   cannot overshoot, and the y read is identical at every point — a
//!   recharts cosmetic, traded for the same data read (T166's trade, kept).
//! * The Y domain is recharts' default `[0, auto]` behaviour, lean: `[0, max]`
//!   for non-negative data, `[min, 0]` for non-positive, `[min, max]` across
//!   zero; no nice-number padding. Hiding a series does not rescale (recharts
//!   does not either).
//! * `CartesianGrid`'s `strokeDasharray="3 3"` hairlines paint as solid
//!   hairlines — Rune's `Shape` has no dash on strokes — while the series'
//!   `strokeDasharray` overlap patterns ARE painted, by cutting the polyline
//!   into on-runs (`dashed_segments`, the area engine's slice).
//! * The `dot`/`activeDot` discs paint as filled 16-gons (Rune has no circle
//!   primitive — T166's `activeDot` decision); `dot`'s `r: 3` and `activeDot`'s
//!   `r: 5` are exact, in view-box units. The `cursor: 'pointer'` hint has no
//!   Rune counterpart. A row whose slot is null/NaN paints no disc — recharts
//!   draws no dot where it draws no line point.
//! * `LabelList position="top"` mounts in the band column its point sits in,
//!   stacked per series in declaration order at the top of the band — Rune's
//!   flexbox has no fraction-offset anchor for a text node (T166's
//!   tooltip-anchor deviation class), so the labels ride the plot's top edge
//!   instead of each point's own top. `fontSize: 11` reads `k.ty.small` (the
//!   kit's 11px face, the bar engine's mapping).
//! * Y tick labels are box-aligned by `justify_between` (first/last at the
//!   plot's edges); recharts centres each label on its tick.
//! * The tooltip anchors at the hovered bucket's top-left (recharts floats it
//!   near the point, clamped — same content, a coarser anchor).
//! * X tick labels wrap instead of recharts' rotation; the `formatAxisTick`
//!   seam keeps React's 20-character truncation for strings.
//! * The hit cells are `a11y_hidden` (a 1000-row chart would otherwise be a
//!   wall of buttons); the legend entries are real focusable buttons.
//! * JS numeric edge semantics follow the T136 precedent (`Number(str)`,
//!   `String(n)`); `toLocaleString` grouping is not in the lean slice, and
//!   `formatEpochMs` reads UTC (Rune carries no host locale clock) — both are
//!   T079's to settle.

#![allow(dead_code)] // the chart-view track wires the factory; until then only tests exercise it

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::rc::Rc;

use rune::{canvas, col, el, row, text, Align, El, LineCap, Shape, Tx};

use super::super::chart_canvas::{
    ChartConfig, ChartDataPoint, ChartRenderContext, ChartRenderer, ChartType, ChartValue,
    PlaceholderChartRenderer,
};
use super::super::log_scale_tooltip::{
    chart_payload, format_entry, LogScaleTooltip, LogScaleTooltipEntry,
};
use super::area_chart_renderer::{
    build_overlap_dash_map, compute_log_scale_hint, dashed_segments, js_string, lean_y_domain,
    map_to_log_scale, series_palette, to_number, LogScaleHint, XTick, YTick,
};
/// The family's `lib/chart/format.ts` seam struct (T079) — the area engine's,
/// re-exported so the line engine's callers wire it through this module.
pub(crate) use super::area_chart_renderer::FormatSeams;
use crate::ui::kit::kit;
use rune_kit::Kit;

// ---- Keys -------------------------------------------------------------------

/// Root, recharts' `<LineChart>` frame.
pub(crate) const ROOT_KEY: &str = "line-chart";
/// The Y-axis + plot stack row.
pub(crate) const PLOT_ROW_KEY: &str = "line-chart-plot-row";
/// The plot itself — every canvas, the labels and the hit layer share its rect.
pub(crate) const PLOT_KEY: &str = "line-chart-plot";
/// The `CartesianGrid` canvas.
pub(crate) const GRID_KEY: &str = "line-chart-grid";
/// The axis frame canvas (recharts' axis lines).
pub(crate) const AXES_KEY: &str = "line-chart-axes";
/// One canvas per series: `line-chart-series-{key}`.
pub(crate) const SERIES_KEY_PREFIX: &str = "line-chart-series-";
/// The `dot` layer: one `r: 3` disc per drawn point per visible series.
pub(crate) const DOTS_KEY: &str = "line-chart-dots";
/// The hovered row's `activeDot`s: one `r: 5` disc per visible series.
pub(crate) const ACTIVE_DOTS_KEY: &str = "line-chart-active-dots";
/// The `LabelList` overlay (React 120-134, `config.showValues`).
pub(crate) const VALUES_KEY: &str = "line-chart-values";
/// One band column per data row: `line-chart-values-band-{row}`.
pub(crate) const VALUES_BAND_PREFIX: &str = "line-chart-values-band-";
/// One value label per row/series: `line-chart-value-{row}-{key}`.
pub(crate) const VALUE_PREFIX: &str = "line-chart-value-";
/// The pointer/press layer over the plot.
pub(crate) const HIT_ZONE_KEY: &str = "line-chart-hit-zone";
/// One hit cell per data row: `line-chart-hit-{i}`.
pub(crate) const HIT_CELL_PREFIX: &str = "line-chart-hit-";
/// The Y-axis label column.
pub(crate) const Y_AXIS_KEY: &str = "line-chart-yaxis";
/// The X-axis label row.
pub(crate) const X_AXIS_KEY: &str = "line-chart-xaxis";
/// The legend (multiSeries only).
pub(crate) const LEGEND_KEY: &str = "line-chart-legend";
/// One entry per series: `line-chart-legend-item-{key}` (+ `-swatch-`/`-label-`).
pub(crate) const LEGEND_ITEM_PREFIX: &str = "line-chart-legend-item-";
/// The swatch of legend entry `{key}`.
pub(crate) const LEGEND_SWATCH_PREFIX: &str = "line-chart-legend-swatch-";
/// The label of legend entry `{key}` (dims to 0.35 when hidden).
pub(crate) const LEGEND_LABEL_PREFIX: &str = "line-chart-legend-label-";
/// The plain (non-log) `<Tooltip>` content box.
pub(crate) const TOOLTIP_KEY: &str = "line-chart-tooltip";

// recharts defaults / React literals, verbatim.
/// recharts' default `YAxis width`.
const Y_AXIS_W: f32 = 60.0;
/// recharts' default `XAxis height`.
const X_AXIS_H: f32 = 30.0;
/// One legend row at `fontSize: 12`.
const LEGEND_H: f32 = 22.0;
/// React `gap-2` between legend entries.
const LEGEND_GAP: f32 = 8.0;
/// React `gap-1` inside an entry.
const LEGEND_ENTRY_GAP: f32 = 4.0;
/// The legend swatch chip.
const SWATCH: f32 = 8.0;
/// React `strokeWidth`.
const STROKE_W: f32 = 2.0;
/// The axis frame's stroke.
const AXIS_LINE_W: f32 = 1.0;
/// The grid hairlines' stroke.
const GRID_LINE_W: f32 = 1.0;
/// React `CartesianGrid` `opacity` when `showGrid`.
const GRID_OPACITY: f32 = 0.3;
/// React `dot` `r`.
const DOT_R: f32 = 3.0;
/// React `activeDot` `r`.
const ACTIVE_DOT_R: f32 = 5.0;
/// The tooltip's anchor inside the hovered bucket.
const TOOLTIP_DX: f32 = 2.0;
/// The tooltip's anchor below the plot's top.
const TOOLTIP_DY: f32 = 8.0;
/// Y ticks: recharts' default five-ish even ticks, lean.
const Y_TICKS: usize = 5;
/// X ticks: the lean recharts-interval cap.
const MAX_X_TICKS: usize = 8;

// ---- The engine-private geometry (the area engine's, engine-local) ------------
//
// The two fraction helpers and the run/gon cutters are the area engine's
// semantics, re-implemented here because they are engine-private there; the
// `lib/chart` slices the chain reads stay imported (see the seams table).

/// A row's X fraction: the band position `(i + 0.5) / n`, or the time-scale
/// position when `config.timeDomain` is set (out-of-domain rows are not drawn,
/// as recharts hides them).
fn x_fraction(
    row: usize,
    point: &ChartDataPoint,
    x_key: &str,
    config: &ChartConfig,
    row_count: usize,
) -> Option<f32> {
    if row_count == 0 {
        return None;
    }
    let Some([d0, d1]) = config.time_domain else {
        return Some((row as f32 + 0.5) / row_count as f32);
    };
    let t = to_number(point.get(x_key).unwrap_or(&ChartValue::Null))?;
    if !t.is_finite() {
        return None;
    }
    if d1 <= d0 {
        return Some(0.5);
    }
    if t < d0 || t > d1 {
        return None;
    }
    Some(((t - d0) / (d1 - d0)) as f32)
}

/// A value's Y fraction (1 = the plot's bottom): NaN guards the layout (JS
/// would paint garbage); a flat domain centres.
fn y_fraction(point: &ChartDataPoint, key: &str, domain: [f64; 2]) -> Option<f32> {
    let v = to_number(point.get(key).unwrap_or(&ChartValue::Null))?;
    if !v.is_finite() {
        return None;
    }
    let span = domain[1] - domain[0];
    let f = if span > 0.0 {
        (v - domain[0]) / span
    } else {
        0.5
    };
    Some((1.0 - f) as f32)
}

/// The runs of drawable points (recharts breaks the line at nulls —
/// `connectNulls` stays false).
fn present_runs(points: &[Option<[f32; 2]>]) -> Vec<Vec<[f32; 2]>> {
    let mut runs = Vec::new();
    let mut current: Vec<[f32; 2]> = Vec::new();
    for p in points {
        match p {
            Some(pt) => current.push(*pt),
            None => {
                if current.len() >= 2 {
                    runs.push(std::mem::take(&mut current));
                } else {
                    current.clear();
                }
            }
        }
    }
    if current.len() >= 2 {
        runs.push(current);
    }
    runs
}

/// A regular n-gon (the `dot`/`activeDot` discs — Rune has no circle
/// primitive).
fn gon(center: [f32; 2], radius: f32, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect()
}

/// The X ticks (and their grid positions): `config.timeTicks` snapped to the
/// nearest band under a time domain, else every `ceil(n/8)`-th band — the
/// lean stand-in for recharts' auto interval.
fn x_tick_plan(
    chart_data: &[ChartDataPoint],
    config: &ChartConfig,
    x_key: &str,
    row_count: usize,
    seams: &FormatSeams,
) -> (Vec<XTick>, Vec<f32>) {
    let mut ticks = Vec::new();
    let mut xs = Vec::new();
    if let Some([d0, d1]) = config.time_domain {
        for v in &config.time_ticks {
            if !v.is_finite() || row_count == 0 {
                continue;
            }
            let f = if d1 > d0 {
                ((v - d0) / (d1 - d0)).clamp(0.0, 1.0)
            } else {
                0.5
            };
            xs.push(f as f32);
            let row = (f * row_count as f64)
                .floor()
                .clamp(0.0, (row_count - 1) as f64) as usize;
            ticks.push(XTick {
                row,
                label: (seams.format_epoch_ms)(*v),
            });
        }
    } else if row_count > 0 {
        let stride = row_count.div_ceil(MAX_X_TICKS);
        let mut ri = 0;
        while ri < row_count {
            let value = chart_data[ri].get(x_key);
            ticks.push(XTick {
                row: ri,
                label: (seams.format_axis_tick)(value.unwrap_or(&ChartValue::Null)),
            });
            xs.push((ri as f32 + 0.5) / row_count as f32);
            ri += stride;
        }
    }
    (ticks, xs)
}

// ---- The geometry plan --------------------------------------------------------

/// One series' paint plan.
#[derive(Clone, Debug)]
pub(crate) struct LineSeriesPlan {
    /// React `dataKey`.
    pub(crate) key: String,
    /// `colors[i % colors.length]` — the stroke, the dot fill and the label
    /// swatch all read it.
    pub(crate) color: rune::Color,
    /// `dashMap[yKey]` (the on/off lengths, `None` = solid).
    pub(crate) dash: Option<&'static [f32]>,
    /// `hiddenSeries.has(yKey)`.
    pub(crate) hidden: bool,
    /// Plot fractions per data row (`None` = the row has no drawable point —
    /// recharts' gap on null/NaN: no line segment, no dot, no label).
    pub(crate) points: Vec<Option<[f32; 2]>>,
    /// The `LabelList` text per row when `config.showValues` (`None` = no
    /// drawable point, or the toggle is off).
    pub(crate) labels: Vec<Option<String>>,
}

/// Everything one frame of the engine paints, computed before any element:
/// the memoised pipeline of the React component (logHint → chartData →
/// dashMap → `<Line>` geometry → `<LabelList>` text), observable without a
/// runtime.
#[derive(Clone, Debug)]
pub(crate) struct LineChartPlan {
    /// `chartData`: `mapToLogScale`'s output when `log_hint.use_log`, else the
    /// input.
    pub(crate) chart_data: Vec<ChartDataPoint>,
    pub(crate) log_hint: LogScaleHint,
    /// `config.xAxis ?? '__index'`.
    pub(crate) x_key: String,
    /// `config.yAxes.length > 1` (the legend gate).
    pub(crate) multi_series: bool,
    pub(crate) series: Vec<LineSeriesPlan>,
    pub(crate) y_domain: [f64; 2],
    /// Top first (the first is the domain max).
    pub(crate) y_ticks: Vec<YTick>,
    pub(crate) x_ticks: Vec<XTick>,
    pub(crate) row_count: usize,
    /// Vertical grid line fractions (the X tick positions).
    pub(crate) grid_xs: Vec<f32>,
    /// Horizontal grid line fractions, top first (the Y tick positions).
    pub(crate) grid_ys: Vec<f32>,
}

/// The engine's whole plan for one frame: the React component's memo chain
/// (`logHint` → `chartData` → `dashMap` → `<Line>` geometry → `<LabelList>`
/// text), as pure data. The lean slices the chain reads are the area
/// engine's, imported — see the seams table in the module docs.
pub(crate) fn line_chart_plan(
    data: &[ChartDataPoint],
    config: &ChartConfig,
    hidden: &BTreeSet<String>,
    dark: bool,
    seams: &FormatSeams,
) -> LineChartPlan {
    let y_axes = &config.y_axes;
    let multi_series = y_axes.len() > 1;
    let log_hint = compute_log_scale_hint(data, y_axes);
    let chart_data = if log_hint.use_log {
        map_to_log_scale(data, y_axes, log_hint.domain_min)
    } else {
        data.to_vec()
    };
    // React builds the dash map from `chartData` (the log-mapped rows) and
    // only when multiSeries.
    let dash_map: BTreeMap<String, Option<&'static [f32]>> = if multi_series {
        build_overlap_dash_map(&chart_data, y_axes)
    } else {
        BTreeMap::new()
    };
    let row_count = chart_data.len();
    let x_key = config.x_axis.clone().unwrap_or_else(|| "__index".into());

    // The Y domain spans every series (hidden included — recharts does not
    // rescale when a series is hidden).
    let mut values: Vec<f64> = Vec::new();
    for key in y_axes {
        for point in &chart_data {
            if let Some(v) = to_number(point.get(key).unwrap_or(&ChartValue::Null)) {
                if v.is_finite() {
                    values.push(v);
                }
            }
        }
    }
    let y_domain = lean_y_domain(&values);

    let k = kit(dark);
    let palette = series_palette(k);
    let series: Vec<LineSeriesPlan> = y_axes
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let mut points = Vec::with_capacity(row_count);
            let mut labels = Vec::with_capacity(row_count);
            for (row, point) in chart_data.iter().enumerate() {
                let x = x_fraction(row, point, &x_key, config, row_count);
                let y = y_fraction(point, key, y_domain);
                match (x, y) {
                    (Some(x), Some(y)) => {
                        points.push(Some([x, y]));
                        // React 120-134: `formatter={logHint.use
                        // ? logTickFormatter : undefined}` — `logTickFormatter`
                        // is `formatCompact(10^v)` over the mapped value;
                        // `undefined` leaves recharts' default `String`
                        // display of the slot.
                        labels.push((config.show_values).then(|| {
                            let slot_value = point.get(key).unwrap_or(&ChartValue::Null);
                            if log_hint.use_log {
                                (seams.format_compact)(10.0_f64.powf(
                                    to_number(slot_value).unwrap_or(f64::NAN),
                                ))
                            } else {
                                match slot_value {
                                    ChartValue::Text(s) => s.clone(),
                                    ChartValue::Number(n) => js_string(*n),
                                    ChartValue::Null => String::new(),
                                }
                            }
                        }));
                    }
                    _ => {
                        // recharts draws no line point, no dot and no label
                        // for a null/NaN slot.
                        points.push(None);
                        labels.push(None);
                    }
                }
            }
            LineSeriesPlan {
                key: key.clone(),
                color: palette[i % palette.len()],
                dash: dash_map.get(key).and_then(|d| *d),
                hidden: hidden.contains(key),
                points,
                labels,
            }
        })
        .collect();

    // Y ticks, top first: the first is the domain max. The label is
    // `logTickFormatter` (`formatCompact(10^v)`) under the log hint, else
    // `formatCompact(v)` — React's two YAxis arms.
    let span = y_domain[1] - y_domain[0];
    let y_ticks: Vec<YTick> = (0..Y_TICKS)
        .map(|t| {
            let value = y_domain[1] - span * t as f64 / (Y_TICKS - 1) as f64;
            let label = if log_hint.use_log {
                (seams.format_compact)(10.0_f64.powf(value))
            } else {
                (seams.format_compact)(value)
            };
            YTick { value, label }
        })
        .collect();

    let (x_ticks, grid_xs) = x_tick_plan(&chart_data, config, &x_key, row_count, seams);
    let grid_ys: Vec<f32> = (0..Y_TICKS)
        .map(|t| t as f32 / (Y_TICKS - 1) as f32)
        .collect();

    LineChartPlan {
        chart_data,
        log_hint,
        x_key,
        multi_series,
        series,
        y_domain,
        y_ticks,
        x_ticks,
        row_count,
        grid_xs,
        grid_ys,
    }
}

// ---- The shapes ---------------------------------------------------------------

/// The `CartesianGrid`: the hairlines at the tick positions (React mounts it
/// always; `showGrid` false is `opacity: 0`, so the canvas stays and paints
/// nothing — the honest invisible grid).
fn grid_shape(plan: &LineChartPlan, k: &Kit, show_grid: bool) -> Shape {
    let mut shape = Shape::new().view_box(1.0, 1.0);
    if show_grid {
        let c = k.alpha(k.c.border_base, GRID_OPACITY); // stroke + opacity 0.3
        for f in &plan.grid_ys {
            shape = shape.line([0.0, *f], [1.0, *f], GRID_LINE_W, LineCap::Butt, c);
        }
        for f in &plan.grid_xs {
            shape = shape.line([*f, 0.0], [*f, 1.0], GRID_LINE_W, LineCap::Butt, c);
        }
    }
    shape
}

/// The axis frame: recharts' `stroke="var(--c-edge)"` axis lines (the Y axis
/// at the plot's left, the X axis at its bottom) on the edge token.
fn axes_shape(k: &Kit) -> Shape {
    Shape::new()
        .view_box(1.0, 1.0)
        .line([0.0, 0.0], [0.0, 1.0], AXIS_LINE_W, LineCap::Butt, k.c.border_base)
        .line([0.0, 1.0], [1.0, 1.0], AXIS_LINE_W, LineCap::Butt, k.c.border_base)
}

/// One series: the `strokeWidth: 2` line through the drawn points, dashed
/// through [`dashed_segments`] when the overlap map says so — no area fill;
/// the `<Line>` of the source carries none.
fn series_shape(series: &LineSeriesPlan) -> Shape {
    let mut shape = Shape::new().view_box(1.0, 1.0);
    for run in present_runs(&series.points) {
        shape = match series.dash {
            Some(pattern) => {
                let mut s = shape;
                for seg in dashed_segments(&run, pattern) {
                    s = s.stroke(seg, STROKE_W, series.color);
                }
                s
            }
            None => shape.stroke(run, STROKE_W, series.color),
        };
    }
    shape
}

/// The `dot` layer: one `r: 3` disc at every drawn point of every visible
/// series (React `dot={config.timeDomain ? false : { r: 3, fill: color }}` —
/// the caller gates the layer on the time domain).
fn dots_shape(plan: &LineChartPlan) -> Shape {
    let mut shape = Shape::new().view_box(1.0, 1.0);
    for series in plan.series.iter().filter(|s| !s.hidden) {
        for point in series.points.iter().flatten() {
            shape = shape.fill(gon(*point, DOT_R, 16), series.color);
        }
    }
    shape
}

/// The `activeDot`s at the hovered row: one `r: 5` disc per visible series
/// (React `activeDot={config.timeDomain ? false : { r: 5, ... }}` — the
/// caller gates the layer on the time domain).
fn active_dots_shape(plan: &LineChartPlan, index: usize) -> Shape {
    let mut shape = Shape::new().view_box(1.0, 1.0);
    for series in plan.series.iter().filter(|s| !s.hidden) {
        if let Some([x, y]) = series.points.get(index).copied().flatten() {
            shape = shape.fill(gon([x, y], ACTIVE_DOT_R, 16), series.color);
        }
    }
    shape
}

// ---- The tooltip --------------------------------------------------------------

/// The hovered row's entries: the visible series only (recharts' payload).
/// Under the log hint this is exactly T136's formatter — the `__orig__` slot
/// read back through `formatCompact` — and `10^logValue` when the slot is
/// absent; the plain tooltip is recharts' default content (the value's JS
/// display).
fn tooltip_entries(
    plan: &LineChartPlan,
    index: usize,
    seams: &FormatSeams,
) -> Vec<LogScaleTooltipEntry> {
    let point = &plan.chart_data[index];
    plan.series
        .iter()
        .filter(|s| !s.hidden)
        .map(|series| {
            if plan.log_hint.use_log {
                format_entry(
                    point.get(&series.key).unwrap_or(&ChartValue::Null),
                    &ChartValue::Text(series.key.clone()),
                    &chart_payload(point),
                    &|v| (seams.format_compact)(v),
                )
            } else {
                LogScaleTooltipEntry {
                    name: series.key.clone(),
                    value: match point.get(&series.key) {
                        Some(ChartValue::Text(s)) => s.clone(),
                        Some(ChartValue::Number(n)) => js_string(*n),
                        _ => String::new(), // null or absent
                    },
                }
            }
        })
        .collect()
}

/// recharts' default entry line: the name, its `' : '` separator, the value
/// (T136's join; a nameless entry is the value alone).
fn entry_line(entry: &LogScaleTooltipEntry) -> String {
    if entry.name.is_empty() {
        entry.value.clone()
    } else {
        format!("{} : {}", entry.name, entry.value)
    }
}

/// The plain `<Tooltip>`'s content box: React's `contentStyle` chrome on the
/// same token mapping T136 documented (surface-alt → `layer_01`, edge →
/// `border_base`, radius 6 → `radius.md`, fg → `text_strong`, 12px →
/// `text_xsmall`).
fn plain_tooltip_box(entries: &[LogScaleTooltipEntry], k: &Kit) -> El {
    col()
        .key(TOOLTIP_KEY)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(k.radius.md)
        .children(entries.iter().enumerate().map(|(i, entry)| {
            text(entry_line(entry))
                .key(format!("{TOOLTIP_KEY}-item-{i}"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_strong)
        }))
}

/// The tooltip element, anchored at the hovered bucket's top-left.
fn tooltip_el(
    plan: &LineChartPlan,
    index: usize,
    seams: &FormatSeams,
    dark: bool,
    k: &Kit,
) -> El {
    let entries = tooltip_entries(plan, index, seams);
    let body = if plan.log_hint.use_log {
        LogScaleTooltip::new(&entries).build(dark)
    } else {
        plain_tooltip_box(&entries, k)
    };
    body.absolute(TOOLTIP_DX, TOOLTIP_DY)
}

// ---- The engine ---------------------------------------------------------------

/// The engine's retained state — React's `useState` on the immediate-mode
/// side of the seam: the hidden-series set and the hovered row
/// (`activeTooltipIndex`).
#[derive(Default)]
struct EngineState {
    /// React `hiddenSeries` (`useState<Set<string>>`).
    hidden: BTreeSet<String>,
    /// recharts' `activeTooltipIndex` while the pointer is inside a bucket.
    hovered: Option<usize>,
}

/// The line chart engine. Cloning shares [`EngineState`] (and the seams), so a
/// factory that clones one engine per build keeps the user's toggles and
/// hover across frames — the component's retained state.
#[derive(Clone)]
pub(crate) struct LineChartRenderer {
    state: Rc<RefCell<EngineState>>,
    seams: FormatSeams,
}

impl LineChartRenderer {
    pub(crate) fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(EngineState::default())),
            seams: FormatSeams::default(),
        }
    }

    /// Wire the T079 `formatCompact` port (or a test probe).
    pub(crate) fn format_compact(mut self, f: Rc<dyn Fn(f64) -> String>) -> Self {
        self.seams.format_compact = f;
        self
    }

    /// Wire the T079 `formatAxisTick` port (or a test probe).
    pub(crate) fn format_axis_tick(mut self, f: Rc<dyn Fn(&ChartValue) -> String>) -> Self {
        self.seams.format_axis_tick = f;
        self
    }

    /// Wire the T079 `formatEpochMs` port (or a test probe).
    pub(crate) fn format_epoch_ms(mut self, f: Rc<dyn Fn(f64) -> String>) -> Self {
        self.seams.format_epoch_ms = f;
        self
    }

    /// React `hiddenSeries`, as the engine keeps it.
    pub(crate) fn hidden_series(&self) -> BTreeSet<String> {
        self.state.borrow().hidden.clone()
    }

    /// recharts' `activeTooltipIndex`, as the engine keeps it.
    pub(crate) fn hovered_row(&self) -> Option<usize> {
        self.state.borrow().hovered
    }

    /// React `toggleSeries`: insert-or-remove in the hidden set.
    pub(crate) fn toggle_series(&self, data_key: &str) {
        let mut state = self.state.borrow_mut();
        if !state.hidden.remove(data_key) {
            state.hidden.insert(data_key.to_string());
        }
    }
}

impl Default for LineChartRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// The canvas's renderer-factory injection point: `ChartType::Line` gets this
/// engine (cloned per build, sharing its state), every other type keeps the
/// honest placeholder. This is the wiring the chart-view track passes to
/// `ChartCanvas::renderer_factory`.
pub(crate) fn renderer_factory<S: 'static>(
    engine: LineChartRenderer,
) -> impl Fn(ChartType) -> Box<dyn ChartRenderer<S> + 'static> {
    move |chart_type| match chart_type {
        ChartType::Line => Box::new(engine.clone()),
        other => Box::new(PlaceholderChartRenderer::new(other)),
    }
}

impl<S: 'static> ChartRenderer<S> for LineChartRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        let k = kit(ctx.dark);
        let (hidden, hovered) = {
            let state = self.state.borrow();
            (state.hidden.clone(), state.hovered)
        };
        let plan = line_chart_plan(ctx.data, ctx.config, &hidden, ctx.dark, &self.seams);
        let hovered = hovered.filter(|i| *i < plan.row_count);
        build_el(ctx, &plan, hovered, &self.seams, k, &self.state)
    }
}

/// One hit cell: a `1/n` band over the plot. Hovering it sets the hovered row
/// (recharts' `activeTooltipIndex`); pressing it routes the row to
/// `onDataPointClick` (React's `onClick` arm); the tooltip mounts inside it.
#[allow(clippy::too_many_arguments)]
fn hit_cell<S: 'static>(
    ctx: &ChartRenderContext<'_, S>,
    plan: &LineChartPlan,
    row: usize,
    hovered: Option<usize>,
    seams: &FormatSeams,
    dark: bool,
    k: &Kit,
    state: &Rc<RefCell<EngineState>>,
) -> El {
    let mut cell = el().key(format!("{HIT_CELL_PREFIX}{row}")).a11y_hidden();
    let hover_state = Rc::clone(state);
    cell = cell.on_hover(move |_s: &mut S| {
        hover_state.borrow_mut().hovered = Some(row);
    });
    if let Some(on_click) = ctx.on_data_point_click.as_ref() {
        let on_click = Rc::clone(on_click);
        cell = cell.on_press(move |s: &mut S| on_click(s, row));
    }
    if hovered == Some(row) {
        cell = cell.child(tooltip_el(plan, row, seams, dark, k));
    }
    cell
}

/// The `LabelList` overlay: one band column per data row holding the series'
/// value labels stacked in declaration order at the top of the band — the
/// `position="top"` labels in the exact horizontal slot their point sits in.
/// Hidden series and value-less rows keep an empty slot so the visible
/// labels stay aligned with their series (the slots are fractions of the
/// parent, but the group is what stacks).
fn values_overlay(plan: &LineChartPlan, k: &Kit) -> El {
    let mut overlay = row()
        .key(VALUES_KEY)
        .absolute(0.0, 0.0)
        .w_full()
        .h_full();
    if plan.row_count == 0 || plan.series.is_empty() {
        return overlay;
    }
    let band = 1.0 / plan.row_count as f32;
    for r in 0..plan.row_count {
        let mut band_col = col()
            .key(format!("{VALUES_BAND_PREFIX}{r}"))
            .w_frac(band)
            .h_full()
            .justify_start(); // position="top": the labels ride the band's top
        for series in &plan.series {
            let label = if series.hidden {
                None
            } else {
                series.labels.get(r).cloned().flatten()
            };
            let slot_el = match label {
                Some(t) => text(t)
                    .key(format!("{VALUE_PREFIX}{r}-{}", series.key))
                    .w_full()
                    .font(k.ty.small) // LabelList fontSize: 11
                    .color(k.c.text_muted) // fill: var(--c-fg-secondary)
                    .align(Align::Center) // centred over the point's band
                    .wrap(),
                None => el().w_full(), // the empty slot keeps the geometry
            };
            band_col = band_col.child(slot_el);
        }
        overlay = overlay.child(band_col);
    }
    overlay
}

/// The tree: recharts' `<LineChart>` composition as the flexbox frame the
/// module docs describe.
#[allow(clippy::too_many_arguments)]
fn build_el<S: 'static>(
    ctx: &ChartRenderContext<'_, S>,
    plan: &LineChartPlan,
    hovered: Option<usize>,
    seams: &FormatSeams,
    k: &Kit,
    state: &Rc<RefCell<EngineState>>,
) -> El {
    let dark = ctx.dark;

    // ---- The plot row: the Y-axis column beside the stacked canvases ------
    let y_axis = col()
        .key(Y_AXIS_KEY)
        .w(Y_AXIS_W)
        .justify_between()
        .children(plan.y_ticks.iter().enumerate().map(|(t, tick)| {
            text(tick.label.clone())
                .key(format!("{Y_AXIS_KEY}-tick-{t}"))
                .w_full()
                .align(Align::End) // right against the axis, as recharts lays it
                .font(k.ty.text_xsmall) // tick fontSize: 12
                .color(k.c.text_muted) // fill: var(--c-fg-secondary) (the kit's collapsed ink)
                .wrap()
        }));

    let mut plot = el()
        .key(PLOT_KEY)
        .grow(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .child(
            canvas(grid_shape(plan, k, ctx.config.show_grid))
                .key(GRID_KEY)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        )
        .child(
            canvas(axes_shape(k))
                .key(AXES_KEY)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        );
    for series in plan.series.iter().filter(|s| !s.hidden) {
        plot = plot.child(
            canvas(series_shape(series))
                .key(format!("{SERIES_KEY_PREFIX}{}", series.key))
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        );
    }
    // React `dot={config.timeDomain ? false : { r: 3, ... }}`: the per-point
    // discs ride every drawn point unless the time domain turned them off.
    if ctx.config.time_domain.is_none() {
        plot = plot.child(
            canvas(dots_shape(plan))
                .key(DOTS_KEY)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        );
    }
    // React `activeDot={config.timeDomain ? false : { r: 5, ... }}`: the
    // hovered row's discs, same gate.
    if let (Some(row), None) = (hovered, ctx.config.time_domain) {
        plot = plot.child(
            canvas(active_dots_shape(plan, row))
                .key(ACTIVE_DOTS_KEY)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        );
    }
    // The LabelList, under the hit layer: labels never block the pointer.
    if ctx.config.show_values {
        plot = plot.child(values_overlay(plan, k));
    }

    // The hit layer: one 1/n band per row, over everything painted.
    let clear_state = Rc::clone(state);
    let mut zone = el()
        .key(HIT_ZONE_KEY)
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .on_hover_change(move |_s: &mut S, inside: bool, tx: &mut Tx| {
            if !inside {
                clear_state.borrow_mut().hovered = None; // the pointer left the chart
                // recharts' tooltip has no exit animation: it unmounts at once.
                tx.instant();
            }
        });
    for row in 0..plan.row_count {
        let band = 1.0 / plan.row_count as f32;
        zone = zone.child(
            hit_cell(ctx, plan, row, hovered, seams, dark, k, state)
                .w_frac(band)
                .h_full(),
        );
    }
    plot = plot.child(zone);

    let mut root = col()
        .key(ROOT_KEY)
        .w_full()
        .h_full()
        .min_w(0.0)
        .min_h(0.0)
        .child(
            row()
                .key(PLOT_ROW_KEY)
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .child(y_axis)
                .child(plot),
        );

    // ---- The X-axis row: under the plot area, beside the Y axis --------
    // (recharts lays the X axis inside the chart's plot frame, right of the
    // Y axis — the row keeps that alignment with a Y_AXIS_W shim.)
    let mut x_axis = row().key(X_AXIS_KEY).h(X_AXIS_H).grow(1.0).min_w(0.0);
    for row in 0..plan.row_count {
        let band = 1.0 / plan.row_count as f32;
        match plan.x_ticks.iter().find(|t| t.row == row) {
            Some(tick) => {
                x_axis = x_axis.child(
                    text(tick.label.clone())
                        .key(format!("{X_AXIS_KEY}-tick-{row}"))
                        .w_frac(band)
                        .align(Align::Center) // centred in its band
                        .font(k.ty.text_xsmall) // tick fontSize: 12
                        .color(k.c.text_muted)
                        .wrap(),
                );
            }
            None => x_axis = x_axis.child(el().w_frac(band)),
        }
    }
    root = root.child(row().child(el().w(Y_AXIS_W)).child(x_axis));

    // ---- The legend (multiSeries gate, as React mounts it) ----------------
    if plan.multi_series {
        root = root.child(legend_row::<S>(plan, k, state));
    }
    root
}

/// The legend row: one pressable entry per series (recharts' default entry —
/// a colour swatch and the series name; the name dims to 0.35 when hidden,
/// React's formatter span).
fn legend_row<S: 'static>(
    plan: &LineChartPlan,
    k: &Kit,
    state: &Rc<RefCell<EngineState>>,
) -> El {
    let mut bar = row()
        .key(LEGEND_KEY)
        .h(LEGEND_H)
        .w_full()
        .center()
        .gap(LEGEND_GAP);
    for series in &plan.series {
        let toggle_state = Rc::clone(state);
        let key = series.key.clone();
        let item = row()
            .key(format!("{LEGEND_ITEM_PREFIX}{key}"))
            .role("button")
            .label(key.clone())
            .help(key.clone())
            .center()
            .gap(LEGEND_ENTRY_GAP)
            .radius(k.radius.sm)
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus)
            .child(
                el().key(format!("{LEGEND_SWATCH_PREFIX}{key}"))
                    .w(SWATCH)
                    .h(SWATCH)
                    .radius(k.radius.sm)
                    .fill(series.color),
            )
            .child(
                text(key.clone())
                    .key(format!("{LEGEND_LABEL_PREFIX}{key}"))
                    .font(k.ty.text_xsmall) // wrapperStyle fontSize: 12
                    .color(k.c.text_strong) // color: var(--c-fg, #eee)
                    .opacity(if series.hidden { 0.35 } else { 1.0 }), // the formatter's dimmed span
            )
            .on_press_tx(move |_s: &mut S, tx: &mut Tx| {
                // React's toggleSeries: insert-or-remove in the hidden set. The
                // Lines carry `isAnimationActive={false}`, so the toggle lands
                // at once rather than fading the series out.
                let mut st = toggle_state.borrow_mut();
                if !st.hidden.remove(&key) {
                    st.hidden.insert(key.clone());
                }
                tx.instant();
            });
        bar = bar.child(item);
    }
    bar
}
