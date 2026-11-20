//! Port of `packages/frontend/src/components/chart/renderers/PieChartRenderer.tsx` (T238; the
//! ledger's older numbering says T240).
//!
//! The React component is a `recharts` `<PieChart>` composition — the fourth
//! engine on the `ChartRenderer` seam (`chart_canvas.rs`), after the area
//! (T166), bar (T192) and line (T216) engines. It is the family's simplest:
//! one series (`config.yAxes[0]`), one wedge per row (`<Cell>`), no grid, no
//! axes, no time domain, no log hint, no dash map, no hidden-series toggle —
//! a centered pie (`cx/cy 50%`, `outerRadius 70%`) whose per-row `<Cell>`
//! carries the palette colour and the row's `onClick`, the `showValues`
//! label (`{name}: {pct}%` with its label line), the plain `<Tooltip>`
//! contentStyle box, and a legend gated on `config.showLegend` (not
//! multiSeries — the pie has one series by construction).
//!
//! ## How the recharts composition maps onto Rune
//!
//! * **Real geometry, not an approximation.** `rune::Shape` fills polygons,
//!   so each slice paints its wedge — the centre vertex plus the arc,
//!   subdivided to a 64-gon resolution — in the `view_box(1, 1)` fractions
//!   of the plot (`cx=cy=0.5`, `outerRadius=0.7` verbatim). The wedge angles
//!   start at 12 o'clock and sweep clockwise (recharts' default `startAngle
//!   90` / sweep), in the same y-down screen angles the SVG arcs use.
//! * **The hit raster.** recharts' `<Cell>` hit areas are the SVG wedges
//!   themselves. Rune's flexbox hit areas are rectangles, so the port rasters
//!   the plot into a 16×16 grid of cells and assigns each cell to the slice
//!   whose wedge contains the cell's centre (the angle test + the 0.7 radius
//!   test); pressing a cell routes `onDataPointClick(row)`, hovering one
//!   mounts the tooltip. The cells tile the plot with no overlaps — the
//!   geometry is the cell centre's wedge, a raster of the exact hit.
//!
//! ## The seams (the split discipline)
//!
//! | React import | owner | shape here |
//! |--------------|-------|------------|
//! | `getColorPalette` (`lib/chart/colors.ts`) | T352 | [`series_palette`] — the five-colour kit palette the area engine sliced; every scheme name maps to it until T352 lands |
//! | `toNumber` (`lib/chart/transform.ts`) | T182 | the area engine's slice, imported — the value coercion the wedge angles read |
//! | the plain tooltip's value display | — | the area engine's `js_string` slice, imported |
//!
//! (No format seam: the pie has no axis ticks and no compact value reads —
//! the `lib/chart/format.ts` family the other engines inject never enters
//! this composition.)
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | root | `pie-chart` |
//! | the pie canvas | `pie-chart-pie` |
//! | the label row / label *i* | `pie-chart-labels` / `pie-chart-label-{row}` |
//! | the hit raster / cell *r*-*c* | `pie-chart-hit-zone` / `pie-chart-hit-{r}-{c}` |
//! | the legend / entry / swatch / name | `pie-chart-legend` / `pie-chart-legend-item-{row}` / `…-swatch-{row}` / `…-name-{row}` |
//! | the tooltip box | `pie-chart-tooltip` |
//!
//! ## Deliberate deviations
//!
//! * recharts' pie labels (`label={...}` + `labelLine`) anchor outside the
//!   wedge's mid-angle — a polar text anchor Rune's flexbox has no
//!   counterpart for (T166's tooltip-anchor deviation class). The port rides
//!   the labels in a wrapped row under the plot, one per slice, the same
//!   `{name}: {pct}%` text; the label line is the same class of cosmetic
//!   (a leader line to a list entry has no flexbox counterpart either).
//! * `((percent ?? 0) * 100).toFixed(0)` — `toFixed`'s rounding follows Rust
//!   (round-half-to-even), not JS — T079's documented stance, kept.
//! * The wedge arcs subdivide to a 64-gon resolution (a full circle); a
//!   hairline slice's arc is one or two segments — the same hairline
//!   reality recharts' sub-degree wedges have.
//! * The hit raster is 16×16: a cell belongs to the wedge containing its
//!   centre, so a hairline slice owns proportionally few cells (a press
//!   between centres rounds to the neighbour) — the raster of the exact
//!   hit, the honest flexbox counterpart of an SVG wedge hit test.
//! * The legend is not pressable (React's pie legend carries no
//!   `onClick`/`cursor: pointer` — the line/area/bar family's toggle does
//!   not exist here); the entries are plain text rows.
//! * The tooltip anchors at the hovered cell's top-left (recharts floats it
//!   near the pointer, clamped — same content, a coarser anchor).
//! * The tooltip's value is the raw slot's JS `String(n)` display, the
//!   family's plain-box arms; `toLocaleString` grouping is T079's.
//! * A null/NaN slot draws no wedge (recharts' skip), but the row still
//!   consumes its palette slot — React's `data.map((_, idx) => <Cell
//!   fill={colors[idx % len]}` colours by ROW index, and the port keeps the
//!   row-indexed palette read.

