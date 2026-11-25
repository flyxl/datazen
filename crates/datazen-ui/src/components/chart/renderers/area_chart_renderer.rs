//! Port of `packages/frontend/src/components/chart/renderers/AreaChartRenderer.tsx` (T166).
//!
//! The React component is a `recharts` `<AreaChart>` composition: the log-scale
//! hint decides whether `mapToLogScale` rewrites the Y values (stashing the
//! originals under `__orig__${key}` for the T136 tooltip to read back), a
//! `useState<Set<string>>` hides series from the legend, `buildOverlapDashMap`
//! dashes series that would overlap, and `<Area>` paints each series — plus
//! the `<CartesianGrid>` hairlines, the two axes with their tick formatters,
//! the `<Legend>` that toggles, and `<LogScaleTooltip>`/`<Tooltip>` content.
//! This is the first chart engine to land on the `ChartRenderer` seam
//! (`chart_canvas.rs`); the other four engines keep their placeholders.
//!
//! ## How the recharts composition maps onto Rune
//!
//! * **Real geometry, not an approximation.** `rune::Shape` fills polygons
//!   and strokes polylines, so each series paints its area polygon (points
//!   down to the baseline and back) plus its `strokeWidth: 2` line inside one
//!   `rune::canvas` element. Grid hairlines and the axis frame are stroke/line
//!   ops. Coordinates are written in the `view_box(1, 1)` fractions of the
//!   plot, so one shape stretches to whatever rect layout gives the plot.
//! * **The plot frame** is a flexbox rebuild of the recharts margin box: a
//!   60pt Y-axis column (`justify_between` over the five ticks — recharts'
//!   default YAxis width), the growing plot stack, a 30pt X-axis row (recharts'
//!   default XAxis height) of per-band tick slots, and the 22pt legend row when
//!   `multiSeries`. Overlapping canvases stack through `.absolute(0.0, 0.0)`
//!   plus `.w_full().h_full()` — the shimmer overlay pattern in Rune's own
//!   tests — so grid, axes, every series and the hit layer share one rect.
//!
//! ## The seams (the split discipline)
//!
//! | React import | owner | shape here |
//! |--------------|-------|------------|
//! | `getColorPalette` (`lib/chart/colors.ts`) | T352 | [`series_palette`] — a lean five-colour kit palette (no new `oklch`); every scheme name maps to it until T352 lands |
//! | `formatAxisTick` / `formatCompact` / `formatEpochMs` (`lib/chart/format.ts`) | T079 | [`FormatSeams`] — three injected `Rc<dyn Fn>`; the defaults are lean line-by-line slices |
//! | `computeLogScaleHint` / `mapToLogScale` (`lib/chart/transform.ts`) | T182 | [`compute_log_scale_hint`] / [`map_to_log_scale`] — the two functions this engine needs, ported arm by arm; the file's transform/aggregation pipeline is not |
//! | `buildOverlapDashMap` (`lib/chart/seriesOverlap.ts`) | T153 | [`build_overlap_dash_map`] + [`STROKE_DASH_PATTERNS`] — ported line by line |
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
//!   positions `(i + 0.5) / n`).
//! * The tooltip follows recharts' `activeTooltipIndex` semantics the same
//!   way: entering a cell (`on_hover`) sets the hovered row, the box mounts
//!   inside that cell (anchored like the floating content), and leaving the
//!   chart (`on_hover_change(false)` on the hit layer) clears it.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | root | `area-chart` |
//! | plot row / plot stack | `area-chart-plot-row` / `area-chart-plot` |
//! | grid canvas / axes canvas | `area-chart-grid` / `area-chart-axes` |
//! | series canvas | `area-chart-series-{key}` |
//! | active dots canvas | `area-chart-active-dots` |
//! | hit layer / hit cell *i* | `area-chart-hit-zone` / `area-chart-hit-{i}` |
//! | Y axis / Y tick *k* | `area-chart-yaxis` / `area-chart-yaxis-tick-{k}` |
//! | X axis / X tick *i* | `area-chart-xaxis` / `area-chart-xaxis-tick-{i}` |
//! | legend / item / swatch / label | `area-chart-legend` / `area-chart-legend-item-{key}` / `…-swatch-{key}` / `…-label-{key}` |
//! | the plain tooltip box | `area-chart-tooltip` |
//! | the log-scale tooltip box | `chart-log-scale-tooltip` (T136's `CONTENT_KEY`) |
//!
//! ## Deliberate deviations
//!
//! * recharts' tick picking ("nice" numbers, auto interval) is not ported: Y
//!   ticks are five quarter-domain values, X ticks are every `ceil(n/8)`-th
//!   band; time-axis ticks come from `config.timeTicks` snapped to the nearest
//!   band (Rune's flexbox has no percent-offset positioning for a label).
//! * `<Area type="monotone">`'s Fritsch–Carlson smoothing is not ported: the
//!   stroke and the area polygon run through the same straight segments. The
//!   monotone spline exists to keep curves from overshooting between points;
//!   the polyline cannot overshoot, and the y read is identical at every
//!   point — a recharts cosmetic, traded for the same data read.
//! * The Y domain is recharts' default `[0, auto]` behaviour, lean: `[0, max]`
//!   for non-negative data, `[min, 0]` for non-positive, `[min, max]` across
//!   zero; no nice-number padding. Hiding a series does not rescale (recharts
//!   does not either).
//! * `CartesianGrid`'s `strokeDasharray="3 3"` hairlines paint as solid
//!   hairlines — Rune's `Shape` has no dash on strokes — while the series'
//!   `strokeDasharray` overlap patterns ARE painted, by cutting the polyline
//!   into on-runs ([`dashed_segments`]).
//! * Y tick labels are box-aligned by `justify_between` (first/last at the
//!   plot's edges); recharts centres each label on its tick.
//! * The tooltip anchors at the hovered bucket's top-left (recharts floats it
//!   near the point, clamped — same content, a coarser anchor).
//! * X tick labels wrap instead of recharts' rotation; the `formatAxisTick`
//!   seam keeps React's 20-character truncation for strings.
//! * `activeDot`'s `r: 5` paints as a 16-gon (Rune has no circle primitive);
//!   the `cursor: 'pointer'` hint has no Rune counterpart.
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
    chart_payload, format_entry, LogScaleTooltip, LogScaleTooltipEntry, ORIG_PREFIX,
};
use crate::ui::kit::kit;
use rune_kit::Kit;

