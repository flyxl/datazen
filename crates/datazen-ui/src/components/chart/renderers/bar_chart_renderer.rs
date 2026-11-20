//! Port of `packages/frontend/src/components/chart/renderers/BarChartRenderer.tsx` (T192).
//!
//! The React component is a `recharts` `<BarChart>` composition: the same
//! log-scale hint as the area engine (`computeLogScaleHint` → `mapToLogScale`,
//! stashing originals under `__orig__${key}`), a `useState<Set<string>>` that
//! hides series from the legend, and one `<Bar>` per Y axis — `fill` from the
//! colour palette, `radius={[3,3,0,0]}`, `hide`, and the per-Bar
//! `onClick(_, index)` that routes a data row — plus the `<CartesianGrid>`
//! hairlines, the two plain axes with their tick formatters, the `<Legend>`
//! that toggles, `<LogScaleTooltip>`/`<Tooltip>` content, and the
//! `<LabelList position="top">` the `showValues` toggle mounts.
//!
//! This is the second chart engine on the `ChartRenderer` seam
//! (`chart_canvas.rs`); the area engine (T166) landed first and this file
//! follows its shape line for line — the same plot frame, the same hit-band
//! routing, the same tooltip mounts — swapping the area polygons for bar
//! rectangles and adding the LabelList the area source does not have.
//!
//! ## What the React source does not have
//!
//! The family's task sketch mentions stacked/grouped modes, a horizontal
//! orientation, `maxBarSize` and a dash map: **the React file has none of
//! them** — no `stackId`, the default vertical layout, no `maxBarSize`, and
//! `<Bar>` carries no `strokeDasharray` (the area source's
//! `buildOverlapDashMap` arm is absent here). The port matches the source:
//! vertical grouped bars only; stacking, orientations and per-bar size caps
//! have nothing to port.
//!
//! ## How the recharts composition maps onto Rune
//!
//! * **Real geometry, not an approximation.** Each series paints its bars as
//!   `rune::Shape` rectangle fills written in the `view_box(1, 1)` fractions
//!   of the plot, so one shape stretches to whatever rect layout gives the
//!   plot. Grid hairlines and the axis frame are stroke/line ops. The bar
//!   rect model lives in [`BarChartPlan`] as pure data (`[x, y, w, h]`
//!   fractions per row), observable without a runtime.
//! * **The plot frame** is the area engine's flexbox rebuild of the recharts
//!   margin box: a 60pt Y-axis column (`justify_between` over the five
//!   ticks — recharts' default YAxis width), the growing plot stack, a 30pt
//!   X-axis row (recharts' default XAxis height) of per-band tick slots, and
//!   the 22pt legend row when `multiSeries`. Overlapping canvases stack
//!   through `.absolute(0.0, 0.0)` plus `.w_full().h_full()` — the shimmer
//!   overlay pattern — so grid, axes, every series, the value labels and the
//!   hit layer share one rect.
//!
//! ## The seams (the split discipline — shared with T166, not forked)
//!
//! | React import | owner | shape here |
//! |--------------|-------|------------|
//! | `computeLogScaleHint` / `mapToLogScale` (`lib/chart/transform.ts`) | T182 | `area_chart_renderer`'s [`compute_log_scale_hint`] / [`map_to_log_scale`] — imported, not re-ported (one seam family, both engines) |
//! | the Y domain + series slots' coercions | T166's lean slices | [`lean_y_domain`] / [`to_number`] / [`js_string`] / [`series_palette`] — imported from the area engine with shared-slice doc notes |
//! | `formatAxisTick` / `formatCompact` (`lib/chart/format.ts`) | T079 | [`FormatSeams`] — the area engine's injected `Rc<dyn Fn>` pair (the seam struct also carries `formatEpochMs`, which this source never imports: the Bar XAxis has no time-domain arm) |
//! | `getColorPalette` (`lib/chart/colors.ts`) | T352 | [`series_palette`] — the lean five-colour kit palette, shared with the area engine |
//! | `LogScaleTooltip` (T136, landed) | T136 | `super::super::log_scale_tooltip`'s `format_entry`/`build`, mounted by this engine |
//!
//! The Y/X tick model ([`YTick`]/[`XTick`]) is the area engine's, imported for
//! the same reason: it is the axis model, not an area-chart particular.
//!
//! ## Interaction (immediate mode)
//!
//! * The `useState<Set<string>>` becomes [`EngineState`] behind an
//!   `Rc<RefCell<..>>` shared by every clone of the engine: the legend entry's
//!   press calls React's insert-or-remove, and the app factory clones the same
//!   engine per build, so the set survives frames — the retained state React
//!   kept.
//! * recharts' per-`<Bar>` `onClick(_, index)` hands the **data row index**;
//!   that routes through one hit cell per row (`w_frac(1/n)` bands over the
//!   plot) whose press calls `on_dataPointClick(row_index)`. Hovering a cell
//!   sets the hovered row (recharts' tooltip `activeTooltipIndex`), the box
//!   mounts inside that cell, and leaving the chart clears it — the area
//!   engine's routing, verbatim.
//! * `cursor={onDataPointClick ? 'pointer' : undefined}` has no Rune
//!   counterpart (the same note T166 made).
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | root | `bar-chart` |
//! | plot row / plot stack | `bar-chart-plot-row` / `bar-chart-plot` |
//! | grid canvas / axes canvas | `bar-chart-grid` / `bar-chart-axes` |
//! | series canvas | `bar-chart-series-{key}` |
//! | values overlay / band / label | `bar-chart-values` / `bar-chart-values-band-{row}` / `bar-chart-value-{row}-{key}` |
//! | hit layer / hit cell *i* | `bar-chart-hit-zone` / `bar-chart-hit-{i}` |
//! | Y axis / Y tick *k* | `bar-chart-yaxis` / `bar-chart-yaxis-tick-{k}` |
//! | X axis / X tick *i* | `bar-chart-xaxis` / `bar-chart-xaxis-tick-{i}` |
//! | legend / item / swatch / label | `bar-chart-legend` / `bar-chart-legend-item-{key}` / `…-swatch-{key}` / `…-label-{key}` |
//! | the plain tooltip box | `bar-chart-tooltip` |
//! | the log-scale tooltip box | `chart-log-scale-tooltip` (T136's `CONTENT_KEY`) |
//!
//! ## Deliberate deviations
//!
//! * **Bar sizing, lean.** recharts computes bar sizes from `barCategoryGap`
//!   (default `'10%'`, the gap between categories) and `barGap` (default `4`,
//!   the pixel gap between same-category bars) against the measured band. In
//!   the fraction canvas the lean rule is: band `1/n` per row, the category's
//!   content is the band's 90% centred, and the `yAxes` series split the
//!   content into equal slots (hidden series keep theirs — recharts' legend
//!   toggle hides a series without relayout or rescaling, the behaviour T166
//!   pinned for the domain). The pixel `barGap` has no view-box counterpart:
//!   the slots abut exactly.
//! * `<Bar radius={[3,3,0,0]}>`'s 3pt rounded top corners are not painted —
//!   Rune's `Shape` has no rounded-rect primitive and a 3pt radius cannot live
//!   in `view_box(1, 1)` fractions. The rect geometry (x/y/w/h) is exact; the
//!   corner is the cosmetic traded, the same class of trade as T166's
//!   `monotone` spline.
//! * The `LabelList position="top"` mounts in the exact band and series slot
//!   its bar occupies, at the top of the band overlay — Rune's flexbox has no
//!   fraction-offset anchor for a text node (the area engine's tooltip-anchor
//!   deviation class), so the label rides the plot's top edge instead of each
//!   bar's own top. `fontSize: 11` reads `k.ty.small` (the kit's 11px face).
//! * A zero-value bar keeps its slot, its `Some([x, y0, w, 0])` rect and its
//!   `"0"` label but paints nothing (a zero-height fill is skipped).
//! * The zero baseline clamps to the plot when the domain does not contain
//!   zero (the flat-domain arms); recharts draws to the true zero line and
//!   lets the plot clip it.
//! * Y ticks are five quarter-domain values and X ticks every `ceil(n/8)`-th
//!   band — recharts' "nice" tick picking is not ported (T166's lean, same
//!   trade). `CartesianGrid`'s `strokeDasharray="3 3"` paints as solid
//!   hairlines (Rune strokes carry no dash); this source has no series dash
//!   array at all.
//! * The hit cells are `a11y_hidden` (a 1000-row chart would otherwise be a
//!   wall of buttons); the legend entries are real focusable buttons — T166's
//!   decision, kept.