#![allow(dead_code)] // the chart-view track wires the factory; until then only tests exercise it

use std::cell::RefCell;
use std::rc::Rc;

use rune::{canvas, col, el, row, text, Align, El, Shape, Tx};

use super::super::chart_canvas::{
    ChartConfig, ChartDataPoint, ChartRenderContext, ChartRenderer, ChartType, ChartValue,
    PlaceholderChartRenderer,
};
use super::area_chart_renderer::{js_string, series_palette, to_number};
use crate::ui::kit::kit;
use rune_kit::Kit;

// ---- Keys -------------------------------------------------------------------

/// Root, recharts' `<PieChart>` frame.
pub(crate) const ROOT_KEY: &str = "pie-chart";
/// The pie canvas — every wedge paints in it.
pub(crate) const PIE_KEY: &str = "pie-chart-pie";
/// The `showValues` label row under the plot.
pub(crate) const LABELS_KEY: &str = "pie-chart-labels";
/// One label per slice: `pie-chart-label-{row}`.
pub(crate) const LABEL_PREFIX: &str = "pie-chart-label-";
/// The pointer raster over the plot.
pub(crate) const HIT_ZONE_KEY: &str = "pie-chart-hit-zone";
/// One raster cell: `pie-chart-hit-{r}-{c}`.
pub(crate) const HIT_CELL_PREFIX: &str = "pie-chart-hit-";
/// The legend row (`config.showLegend` gate).
pub(crate) const LEGEND_KEY: &str = "pie-chart-legend";
/// One entry per slice: `pie-chart-legend-item-{row}` (+ `-swatch-`/`-name-`).
pub(crate) const LEGEND_ITEM_PREFIX: &str = "pie-chart-legend-item-";
/// The swatch of legend entry `{row}`.
pub(crate) const LEGEND_SWATCH_PREFIX: &str = "pie-chart-legend-swatch-";
/// The name text of legend entry `{row}`.
pub(crate) const LEGEND_NAME_PREFIX: &str = "pie-chart-legend-name-";
/// The `<Tooltip>` content box.
pub(crate) const TOOLTIP_KEY: &str = "pie-chart-tooltip";

// recharts defaults / React literals, verbatim.
/// React `cx="50%"` / `cy="50%"`.
const CENTER: f32 = 0.5;
/// React `outerRadius="70%"` — 70% of the container's half-extent, in the
/// view-box fractions the square pie area's unit box maps to (0.5 is the
/// inscribed circle's radius, so 70% of it is 0.35).
const OUTER_RADIUS: f32 = 0.35;
/// React `wrapperStyle fontSize: 12` — one legend row.
const LEGEND_H: f32 = 22.0;
/// The legend row's gap between entries.
const LEGEND_GAP: f32 = 8.0;
/// One entry's inner gap (swatch → name).
const LEGEND_ENTRY_GAP: f32 = 4.0;
/// The legend swatch chip.
const SWATCH: f32 = 8.0;
/// The label row's height (fontSize 11 like the family's LabelList).
const LABEL_H: f32 = 18.0;
/// The label row's gap between labels.
const LABEL_GAP: f32 = 8.0;
/// The tooltip's anchor inside the hovered cell.
const TOOLTIP_DX: f32 = 2.0;
/// The tooltip's anchor below the cell's top.
const TOOLTIP_DY: f32 = 8.0;
/// The hit raster's side count (16×16 cells tile the plot).
pub(crate) const HIT_GRID: usize = 16;
/// The wedge arcs subdivide to this resolution over a full circle.
const ARC_RESOLUTION: usize = 64;