// ---- Keys -------------------------------------------------------------------

/// Root, recharts' `<AreaChart>` frame.
pub const ROOT_KEY: &str = "area-chart";
/// The Y-axis + plot stack row.
pub const PLOT_ROW_KEY: &str = "area-chart-plot-row";
/// The plot itself — every canvas and the hit layer share its rect.
pub const PLOT_KEY: &str = "area-chart-plot";
/// The `CartesianGrid` canvas.
pub const GRID_KEY: &str = "area-chart-grid";
/// The axis frame canvas (recharts' axis lines).
pub const AXES_KEY: &str = "area-chart-axes";
/// One canvas per series: `area-chart-series-{key}`.
pub const SERIES_KEY_PREFIX: &str = "area-chart-series-";
/// The hovered row's `activeDot`s, one per visible series.
pub const DOTS_KEY: &str = "area-chart-active-dots";
/// The pointer/press layer over the plot.
pub const HIT_ZONE_KEY: &str = "area-chart-hit-zone";
/// One hit cell per data row: `area-chart-hit-{i}`.
pub const HIT_CELL_PREFIX: &str = "area-chart-hit-";
/// The Y-axis label column.
pub const Y_AXIS_KEY: &str = "area-chart-yaxis";
/// The X-axis label row.
pub const X_AXIS_KEY: &str = "area-chart-xaxis";
/// The legend (multiSeries only).
pub const LEGEND_KEY: &str = "area-chart-legend";
/// One entry per series: `area-chart-legend-item-{key}` (+ `-swatch-`/`-label-`).
pub const LEGEND_ITEM_PREFIX: &str = "area-chart-legend-item-";
/// The swatch of legend entry `{key}`.
pub const LEGEND_SWATCH_PREFIX: &str = "area-chart-legend-swatch-";
/// The label of legend entry `{key}` (dims to 0.35 when hidden).
pub const LEGEND_LABEL_PREFIX: &str = "area-chart-legend-label-";
/// The plain (non-log) `<Tooltip>` content box.
pub const TOOLTIP_KEY: &str = "area-chart-tooltip";

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
/// React `fillOpacity`.
const FILL_OPACITY: f32 = 0.15;
/// React `strokeWidth`.
const STROKE_W: f32 = 2.0;
/// The axis frame's stroke.
const AXIS_LINE_W: f32 = 1.0;
/// The grid hairlines' stroke.
const GRID_LINE_W: f32 = 1.0;
/// React `CartesianGrid` `opacity` when `showGrid`.
const GRID_OPACITY: f32 = 0.3;
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