#![allow(dead_code)] // the chart-view track wires the factory; until then only tests exercise it

use std::cell::RefCell;
use std::collections::BTreeSet;
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
    compute_log_scale_hint, js_string, lean_y_domain, map_to_log_scale, series_palette, to_number,
    LogScaleHint, XTick, YTick,
};
/// The family's `lib/chart/format.ts` seam struct (T079) — the area engine's,
/// re-exported so the bar engine's callers wire it through this module.
pub(crate) use super::area_chart_renderer::FormatSeams;
use crate::ui::kit::kit;
use rune_kit::Kit;

// ---- Keys -------------------------------------------------------------------

/// Root, recharts' `<BarChart>` frame.
pub(crate) const ROOT_KEY: &str = "bar-chart";
/// The Y-axis + plot stack row.
pub(crate) const PLOT_ROW_KEY: &str = "bar-chart-plot-row";
/// The plot itself — every canvas, the labels and the hit layer share its rect.
pub(crate) const PLOT_KEY: &str = "bar-chart-plot";
/// The `CartesianGrid` canvas.
pub(crate) const GRID_KEY: &str = "bar-chart-grid";
/// The axis frame canvas (recharts' axis lines).
pub(crate) const AXES_KEY: &str = "bar-chart-axes";
/// One canvas per series: `bar-chart-series-{key}`.
pub(crate) const SERIES_KEY_PREFIX: &str = "bar-chart-series-";
/// The `LabelList` overlay (React 93-101, `config.showValues`).
pub(crate) const VALUES_KEY: &str = "bar-chart-values";
/// One band column per data row inside the overlay: `…-band-{row}`.
pub(crate) const VALUES_BAND_PREFIX: &str = "bar-chart-values-band-";
/// One value label per row/series: `bar-chart-value-{row}-{key}`.
pub(crate) const VALUE_PREFIX: &str = "bar-chart-value-";
/// The pointer/press layer over the plot.
pub(crate) const HIT_ZONE_KEY: &str = "bar-chart-hit-zone";
/// One hit cell per data row: `bar-chart-hit-{i}`.
pub(crate) const HIT_CELL_PREFIX: &str = "bar-chart-hit-";
/// The Y-axis label column.
pub(crate) const Y_AXIS_KEY: &str = "bar-chart-yaxis";
/// The X-axis label row.
pub(crate) const X_AXIS_KEY: &str = "bar-chart-xaxis";
/// The legend (multiSeries only).
pub(crate) const LEGEND_KEY: &str = "bar-chart-legend";
/// One entry per series: `bar-chart-legend-item-{key}` (+ `-swatch-`/`-label-`).
pub(crate) const LEGEND_ITEM_PREFIX: &str = "bar-chart-legend-item-";
/// The swatch of legend entry `{key}`.
pub(crate) const LEGEND_SWATCH_PREFIX: &str = "bar-chart-legend-swatch-";
/// The label of legend entry `{key}` (dims to 0.35 when hidden).
pub(crate) const LEGEND_LABEL_PREFIX: &str = "bar-chart-legend-label-";
/// The plain (non-log) `<Tooltip>` content box.
pub(crate) const TOOLTIP_KEY: &str = "bar-chart-tooltip";

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
/// The axis frame's stroke.
const AXIS_LINE_W: f32 = 1.0;
/// The grid hairlines' stroke.
const GRID_LINE_W: f32 = 1.0;
/// React `CartesianGrid` `opacity` when `showGrid`.
const GRID_OPACITY: f32 = 0.3;
/// The tooltip's anchor inside the hovered band.
const TOOLTIP_DX: f32 = 2.0;
/// The tooltip's anchor below the plot's top.
const TOOLTIP_DY: f32 = 8.0;
/// recharts' `barCategoryGap` default `'10%'` — the gap between categories,
/// as a share of the band (each category's bars span the centred 90%).
const BAR_CATEGORY_GAP: f32 = 0.10;
/// Y ticks: recharts' default five-ish even ticks, lean (T166's trade).
const Y_TICKS: usize = 5;
/// X ticks: the lean recharts-interval cap.
const MAX_X_TICKS: usize = 8;