// ---- The geometry plan --------------------------------------------------------

/// One slice's paint plan.
#[derive(Clone, Debug)]
pub(crate) struct PieSlicePlan {
    /// The data row the slice renders (the Cell's `idx`).
    pub(crate) row: usize,
    /// React `nameKey` (`config.xAxis ?? '__index'`) — the slice's name, the
    /// raw slot display.
    pub(crate) name: String,
    /// The slice's value (null/NaN rows draw no wedge — recharts' skip).
    pub(crate) value: Option<f64>,
    /// `colors[idx % colors.length]` — the ROW index's palette slot (a null
    /// row still consumes its colour, as React's map does).
    pub(crate) color: rune::Color,
    /// The wedge's start angle, radians clockwise from 12 o'clock.
    pub(crate) start: f64,
    /// The wedge's sweep (radians) — `value / total * 2π`.
    pub(crate) weight: f64,
    /// The `showValues` label text (`None` when the toggle is off or the row
    /// has no wedge).
    pub(crate) label: Option<String>,
}

/// Everything one frame of the engine paints, computed before any element:
/// the React component's reads (the palette, `yKey`/`nameKey`, the wedge
/// angles and the label texts), observable without a runtime.
#[derive(Clone, Debug)]
pub(crate) struct PieChartPlan {
    /// `config.yAxes[0]` — the one series the pie plots.
    pub(crate) y_key: Option<String>,
    /// `config.xAxis ?? '__index'`.
    pub(crate) name_key: String,
    /// The finite values' sum (the angles' denominator).
    pub(crate) total: f64,
    pub(crate) slices: Vec<PieSlicePlan>,
    pub(crate) row_count: usize,
}

/// JS `toFixed(0)`: the integer display (round-half-even per Rust, the
/// documented T079 stance).
fn round0(v: f64) -> String {
    format!("{:.0}", v)
}

/// The engine's whole plan for one frame: the React component's reads as
/// pure data. The lean slices the chain reads are the area engine's,
/// imported — see the seams table in the module docs.
pub(crate) fn pie_chart_plan(
    data: &[ChartDataPoint],
    config: &ChartConfig,
    dark: bool,
) -> PieChartPlan {
    let y_key = config.y_axes.first().cloned();
    let name_key = config.x_axis.clone().unwrap_or_else(|| "__index".into());

    // The wedges: the finite values' angles, clockwise from 12 o'clock, in
    // row order (a null/NaN row consumes its palette slot but no angle).
    let mut total = 0.0;
    let mut values: Vec<Option<f64>> = Vec::with_capacity(data.len());
    for point in data {
        let v = y_key
            .as_deref()
            .and_then(|key| to_number(point.get(key).unwrap_or(&ChartValue::Null)))
            .filter(|v| v.is_finite());
        if let Some(v) = v {
            total += v;
        }
        values.push(v);
    }

    let k = kit(dark);
    let palette = series_palette(k);
    let mut start = 0.0;
    let mut slices = Vec::with_capacity(data.len());
    for (row, point) in data.iter().enumerate() {
        let v = values[row];
        let weight = match (v, total.partial_cmp(&0.0)) {
            (Some(v), Some(order)) if order.is_gt() && v > 0.0 => v / total * std::f64::consts::TAU,
            _ => 0.0, // no data, a zero total, or a zero value: no angle
        };
        let name = match point.get(&name_key) {
            Some(ChartValue::Text(s)) => s.clone(),
            Some(ChartValue::Number(n)) => js_string(*n),
            _ => String::new(), // null or absent
        };
        // React 26-31: `${props.name ?? ''}: ${((props.percent ?? 0) * 100)
        // .toFixed(0)}%` — percent is the slice's value share.
        let label = match (config.show_values, v, total.partial_cmp(&0.0)) {
            (true, Some(v), Some(order)) if order.is_gt() && v > 0.0 => Some(format!(
                "{}: {}%",
                name,
                round0(v / total * 100.0)
            )),
            (true, _, _) => Some(format!("{}: 0%", name)), // a zero-weight slice still labels
            (false, _, _) => None,
        };
        slices.push(PieSlicePlan {
            row,
            name,
            value: v,
            color: palette[row % palette.len()],
            start,
            weight,
            label,
        });
        start += weight;
    }

    PieChartPlan {
        y_key,
        name_key,
        total,
        slices,
        row_count: data.len(),
    }
}