// ---- Lean `lib/chart` slices -------------------------------------------------
//
// Each slice below is a line-by-line port of exactly what this engine calls;
// the owning task (see the module docs) ports the whole file. The JS coercion
// helpers mirror `log_scale_tooltip.rs`'s documented semantics.

/// JS `Number(str)`: trimmed, `''` is 0, decimal grammar, else NaN (the
/// JS-only hex/inf spellings deviate — T136's docs spell out the same caveat).
fn js_number(s: &str) -> f64 {
    let trimmed = s.trim();
    if trimmed.is_empty() {
        0.0
    } else {
        trimmed.parse::<f64>().unwrap_or(f64::NAN)
    }
}

/// JS `String(n)` for the representable forms. Shared with the bar engine
/// (T192) — the plain-tooltip and `LabelList` value displays read the same
/// coercion, so the slice is `pub` and both engines import it.
pub fn js_string(n: f64) -> String {
    if n.is_nan() {
        "NaN".to_string()
    } else if n.is_infinite() {
        if n < 0.0 {
            "-Infinity".to_string()
        } else {
            "Infinity".to_string()
        }
    } else if n == 0.0 {
        "0".to_string()
    } else {
        n.to_string()
    }
}

/// `transform.ts`'s `toNumber`: numbers pass (NaN included — JS
/// `typeof NaN === 'number'`), strings parse (a finite parse is the number,
/// a NaN parse is null), null → null. Shared with the bar engine (T192):
/// `computeLogScaleHint`/`mapToLogScale` are one seam family, so the bar
/// engine reads series slots through the same coercion rather than forking it.
pub fn to_number(v: &ChartValue) -> Option<f64> {
    match v {
        ChartValue::Number(n) => Some(*n),
        ChartValue::Text(s) => {
            let n = js_number(s);
            (!n.is_nan()).then_some(n)
        }
        ChartValue::Null => None,
    }
}

/// `seriesOverlap.ts`'s `Number(pt[key])` + `Number.isFinite` gate: absent →
/// NaN (skipped), null → 0 (kept — JS `Number(null) === 0`).
fn finite_coercion(v: Option<&ChartValue>) -> Option<f64> {
    let n = match v {
        None => f64::NAN,
        Some(ChartValue::Null) => 0.0,
        Some(ChartValue::Number(n)) => *n,
        Some(ChartValue::Text(s)) => js_number(s),
    };
    n.is_finite().then_some(n)
}

/// Mirror of `LogScaleHint` (`{ use, domainMin }`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LogScaleHint {
    /// React `use`.
    pub use_log: bool,
    /// React `domainMin`.
    pub domain_min: f64,
}

/// `transform.ts`'s `computeLogScaleHint`, arm by arm (the module docs own the
/// full-file attribution to T182): two-plus series, no negatives, a positive
/// max, and a >10x max/min-positive spread.
pub fn compute_log_scale_hint(
    data: &[ChartDataPoint],
    series_keys: &[String],
) -> LogScaleHint {
    let off = LogScaleHint {
        use_log: false,
        domain_min: 1.0,
    };
    if series_keys.len() < 2 {
        return off;
    }
    let mut global_max = f64::NEG_INFINITY;
    let mut min_positive = f64::INFINITY;
    let mut has_negative = false;
    for key in series_keys {
        for point in data {
            let Some(v) = to_number(point.get(key).unwrap_or(&ChartValue::Null)) else {
                continue; // toNumber's null skip
            };
            if v < 0.0 {
                has_negative = true; // NaN fails this in JS and Rust alike
            }
            if v > global_max {
                global_max = v;
            }
            if v > 0.0 && v < min_positive {
                min_positive = v;
            }
        }
    }
    if has_negative || global_max <= 0.0 || min_positive.is_infinite() {
        return off;
    }
    if global_max / min_positive < 10.0 {
        return off;
    }
    let domain_min = 1.0_f64.max(10.0_f64.powf(min_positive.log10().floor()));
    LogScaleHint {
        use_log: true,
        domain_min,
    }
}