// ---- The geometry plan ---------------------------------------------------------

/// One series' paint plan.
#[derive(Clone, Debug)]
pub(crate) struct BarSeriesPlan {
    /// React `dataKey`.
    pub(crate) key: String,
    /// `colors[i % colors.length]`.
    pub(crate) color: rune::Color,
    /// `hiddenSeries.has(yKey)`.
    pub(crate) hidden: bool,
    /// The bar rectangles as `[x, y, w, h]` view-box fractions, one per data
    /// row (`None` = the row has no drawable value — recharts paints no bar
    /// for null/NaN). A zero value keeps a zero-height rect: the slot and the
    /// label stay, the fill does not.
    pub(crate) rects: Vec<Option<[f32; 4]>>,
    /// The `LabelList` text per row when `config.showValues` (`None` = no bar
    /// to label, or the toggle is off).
    pub(crate) labels: Vec<Option<String>>,
}

/// Everything one frame of the engine paints, computed before any element:
/// the memoised pipeline of the React component (logHint → chartData → bar
/// geometry), observable without a runtime.
#[derive(Clone, Debug)]
pub(crate) struct BarChartPlan {
    /// `chartData`: `mapToLogScale`'s output when `log_hint.use_log`, else the
    /// input.
    pub(crate) chart_data: Vec<ChartDataPoint>,
    pub(crate) log_hint: LogScaleHint,
    /// `config.xAxis ?? '__index'`.
    pub(crate) x_key: String,
    /// `config.yAxes.length > 1` (the legend gate).
    pub(crate) multi_series: bool,
    pub(crate) series: Vec<BarSeriesPlan>,
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
/// (`logHint` → `chartData` → `<Bar>` geometry → `<LabelList>` text), as pure
/// data. The lean slices the chain reads are the area engine's, imported —
/// see the seams table in the module docs.
pub(crate) fn bar_chart_plan(
    data: &[ChartDataPoint],
    config: &ChartConfig,
    hidden: &BTreeSet<String>,
    dark: bool,
    seams: &FormatSeams,
) -> BarChartPlan {
    let y_axes = &config.y_axes;
    let multi_series = y_axes.len() > 1;
    let log_hint = compute_log_scale_hint(data, y_axes);
    let chart_data = if log_hint.use_log {
        map_to_log_scale(data, y_axes, log_hint.domain_min)
    } else {
        data.to_vec()
    };
    let row_count = chart_data.len();
    let x_key = config.x_axis.clone().unwrap_or_else(|| "__index".into());

    // The Y domain spans every series (hidden included — recharts does not
    // rescale when a series is hidden; the bar slots keep theirs too).
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
    let span = y_domain[1] - y_domain[0];

    // The zero baseline recharts hangs every bar from. A domain that does not
    // contain zero (the flat arms) clamps to the nearest edge — recharts
    // draws to the true zero line and lets the plot clip it.
    let zero_y = (1.0 - (0.0 - y_domain[0]) / span).clamp(0.0, 1.0) as f32;

    let k = kit(dark);
    let palette = series_palette(k);
    let series_count = y_axes.len();
    let band = if row_count > 0 { 1.0 / row_count as f32 } else { 0.0 };
    // recharts' barCategoryGap '10%': the category's content is the centred
    // 90% of its band; the series split it into equal slots.
    let content = band * (1.0 - BAR_CATEGORY_GAP);
    let slot = if series_count > 0 {
        content / series_count as f32
    } else {
        0.0
    };

    let series: Vec<BarSeriesPlan> = y_axes
        .iter()
        .enumerate()
        .map(|(i, key)| {
            let mut rects = Vec::with_capacity(row_count);
            let mut labels = Vec::with_capacity(row_count);
            for (r, point) in chart_data.iter().enumerate() {
                let slot_value = point.get(key).unwrap_or(&ChartValue::Null);
                let v = to_number(slot_value).filter(|v| v.is_finite());
                match v {
                    None => {
                        rects.push(None); // recharts paints no bar for null/NaN
                        labels.push(None);
                    }
                    Some(v) => {
                        // The series slot inside its row's band content.
                        let x = r as f32 * band + band * BAR_CATEGORY_GAP * 0.5 + i as f32 * slot;
                        // The value's top edge and the baseline it hangs from.
                        let y_v = (1.0 - (v - y_domain[0]) / span) as f32;
                        let y = y_v.min(zero_y);
                        let h = (y_v - zero_y).abs();
                        rects.push(Some([x, y, slot, h]));
                        labels.push((config.show_values).then(|| {
                            // React 93-101: `formatter={logHint.use
                            // ? (v) => logTickFormatter(Number(v)) :
                            // undefined}` — `logTickFormatter` is
                            // `formatCompact(10^v)` over the mapped value;
                            // `undefined` leaves recharts' default `String`
                            // display of the slot.
                            if log_hint.use_log {
                                (seams.format_compact)(10.0_f64.powf(v))
                            } else {
                                match slot_value {
                                    ChartValue::Text(s) => s.clone(),
                                    ChartValue::Number(n) => js_string(*n),
                                    ChartValue::Null => String::new(),
                                }
                            }
                        }));
                    }
                }
            }
            BarSeriesPlan {
                key: key.clone(),
                color: palette[i % palette.len()],
                hidden: hidden.contains(key),
                rects,
                labels,
            }
        })
        .collect();

    // Y ticks, top first: the first is the domain max. The label is
    // `logTickFormatter` (`formatCompact(10^v)`) under the log hint, else
    // `formatCompact(v)` — React's two YAxis arms.
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

    // X ticks: every `ceil(n/8)`-th band (the lean recharts-interval cap).
    // The Bar source's XAxis has no time-domain arm, so there is only this
    // one ladder — the tick label is `formatAxisTick` of the row's slot.
    let mut x_ticks = Vec::new();
    let mut grid_xs = Vec::new();
    if row_count > 0 {
        let stride = row_count.div_ceil(MAX_X_TICKS);
        let mut r = 0;
        while r < row_count {
            x_ticks.push(XTick {
                row: r,
                label: (seams.format_axis_tick)(
                    chart_data[r].get(&x_key).unwrap_or(&ChartValue::Null),
                ),
            });
            grid_xs.push((r as f32 + 0.5) / row_count as f32);
            r += stride;
        }
    }
    let grid_ys: Vec<f32> = (0..Y_TICKS)
        .map(|t| t as f32 / (Y_TICKS - 1) as f32)
        .collect();

    BarChartPlan {
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
fn grid_shape(plan: &BarChartPlan, k: &Kit, show_grid: bool) -> Shape {
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

/// One series' bars: a rectangle fill per drawable row, in the series colour
/// (React's `<Bar fill>` — recharts' Bar has no stroke by default). The 3pt
/// rounded top corners are the documented cosmetic trade; a zero-height rect
/// (a zero value) paints nothing.
fn series_shape(series: &BarSeriesPlan) -> Shape {
    let mut shape = Shape::new().view_box(1.0, 1.0);
    for rect in series.rects.iter().flatten() {
        let [x, y, w, h] = *rect;
        if w > 0.0 && h > 0.0 {
            shape = shape.fill(
                vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]],
                series.color,
            );
        }
    }
    shape
}

// ---- The tooltip ---------------------------------------------------------------

/// The hovered row's entries: the visible series only (recharts' payload).
/// Under the log hint this is exactly T136's formatter — the `__orig__` slot
/// read back through `formatCompact` — and `10^logValue` when the slot is
/// absent; the plain tooltip is recharts' default content (the value's JS
/// display).
fn tooltip_entries(
    plan: &BarChartPlan,
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

/// The tooltip element, anchored at the hovered band's top-left.
fn tooltip_el(
    plan: &BarChartPlan,
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
    /// recharts' `activeTooltipIndex` while the pointer is inside a band.
    hovered: Option<usize>,
}

/// The bar chart engine. Cloning shares [`EngineState`] (and the seams), so a
/// factory that clones one engine per build keeps the user's toggles and
/// hover across frames — the component's retained state.
#[derive(Clone)]
pub(crate) struct BarChartRenderer {
    state: Rc<RefCell<EngineState>>,
    seams: FormatSeams,
}

impl BarChartRenderer {
    pub(crate) fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(EngineState::default())),
            seams: FormatSeams::default(),
        }
    }