// ---- The shapes ---------------------------------------------------------------

/// One wedge's polygon: the centre vertex plus the arc (subdivided to the
/// 64-gon resolution), in the plot's view-box fractions.
pub(crate) fn wedge_polygon(start: f64, weight: f64) -> Vec<[f32; 2]> {
    let segments = (weight / std::f64::consts::TAU * ARC_RESOLUTION as f64).ceil() as usize;
    let segments = segments.max(2);
    let mut polygon = vec![[CENTER, CENTER]];
    for i in 0..=segments {
        let angle = start + weight * i as f64 / segments as f64;
        // Clockwise from 12 o'clock in the y-down screen angles recharts'
        // startAngle-90/sweep arcs use.
        let theta = angle - std::f64::consts::FRAC_PI_2;
        polygon.push([
            CENTER + OUTER_RADIUS * theta.cos() as f32,
            CENTER + OUTER_RADIUS * theta.sin() as f32,
        ]);
    }
    polygon
}

/// The pie: one filled wedge per slice with an angle, in row order.
fn pie_shape(plan: &PieChartPlan) -> Shape {
    let mut shape = Shape::new().view_box(1.0, 1.0);
    for slice in &plan.slices {
        if slice.weight > 0.0 {
            shape = shape.fill(wedge_polygon(slice.start, slice.weight), slice.color);
        }
    }
    shape
}

// ---- The tooltip --------------------------------------------------------------

/// The hovered slice's entry: recharts' default pie tooltip content — the
/// name, the `' : '` separator, the raw value's JS display.
fn tooltip_line(slice: &PieSlicePlan) -> String {
    let value = match slice.value {
        Some(v) => js_string(v),
        None => String::new(), // recharts labels a null value with its empty display
    };
    if slice.name.is_empty() {
        value
    } else {
        format!("{} : {}", slice.name, value)
    }
}

/// The plain `<Tooltip>`'s content box: React's `contentStyle` chrome on the
/// token mapping the family documented (surface-alt → `layer_01`, edge →
/// `border_base`, radius 6 → `radius.md`, fg → `text_strong`, 12px →
/// `text_xsmall`).
fn plain_tooltip_box(slice: &PieSlicePlan, k: &Kit) -> El {
    col()
        .key(TOOLTIP_KEY)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(k.radius.md)
        .child(
            text(tooltip_line(slice))
                .key(format!("{TOOLTIP_KEY}-item-0"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_strong),
        )
}

/// The tooltip element, anchored at the hovered cell's top-left.
fn tooltip_el(slice: &PieSlicePlan, k: &Kit) -> El {
    plain_tooltip_box(slice, k).absolute(TOOLTIP_DX, TOOLTIP_DY)
}

// ---- The hit raster -------------------------------------------------------------

/// The slice whose wedge contains a plot point (`None` outside the pie or
/// between wedges): the angle test from 12 o'clock clockwise plus the radius
/// test, over the slices' cumulative spans.
pub(crate) fn slice_at(plan: &PieChartPlan, x: f32, y: f32) -> Option<usize> {
    let dx = x as f64 - CENTER as f64;
    let dy = y as f64 - CENTER as f64;
    if (dx * dx + dy * dy).sqrt() > OUTER_RADIUS as f64 {
        return None; // outside the pie — no Cell
    }
    // The angle clockwise from 12 o'clock, in the y-down screen angles.
    let angle = (dy.atan2(dx) + std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU)
        % std::f64::consts::TAU;
    let mut cursor = 0.0;
    for slice in &plan.slices {
        cursor += slice.weight;
        if angle < cursor {
            return Some(slice.row);
        }
    }
    None // the angle ran past the last wedge (a rounding edge at 12 o'clock)
}

/// The hit raster: a 16×16 grid of cells over the plot, each routing to the
/// slice whose wedge contains its centre. Hovering a cell sets the hovered
/// slice (recharts' active slice); pressing it routes the row to
/// `onDataPointClick` (the Cell's `onClick`); the tooltip mounts inside it.
#[allow(clippy::too_many_arguments)]
fn hit_cell<S: 'static>(
    ctx: &ChartRenderContext<'_, S>,
    plan: &PieChartPlan,
    r: usize,
    c: usize,
    hovered_cell: Option<(usize, usize)>,
    k: &Kit,
    state: &Rc<RefCell<EngineState>>,
) -> El {
    // The cell's centre in plot fractions: which wedge owns it.
    let centre = [
        (c as f32 + 0.5) / HIT_GRID as f32,
        (r as f32 + 0.5) / HIT_GRID as f32,
    ];
    let owner = slice_at(plan, centre[0], centre[1]);
    let mut cell = el()
        .key(format!("{HIT_CELL_PREFIX}{r}-{c}"))
        .a11y_hidden();
    let hover_state = Rc::clone(state);
    cell = cell.on_hover(move |_s: &mut S| {
        // Hovering a cell outside the pie clears (recharts' tooltip
        // deactivates between wedges); hovering one inside sets the slice —
        // the same `activeIndex` semantics — and remembers the cell, the
        // one mount site the tooltip takes.
        let mut st = hover_state.borrow_mut();
        st.hovered = owner;
        st.hovered_cell = owner.map(|_| (r, c));
    });
    if let (Some(on_click), Some(row)) = (ctx.on_data_point_click.as_ref(), owner) {
        let on_click = Rc::clone(on_click);
        cell = cell.on_press(move |s: &mut S| on_click(s, row));
    }
    // The tooltip mounts in the hovered CELL only (a wedge owns many cells;
    // React's tooltip is one floating box at the pointer).
    if let (Some(slice), Some((hr, hc))) = (
        owner.and_then(|row| plan.slices.iter().find(|slice| slice.row == row)),
        hovered_cell,
    ) {
        if (r, c) == (hr, hc) {
            cell = cell.child(tooltip_el(slice, k));
        }
    }
    cell
}