/// `transform.ts`'s `mapToLogScale`: clone each point, stash the original
/// under `__orig__${key}` (the slot T136's tooltip reads), and rewrite the
/// slot as `log10(max(v, domainMin))` — with JS `Math.max` NaN semantics
/// preserved.
pub fn map_to_log_scale(
    data: &[ChartDataPoint],
    series_keys: &[String],
    domain_min: f64,
) -> Vec<ChartDataPoint> {
    data.iter()
        .map(|point| {
            let mut clone = point.clone();
            for key in series_keys {
                let raw = point.get(key).unwrap_or(&ChartValue::Null);
                let Some(v) = to_number(raw) else {
                    continue; // v == null → untouched
                };
                clone.insert(format!("{ORIG_PREFIX}{key}"), ChartValue::Number(v));
                let clamped = if v.is_nan() {
                    f64::NAN // Math.max(NaN, d) === NaN; Rust's f64::max would keep d
                } else {
                    v.max(domain_min)
                };
                clone.insert(key.clone(), ChartValue::Number(clamped.log10()));
            }
            clone
        })
        .collect()
}

/// `colors.ts`'s `STROKE_DASH_PATTERNS` as on/off point lengths (`undefined`
/// is solid).
pub const STROKE_DASH_PATTERNS: [Option<&'static [f32]>; 6] = [
    None,                          // undefined — solid
    Some(&[8.0, 4.0]),             // '8 4' dashed
    Some(&[2.0, 4.0]),             // '2 4' dotted
    Some(&[8.0, 4.0, 2.0, 4.0]),   // '8 4 2 4' dash-dot
    Some(&[12.0, 4.0]),            // '12 4' long dash
    Some(&[4.0, 4.0]),             // '4 4' short dash
];

/// `seriesOverlap.ts`'s `areSeriesTooClose`: every point's difference under
/// `threshold` (points with a non-finite side are skipped, as JS skips them).
fn series_too_close(
    data: &[ChartDataPoint],
    key_a: &str,
    key_b: &str,
    threshold: f64,
) -> bool {
    let mut max_diff = 0.0;
    for point in data {
        let (Some(a), Some(b)) = (
            finite_coercion(point.get(key_a)),
            finite_coercion(point.get(key_b)),
        ) else {
            continue;
        };
        let diff = (a - b).abs();
        if diff > threshold {
            return false;
        }
        if diff > max_diff {
            max_diff = diff;
        }
    }
    true
}

/// `seriesOverlap.ts`'s `buildOverlapDashMap`, line by line (T153 owns the
/// file): `None` = solid; overlapping pairs get patterns cycled from solid.
pub fn build_overlap_dash_map(
    data: &[ChartDataPoint],
    y_axes: &[String],
) -> BTreeMap<String, Option<&'static [f32]>> {
    let mut result: BTreeMap<String, Option<&'static [f32]>> =
        y_axes.iter().map(|k| (k.clone(), None)).collect();
    if y_axes.len() < 2 || data.is_empty() {
        return result;
    }
    let mut global_min = f64::INFINITY;
    let mut global_max = f64::NEG_INFINITY;
    for point in data {
        for key in y_axes {
            let Some(v) = finite_coercion(point.get(key)) else {
                continue;
            };
            if v < global_min {
                global_min = v;
            }
            if v > global_max {
                global_max = v;
            }
        }
    }
    let range = global_max - global_min;
    if range == 0.0 {
        // All identical — cycle every series through the patterns.
        let mut idx = 0;
        for key in y_axes {
            if let Some(slot) = result.get_mut(key) {
                *slot = STROKE_DASH_PATTERNS[idx % STROKE_DASH_PATTERNS.len()];
            }
            idx += 1;
        }
        return result;
    }
    let threshold = range * 0.05; // OVERLAP_RATIO
    let mut overlapping: BTreeSet<&str> = BTreeSet::new();
    for i in 0..y_axes.len() {
        for j in i + 1..y_axes.len() {
            if series_too_close(data, &y_axes[i], &y_axes[j], threshold) {
                overlapping.insert(y_axes[i].as_str());
                overlapping.insert(y_axes[j].as_str());
            }
        }
    }
    if overlapping.is_empty() {
        return result;
    }
    // Only the overlapping series get patterns; the first stays solid.
    let mut idx = 0;
    for key in y_axes {
        if overlapping.contains(key.as_str()) {
            if let Some(slot) = result.get_mut(key) {
                *slot = STROKE_DASH_PATTERNS[idx % STROKE_DASH_PATTERNS.len()];
            }
            idx += 1;
        }
    }
    result
}