    /// Wire the T079 `formatCompact` port (or a test probe) — the Y tick
    /// formatter, the `logTickFormatter` arms and the tooltip values.
    pub(crate) fn format_compact(mut self, f: Rc<dyn Fn(f64) -> String>) -> Self {
        self.seams.format_compact = f;
        self
    }

    /// Wire the T079 `formatAxisTick` port (or a test probe) — the
    /// categorical X tick formatter. (`formatEpochMs` stays in the shared
    /// `FormatSeams` but is never wired here: the Bar source's XAxis has no
    /// time-domain arm and never imports it.)
    pub(crate) fn format_axis_tick(mut self, f: Rc<dyn Fn(&ChartValue) -> String>) -> Self {
        self.seams.format_axis_tick = f;
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

impl Default for BarChartRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// The canvas's renderer-factory injection point: `ChartType::Bar` gets this
/// engine (cloned per build, sharing its state), every other type keeps the
/// honest placeholder. This is the wiring the chart-view track passes to
/// `ChartCanvas::renderer_factory`.
pub(crate) fn renderer_factory<S: 'static>(
    engine: BarChartRenderer,
) -> impl Fn(ChartType) -> Box<dyn ChartRenderer<S> + 'static> {
    move |chart_type| match chart_type {
        ChartType::Bar => Box::new(engine.clone()),
        other => Box::new(PlaceholderChartRenderer::new(other)),
    }
}