// ---- The engine -----------------------------------------------------------------

/// The engine's retained state — the hovered slice (recharts' active
/// `activeIndex`) and the raster cell the pointer is in (the tooltip mounts
/// in THAT cell — a wedge owns many cells, and React's tooltip is one
/// floating box, not one per owned cell). There is no hidden-series set —
/// the pie legend does not toggle.
#[derive(Default)]
struct EngineState {
    hovered: Option<usize>,
    /// The hovered raster cell `(row, column)` — the tooltip's mount site.
    hovered_cell: Option<(usize, usize)>,
}

/// The pie chart engine. Cloning shares [`EngineState`], so a factory that
/// clones one engine per build keeps the user's hover across frames — the
/// retained state React kept.
#[derive(Clone)]
pub(crate) struct PieChartRenderer {
    state: Rc<RefCell<EngineState>>,
}

impl PieChartRenderer {
    pub(crate) fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(EngineState::default())),
        }
    }

    /// recharts' active slice index, as the engine keeps it.
    pub(crate) fn hovered_slice(&self) -> Option<usize> {
        self.state.borrow().hovered
    }

    /// The hovered raster cell, as the engine keeps it (tests only).
    pub(crate) fn hovered_cell(&self) -> Option<(usize, usize)> {
        self.state.borrow().hovered_cell
    }
}

impl Default for PieChartRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// The canvas's renderer-factory injection point: `ChartType::Pie` gets this
/// engine (cloned per build, sharing its state), every other type keeps the
/// honest placeholder.
pub(crate) fn renderer_factory<S: 'static>(
    engine: PieChartRenderer,
) -> impl Fn(ChartType) -> Box<dyn ChartRenderer<S> + 'static> {
    move |chart_type| match chart_type {
        ChartType::Pie => Box::new(engine.clone()),
        other => Box::new(PlaceholderChartRenderer::new(other)),
    }
}

impl<S: 'static> ChartRenderer<S> for PieChartRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        let k = kit(ctx.dark);
        let (hovered, hovered_cell) = {
            let state = self.state.borrow();
            (state.hovered, state.hovered_cell)
        };
        let plan = pie_chart_plan(ctx.data, ctx.config, ctx.dark);
        // The hovered row is filtered to the rows that still exist — the
        // hovered cell rides with it (recharts clears its activeIndex when
        // the row vanishes; the tooltip unmounts instead of re-targeting).
        let hovered = hovered.filter(|row| plan.slices.iter().any(|s| s.row == *row));
        let hovered_cell = hovered_cell.filter(|_| hovered.is_some());
        build_el(ctx, &plan, hovered_cell, k, &self.state)
    }
}