// ---- The format seams (lib/chart/format.ts, T079) -----------------------------

/// The three `lib/chart/format.ts` entry points this engine calls, injectable
/// (T079 ports the file; tests and callers wire their own). The defaults are
/// lean line-by-line slices that keep React's arm structure.
#[derive(Clone)]
pub struct FormatSeams {
    /// `formatCompact` — the Y tick formatter (and `logTickFormatter`'s inner
    /// call) plus the tooltip values.
    pub format_compact: Rc<dyn Fn(f64) -> String>,
    /// `formatAxisTick` — the categorical X tick formatter.
    pub format_axis_tick: Rc<dyn Fn(&ChartValue) -> String>,
    /// `formatEpochMs` — the time-domain X tick formatter.
    pub format_epoch_ms: Rc<dyn Fn(f64) -> String>,
}

impl Default for FormatSeams {
    fn default() -> Self {
        Self {
            format_compact: Rc::new(lean_format_compact),
            format_axis_tick: Rc::new(lean_format_axis_tick),
            format_epoch_ms: Rc::new(lean_format_epoch_ms),
        }
    }
}

/// Lean `formatCompact`: the B/M/K arms and the integer arm; `toFixed`'s
/// rounding follows Rust (round-half-to-even), not JS — T079 settles it.
fn lean_format_compact(v: f64) -> String {
    let abs = v.abs();
    if abs >= 1e9 {
        return format!("{:.1}B", v / 1e9);
    }
    if abs >= 1e6 {
        return format!("{:.1}M", v / 1e6);
    }
    if abs >= 1e4 {
        return format!("{:.1}K", v / 1e3);
    }
    if v.is_finite() && v.fract() == 0.0 {
        return js_string(v); // Number.isInteger
    }
    let digits = if abs >= 100.0 {
        0
    } else if abs >= 1.0 {
        1
    } else {
        2
    };
    format!("{:.*}", digits, v)
}

/// Lean `formatNumber`: integers via `String(n)` — no `toLocaleString`
/// grouping in the slice — and fractions rounded to ≤2 decimals, trailing
/// zeros trimmed.
fn lean_format_number(v: f64) -> String {
    if v.is_finite() && v.fract() == 0.0 {
        return js_string(v);
    }
    let mut s = format!("{:.2}", v);
    while s.ends_with('0') {
        s.pop();
    }
    if s.ends_with('.') {
        s.pop();
    }
    s
}

/// `format.ts`'s epoch-ms window (2000–2100 UTC).
const EPOCH_MS_MIN: f64 = 946_684_800_000.0;
const EPOCH_MS_MAX: f64 = 4_102_444_800_000.0;

fn is_epoch_ms(v: f64) -> bool {
    v >= EPOCH_MS_MIN && v <= EPOCH_MS_MAX
}

/// Lean `formatEpochMs`: the `HH:mm:ss` clock, read in UTC (no host locale
/// clock in the engine); a non-finite timestamp spells JS's NaN arms.
fn lean_format_epoch_ms(ts: f64) -> String {
    if !ts.is_finite() {
        return "NaN:NaN:NaN".to_string();
    }
    let secs = ts.div_euclid(1000.0).rem_euclid(86_400.0);
    let h = secs.div_euclid(3600.0);
    let m = secs.rem_euclid(3600.0).div_euclid(60.0);
    let s = secs.rem_euclid(60.0);
    format!("{h:02.0}:{m:02.0}:{s:02.0}")
}