impl<S: 'static> ChartRenderer<S> for BarChartRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        let k = kit(ctx.dark);
        let (hidden, hovered) = {
            let state = self.state.borrow();
            (state.hidden.clone(), state.hovered)
        };
        let plan = bar_chart_plan(ctx.data, ctx.config, &hidden, ctx.dark, &self.seams);
        let hovered = hovered.filter(|i| *i < plan.row_count);
        build_el(ctx, &plan, hovered, &self.seams, k, &self.state)
    }
}

/// One hit cell: a `1/n` band over the plot. Hovering it sets the hovered row
/// (recharts' `activeTooltipIndex`); pressing it routes the row to
/// `onDataPointClick` (React's per-`<Bar>` `onClick(_, index)` arm); the
/// tooltip mounts inside it.
#[allow(clippy::too_many_arguments)]
fn hit_cell<S: 'static>(
    ctx: &ChartRenderContext<'_, S>,
    plan: &BarChartPlan,
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

/// The `LabelList` overlay: one band column per data row, whose children are
/// the series slots (the same 90%/m shares the bars split, centred the same
/// way), each holding its bar's value label at the top of the band — the
/// `position="top"` labels in the exact horizontal slot their bar occupies.
/// Hidden series and value-less rows keep an empty slot so the visible labels
/// stay aligned with their bars (the slots are fractions of the parent, but
/// the group is what centres).
fn values_overlay(plan: &BarChartPlan, k: &Kit) -> El {
    let mut overlay = row()
        .key(VALUES_KEY)
        .absolute(0.0, 0.0)
        .w_full()
        .h_full();
    if plan.row_count == 0 || plan.series.is_empty() {
        return overlay;
    }
    let band = 1.0 / plan.row_count as f32;
    // The label slot = the bar slot: the category content's share per series.
    let slot = (1.0 - BAR_CATEGORY_GAP) / plan.series.len() as f32;
    for r in 0..plan.row_count {
        let mut band_row = row()
            .key(format!("{VALUES_BAND_PREFIX}{r}"))
            .w_frac(band)
            .h_full()
            .justify_center(); // the slot group centres, as the bars' content does
        for series in &plan.series {
            let label = if series.hidden {
                None
            } else {
                series.labels.get(r).cloned().flatten()
            };
            let slot_el = match label {
                Some(t) => text(t)
                    .key(format!("{VALUE_PREFIX}{r}-{}", series.key))
                    .w_frac(slot)
                    .font(k.ty.small) // LabelList fontSize: 11
                    .color(k.c.text_muted) // fill: var(--c-fg-secondary)
                    .align(Align::Center) // centred over the bar's slot
                    .wrap(),
                None => el().w_frac(slot), // the empty slot keeps the geometry
            };
            band_row = band_row.child(slot_el);
        }
        overlay = overlay.child(band_row);
    }
    overlay
}

/// The tree: recharts' `<BarChart>` composition as the flexbox frame the
/// module docs describe.
#[allow(clippy::too_many_arguments)]
fn build_el<S: 'static>(
    ctx: &ChartRenderContext<'_, S>,
    plan: &BarChartPlan,
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
    plan: &BarChartPlan,
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
                // Bars carry `isAnimationActive={false}`, so the toggle lands
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