/// The label row: the `showValues` labels, one per labelled slice, wrapped
/// (the polar anchors have no flexbox counterpart — the module docs'
/// deviation).
fn labels_row(plan: &PieChartPlan, k: &Kit) -> El {
    let mut labels = row()
        .key(LABELS_KEY)
        .h(LABEL_H)
        .w_full()
        .flex_wrap()
        .center()
        .gap(LABEL_GAP);
    for slice in plan.slices.iter().filter(|s| s.label.is_some()) {
        let label = slice.label.clone().unwrap_or_default();
        labels = labels.child(
            text(label)
                .key(format!("{LABEL_PREFIX}{}", slice.row))
                .font(k.ty.small) // the family's LabelList fontSize: 11
                .color(k.c.text_muted) // fill: var(--c-fg-secondary)
                .wrap(),
        );
    }
    labels
}

/// The legend row: `config.showLegend` gate, one plain entry per slice
/// (recharts' default legend content — the swatch and the name; not
/// pressable, React's pie legend carries no onClick).
fn legend_row(plan: &PieChartPlan, k: &Kit) -> El {
    let mut bar = row()
        .key(LEGEND_KEY)
        .h(LEGEND_H)
        .w_full()
        .flex_wrap()
        .center()
        .gap(LEGEND_GAP);
    for slice in &plan.slices {
        let item = row()
            .key(format!("{LEGEND_ITEM_PREFIX}{}", slice.row))
            .center()
            .gap(LEGEND_ENTRY_GAP)
            .child(
                el().key(format!("{LEGEND_SWATCH_PREFIX}{}", slice.row))
                    .w(SWATCH)
                    .h(SWATCH)
                    .radius(k.radius.sm)
                    .fill(slice.color),
            )
            .child(
                text(slice.name.clone())
                    .key(format!("{LEGEND_NAME_PREFIX}{}", slice.row))
                    .font(k.ty.text_xsmall) // wrapperStyle fontSize: 12
                    .color(k.c.text_strong) // color: var(--c-fg, #eee)
                    .wrap(),
            );
        bar = bar.child(item);
    }
    bar
}

/// The tree: recharts' `<PieChart>` composition — the centered pie canvas
/// under the hit raster, the label row and the legend.
fn build_el<S: 'static>(
    ctx: &ChartRenderContext<'_, S>,
    plan: &PieChartPlan,
    hovered_cell: Option<(usize, usize)>,
    k: &Kit,
    state: &Rc<RefCell<EngineState>>,
) -> El {
    // The pie area: the largest square fitting the room the column gives
    // (recharts' pie is a circle — the ResponsiveContainer's square; the
    // view_box stretch maps the unit circle to the inscribed circle of a
    // square). The parent's alignment centers it.
    let mut plot = el()
        .key("pie-chart-plot")
        .aspect_fit(1.0)
        .min_w(0.0)
        .min_h(0.0)
        .child(
            canvas(pie_shape(plan))
                .key(PIE_KEY)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        );
    // The hit raster: the rows grow equally (1/16 each) and the cells take
    // 1/16 of the width — the grid tiles the plot with no overlaps.
    let clear_state = Rc::clone(state);
    let mut zone = col()
        .key(HIT_ZONE_KEY)
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .on_hover_change(move |_s: &mut S, inside: bool, tx: &mut Tx| {
            if !inside {
                let mut st = clear_state.borrow_mut();
                st.hovered = None; // the pointer left the chart
                st.hovered_cell = None;
                // recharts' tooltip has no exit animation: it unmounts at once.
                tx.instant();
            }
        });
    for r in 0..HIT_GRID {
        let mut grid_row = row()
            .key(format!("pie-chart-hit-row-{r}"))
            .grow(1.0)
            .min_h(0.0);
        for c in 0..HIT_GRID {
            grid_row = grid_row.child(
                hit_cell(ctx, plan, r, c, hovered_cell, k, state)
                    .w_frac(1.0 / HIT_GRID as f32)
                    .h_full(),
            );
        }
        zone = zone.child(grid_row);
    }
    plot = plot.child(zone);

    let mut root = col()
        .key(ROOT_KEY)
        .w_full()
        .h_full()
        .min_w(0.0)
        .min_h(0.0)
        .child(plot);

    // The label row (the showValues gate, React 26-32).
    if ctx.config.show_values {
        root = root.child(labels_row(plan, k));
    }

    // The legend (the showLegend gate, React 52 — not multiSeries).
    if ctx.config.show_legend {
        root = root.child(legend_row(plan, k));
    }
    root
}