/// Lean `formatAxisTick`: null → `''`, numbers through the epoch window as
/// clock time else `formatNumber`, strings truncated at 20 chars (Unicode
/// scalars, not JS UTF-16 units — the deviation class T136 counted).
fn lean_format_axis_tick(v: &ChartValue) -> String {
    match v {
        ChartValue::Null => String::new(),
        ChartValue::Number(n) => {
            if is_epoch_ms(*n) {
                lean_format_epoch_ms(*n)
            } else {
                lean_format_number(*n)
            }
        }
        ChartValue::Text(s) => {
            let count = s.chars().count();
            if count > 20 {
                let prefix: String = s.chars().take(18).collect();
                format!("{prefix}…")
            } else {
                s.clone()
            }
        }
    }
}

/// The lean `getColorPalette` (T352 owns `colors.ts` and its fixed-hex
/// palettes): one kit-token series palette for every scheme name — the same
/// "series colours are data, chrome is CSS variables" split React's file
/// documents, read from kit semantic colours so no new raw colour is spelled.
/// Shared with the bar engine (T192): both engines' `colors[i % colors.length]`
/// read the same five-colour kit palette, so the slice is `pub`.
pub fn series_palette(k: &Kit) -> [rune::Color; 5] {
    [
        k.c.info.fg,
        k.c.success.fg,
        k.c.warning.fg,
        k.c.danger.fg,
        k.c.text_accent,
    ]
}

// ---- The geometry plan --------------------------------------------------------

/// One Y axis tick: the axis-space value and its formatted label.
#[derive(Clone, Debug, PartialEq)]
pub struct YTick {
    pub value: f64,
    pub label: String,
}

/// One X axis tick: the data row whose band shows it, and its label.
#[derive(Clone, Debug, PartialEq)]
pub struct XTick {
    pub row: usize,
    pub label: String,
}

/// One series' paint plan.
#[derive(Clone, Debug)]
pub struct SeriesPlan {
    /// React `dataKey`.
    pub key: String,
    /// `colors[i % colors.length]`.
    pub color: rune::Color,
    /// `dashMap[yKey]` (the on/off lengths, `None` = solid).
    pub dash: Option<&'static [f32]>,
    /// `hiddenSeries.has(yKey)`.
    pub hidden: bool,
    /// Plot fractions per data row (`None` = the row has no drawable point —
    /// recharts' gap on null/NaN).
    pub points: Vec<Option<[f32; 2]>>,
}

/// Everything one frame of the engine paints, computed before any element:
/// the memoised pipeline of the React component (logHint → chartData →
/// dashMap → geometry), observable without a runtime.
#[derive(Clone, Debug)]
pub struct AreaChartPlan {
    /// `chartData`: `mapToLogScale`'s output when `log_hint.use_log`, else the
    /// input.
    pub chart_data: Vec<ChartDataPoint>,
    pub log_hint: LogScaleHint,
    /// `config.xAxis ?? '__index'`.
    pub x_key: String,
    /// `config.yAxes.length > 1` (the legend gate).
    pub multi_series: bool,
    pub series: Vec<SeriesPlan>,
    pub y_domain: [f64; 2],
    /// Top first (the first is the domain max).
    pub y_ticks: Vec<YTick>,
    pub x_ticks: Vec<XTick>,
    pub row_count: usize,
    /// Vertical grid line fractions (the X tick positions).
    pub grid_xs: Vec<f32>,
    /// Horizontal grid line fractions, top first (the Y tick positions).
    pub grid_ys: Vec<f32>,
}

/// The recharts default domain, lean: `[0, max]` for non-negative data,
/// `[min, 0]` for non-positive, `[min, max]` across zero; flat data widens by
/// one; no data is an honest `[0, 1]` frame. Shared with the bar engine
/// (T192): both charts ride recharts' default Y domain, so the slice is
/// `pub` and the bar engine imports it instead of forking it.
pub fn lean_y_domain(values: &[f64]) -> [f64; 2] {
    if values.is_empty() {
        return [0.0, 1.0];
    }
    let min = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    if max == min {
        return [min, min + 1.0];
    }
    if min >= 0.0 {
        [0.0, max]
    } else if max <= 0.0 {
        [min, 0.0]
    } else {
        [min, max]
    }
}

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

/// The engine's whole plan for one frame: the React component's memo chain
/// (`logHint` → `chartData` → `dashMap` → `<Area>` geometry), as pure data.
pub fn area_chart_plan(
    data: &[ChartDataPoint],
    config: &ChartConfig,
    hidden: &BTreeSet<String>,
    dark: bool,
    seams: &FormatSeams,
) -> AreaChartPlan {
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
    let dash_map = if multi_series {
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
    let series: Vec<SeriesPlan> = y_axes
        .iter()
        .enumerate()
        .map(|(i, key)| {
            SeriesPlan {
                key: key.clone(),
                color: palette[i % palette.len()],
                dash: dash_map.get(key).and_then(|d| *d),
                hidden: hidden.contains(key),
                points: chart_data
                    .iter()
                    .enumerate()
                    .map(|(row, point)| {
                        let x = x_fraction(row, point, &x_key, config, row_count);
                        let y = y_fraction(point, key, y_domain);
                        match (x, y) {
                            (Some(x), Some(y)) => Some([x, y]),
                            _ => None,
                        }
                    })
                    .collect(),
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

    AreaChartPlan {
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

fn dist(a: [f32; 2], b: [f32; 2]) -> f32 {
    ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt()
}

/// Cut a polyline into the dash pattern's on-runs: the port of SVG
/// `strokeDasharray` (Rune strokes have no dash of their own), so the overlap
/// patterns actually paint.
pub fn dashed_segments(points: &[[f32; 2]], pattern: &[f32]) -> Vec<Vec<[f32; 2]>> {
    let pattern: Vec<f32> = pattern.iter().copied().filter(|l| *l > 0.0).collect();
    let mut out = Vec::new();
    if pattern.is_empty() || points.len() < 2 {
        return out;
    }
    let mut run: Vec<[f32; 2]> = Vec::new();
    let mut on = true;
    let mut leg = 0usize;
    let mut step = pattern[0];
    for pair in points.windows(2) {
        let (mut a, b) = (pair[0], pair[1]);
        let mut left = dist(a, b);
        while left > 1e-6 {
            if step >= left {
                if on {
                    if run.last() != Some(&a) {
                        run.push(a);
                    }
                    run.push(b);
                }
                step -= left;
                left = 0.0;
            } else {
                let t = step / left;
                let cut = [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t];
                if on {
                    if run.last() != Some(&a) {
                        run.push(a);
                    }
                    run.push(cut);
                    if run.len() >= 2 {
                        out.push(std::mem::take(&mut run));
                    }
                }
                a = cut;
                left -= step;
                on = !on;
                leg = (leg + 1) % pattern.len();
                step = pattern[leg];
            }
        }
    }
    if on && run.len() >= 2 {
        out.push(run);
    }
    out
}

/// The runs of drawable points (recharts breaks the line/area at nulls —
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

/// A regular n-gon (the `activeDot` disc — Rune has no circle primitive).
fn gon(center: [f32; 2], radius: f32, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect()
}

/// The `CartesianGrid`: the hairlines at the tick positions (React mounts it
/// always; `showGrid` false is `opacity: 0`, so the canvas stays and paints
/// nothing — the honest invisible grid).
fn grid_shape(plan: &AreaChartPlan, k: &Kit, show_grid: bool) -> Shape {
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

/// One series: the area polygon (points → baseline → back) at
/// `fillOpacity 0.15` plus the `strokeWidth 2` line, dashed through
/// [`dashed_segments`] when the overlap map says so.
fn series_shape(series: &SeriesPlan, k: &Kit) -> Shape {
    let mut shape = Shape::new().view_box(1.0, 1.0);
    for run in present_runs(&series.points) {
        let mut polygon = run.clone();
        let last_x = polygon[polygon.len() - 1][0];
        let first_x = polygon[0][0];
        polygon.push([last_x, 1.0]);
        polygon.push([first_x, 1.0]);
        shape = shape.fill(polygon, k.alpha(series.color, FILL_OPACITY));
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

/// The `activeDot`s at the hovered row: one `r: 5` disc per visible series.
fn dots_shape(plan: &AreaChartPlan, index: usize) -> Shape {
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
    plan: &AreaChartPlan,
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
    plan: &AreaChartPlan,
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

/// The area chart engine. Cloning shares [`EngineState`] (and the seams), so a
/// factory that clones one engine per build keeps the user's toggles and
/// hover across frames — the component's retained state.
#[derive(Clone)]
pub struct AreaChartRenderer {
    state: Rc<RefCell<EngineState>>,
    seams: FormatSeams,
}

impl AreaChartRenderer {
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(EngineState::default())),
            seams: FormatSeams::default(),
        }
    }

    /// Wire the T079 `formatCompact` port (or a test probe).
    pub fn format_compact(mut self, f: Rc<dyn Fn(f64) -> String>) -> Self {
        self.seams.format_compact = f;
        self
    }

    /// Wire the T079 `formatAxisTick` port (or a test probe).
    pub fn format_axis_tick(mut self, f: Rc<dyn Fn(&ChartValue) -> String>) -> Self {
        self.seams.format_axis_tick = f;
        self
    }

    /// Wire the T079 `formatEpochMs` port (or a test probe).
    pub fn format_epoch_ms(mut self, f: Rc<dyn Fn(f64) -> String>) -> Self {
        self.seams.format_epoch_ms = f;
        self
    }

    /// React `hiddenSeries`, as the engine keeps it.
    pub fn hidden_series(&self) -> BTreeSet<String> {
        self.state.borrow().hidden.clone()
    }

    /// recharts' `activeTooltipIndex`, as the engine keeps it.
    pub fn hovered_row(&self) -> Option<usize> {
        self.state.borrow().hovered
    }

    /// React `toggleSeries`: insert-or-remove in the hidden set.
    pub fn toggle_series(&self, data_key: &str) {
        let mut state = self.state.borrow_mut();
        if !state.hidden.remove(data_key) {
            state.hidden.insert(data_key.to_string());
        }
    }
}

impl Default for AreaChartRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// The canvas's renderer-factory injection point: `ChartType::Area` gets this
/// engine (cloned per build, sharing its state), every other type keeps the
/// honest placeholder. This is the wiring the chart-view track passes to
/// `ChartCanvas::renderer_factory`.
pub fn renderer_factory<S: 'static>(
    engine: AreaChartRenderer,
) -> impl Fn(ChartType) -> Box<dyn ChartRenderer<S> + 'static> {
    move |chart_type| match chart_type {
        ChartType::Area => Box::new(engine.clone()),
        other => Box::new(PlaceholderChartRenderer::new(other)),
    }
}

impl<S: 'static> ChartRenderer<S> for AreaChartRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        let k = kit(ctx.dark);
        let (hidden, hovered) = {
            let state = self.state.borrow();
            (state.hidden.clone(), state.hovered)
        };
        let plan = area_chart_plan(ctx.data, ctx.config, &hidden, ctx.dark, &self.seams);
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
    plan: &AreaChartPlan,
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

/// The tree: recharts' `<AreaChart>` composition as the flexbox frame the
/// module docs describe.
#[allow(clippy::too_many_arguments)]
fn build_el<S: 'static>(
    ctx: &ChartRenderContext<'_, S>,
    plan: &AreaChartPlan,
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
            canvas(series_shape(series, k))
                .key(format!("{SERIES_KEY_PREFIX}{}", series.key))
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        );
    }
    // activeDot is disabled under a time domain (React `activeDot={false}`).
    if let (Some(row), None) = (hovered, ctx.config.time_domain) {
        plot = plot.child(
            canvas(dots_shape(plan, row))
                .key(DOTS_KEY)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        );
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
    root = root
        .child(row().child(el().w(Y_AXIS_W)).child(x_axis));

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
    plan: &AreaChartPlan,
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
                // Areas carry `isAnimationActive={false}`, so the toggle lands
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
