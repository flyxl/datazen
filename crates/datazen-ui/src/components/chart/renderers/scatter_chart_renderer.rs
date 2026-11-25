//! Port of `packages/frontend/src/components/chart/renderers/ScatterChartRenderer.tsx` (T261).
//!
//! The React component is a `recharts` `<ScatterChart>` composition — the fifth
//! engine on the `ChartRenderer` seam (`chart_canvas.rs`), after the area
//! (T166), bar (T192), line (T216) and pie (T238) engines. It is the only one
//! with a **numeric X axis**: both axes are `type="number"`, both format their
//! ticks with `formatCompact`, and the domain of each is derived from its own
//! column. One `<Scatter>` paints every row as an equal-size dot (`ZAxis
//! range={[40, 40]}` → a fixed 40px diameter), `fill={colors[0]}` — a single
//! palette slot for the whole series, not one per row — plus a `CartesianGrid`,
//! a plain `<Tooltip>` with a dashed cursor, a `<Legend>` gated on
//! `config.showLegend` (not `multiSeries`: the scatter has one series), and the
//! per-point `onClick(_, index)`.
//!
//! ## How the recharts composition maps onto Rune
//!
//! * **Real geometry, not an approximation.** Each drawable row's `(x, y)` is
//!   mapped from its own numeric domain into the `view_box(1, 1)` plot
//!   fractions (`x = (xv - x0) / xspan`, `y = 1 - (yv - y0) / yspan`) and
//!   painted as a filled 16-gon — the dot. Grid hairlines, the axis frame and
//!   the hovered cursor crosshair are stroke/line ops in the same box. The
//!   whole mapping lives in [`ScatterChartPlan`] as pure data, observable
//!   without a runtime.
//! * **The plot frame** is the family's flexbox rebuild of the recharts margin
//!   box: a 60pt Y-axis column (five ticks), the growing plot stack and a 30pt
//!   X-axis row of five numeric-tick segments, plus the 22pt legend row when
//!   `showLegend`. Overlapping canvases stack through `.absolute(0.0, 0.0)`
//!   plus `.w_full().h_full()`, so grid, axes, dots, cursor and hit layer share
//!   one rect.
//! * **The hit raster.** recharts' per-point hit areas are the SVG dots.
//!   Rune's flexbox hit areas are rectangles, so the port rasters the plot into
//!   a 16×16 grid and assigns each cell to the nearest dot whose centre lies
//!   within [`HIT_RADIUS`] (ties → the lower row). Pressing a cell routes that
//!   row to `on_data_pointClick`; hovering mounts the tooltip; a cell with no
//!   dot in range routes nothing (the exact hit, rasterised).
//!
//! ## The seams (the split discipline — the area engine's, not forked)
//!
//! | React import | owner | shape here |
//! |--------------|-------|------------|
//! | `formatCompact` (`lib/chart/format.ts`) | T079 | [`FormatSeams`] — the area engine's injected seam; both axes' `tickFormatter` call it (the scatter never imports `formatAxisTick`/`formatEpochMs`) |
//! | `getColorPalette` (`lib/chart/colors.ts`) | T352 | [`series_palette`] — the five-colour kit palette; the scatter reads `palette[0]` only |
//! | the numeric domain + value coercions | T166's lean slices | [`lean_y_domain`] / [`to_number`] / [`js_string`] — imported; the same "default `[0, auto]`" domain both axes ride |
//!
//! [`NumericTick`] is declared here (the family's `XTick` is band-indexed and
//! cannot carry a numeric X position); the Y/X tick *values* still come from
//! the shared domain slice.
//!
//! ## Keys
//!
//! | part | key |
//! |------|-----|
//! | root / plot row / plot | `scatter-chart` / `scatter-chart-plot-row` / `scatter-chart-plot` |
//! | grid / axes / dots / cursor canvases | `scatter-chart-grid` / `scatter-chart-axes` / `scatter-chart-dots` / `scatter-chart-cursor` |
//! | hit zone / cell *r*-*c* | `scatter-chart-hit-zone` / `scatter-chart-hit-{r}-{c}` |
//! | Y axis / Y tick *k* | `scatter-chart-yaxis` / `scatter-chart-yaxis-tick-{k}` |
//! | X axis / X tick *k* | `scatter-chart-xaxis` / `scatter-chart-xaxis-tick-{k}` |
//! | legend / swatch / name | `scatter-chart-legend` / `scatter-chart-legend-swatch` / `scatter-chart-legend-name` |
//! | tooltip box / line *i* | `scatter-chart-tooltip` / `scatter-chart-tooltip-item-{i}` |
//!
//! ## Deliberate deviations
//!
//! * `ZAxis range={[40, 40]}` fixes the dot's **pixel** diameter. Rune's
//!   `Shape` has no circle primitive and a `view_box(1, 1)` canvas stretches x
//!   and y independently, so the dot is a 16-gon of radius [`DOT_RADIUS`] in
//!   view-box fractions — an ellipse that follows the plot's aspect, the same
//!   class of cosmetic trade as T166's straight-segment `monotone` line. The
//!   centre is exact; only the outline's roundness is approximated.
//! * recharts' "nice" tick picking is not ported: each axis draws five evenly
//!   spaced values across its own domain, with `formatCompact` labels. X tick
//!   labels are centred in midpoint-bounded segments so inner ticks land on
//!   their exact fractions; the two edge labels are inset half a gap to stay
//!   inside the plot (recharts centres them on the axis endpoints and clips).
//! * `CartesianGrid`'s `strokeDasharray="3 3"` and the tooltip cursor's dash
//!   paint as solid hairlines/crosshair (Rune strokes carry no dash) — the
//!   crosshair is a real vertical+horizontal line through the hovered dot.
//! * `<Scatter>` carries no `name` prop, so recharts' legend entry would be
//!   blank; the port labels the single swatch with `config.yAxes[0]` (the
//!   plotted value column) for legibility, pinned in tests. The legend is a
//!   plain row — React's scatter legend has no `onClick` toggle.
//! * The tooltip shows both coordinates as two `name : value` rows in the
//!   `contentStyle` box (recharts shows the x label plus one payload row). Both
//!   values use the raw slot display (`js_string`), the family's plain-tooltip
//!   arm. `cursor: 'pointer'` has no Rune counterpart.
//! * Null/NaN or absent x/y values paint no dot (recharts' gap) and contribute
//!   nothing to either domain. A drawable point is always inside its domain,
//!   so its view-box fractions are inside `[0, 1]`.
//! * The hit cells are `a11y_hidden` (a 1000-row chart would otherwise be a
//!   wall of buttons); the legend entry is a plain labelled row.

#![allow(dead_code)] // the chart-view track wires the factory; until then only tests exercise it

use std::cell::RefCell;
use std::rc::Rc;

use rune::{canvas, col, el, row, text, Align, El, LineCap, Shape, Tx};

use super::super::chart_canvas::{
    ChartConfig, ChartDataPoint, ChartRenderContext, ChartRenderer, ChartType, ChartValue,
    PlaceholderChartRenderer,
};
use super::area_chart_renderer::{js_string, lean_y_domain, series_palette, to_number};
/// The family's `lib/chart/format.ts` seam struct (T079) — the area engine's,
/// re-exported so the scatter engine's callers wire it through this module.
pub use super::area_chart_renderer::FormatSeams;
use crate::ui::kit::kit;
use rune_kit::Kit;

// ---- Keys -------------------------------------------------------------------

/// Root, recharts' `<ScatterChart>` frame.
pub const ROOT_KEY: &str = "scatter-chart";
/// The Y-axis + plot stack row.
pub const PLOT_ROW_KEY: &str = "scatter-chart-plot-row";
/// The plot — every canvas and the hit raster share its rect.
pub const PLOT_KEY: &str = "scatter-chart-plot";
/// The `CartesianGrid` canvas.
pub const GRID_KEY: &str = "scatter-chart-grid";
/// The axis frame canvas.
pub const AXES_KEY: &str = "scatter-chart-axes";
/// The dots canvas — every point paints in it.
pub const DOTS_KEY: &str = "scatter-chart-dots";
/// The hovered point's cursor crosshair canvas.
pub const CURSOR_KEY: &str = "scatter-chart-cursor";
/// The pointer/press raster over the plot.
pub const HIT_ZONE_KEY: &str = "scatter-chart-hit-zone";
/// One raster cell: `scatter-chart-hit-{r}-{c}`.
pub const HIT_CELL_PREFIX: &str = "scatter-chart-hit-";
/// The Y-axis label column.
pub const Y_AXIS_KEY: &str = "scatter-chart-yaxis";
/// The X-axis label row.
pub const X_AXIS_KEY: &str = "scatter-chart-xaxis";
/// The legend row (`config.showLegend` gate).
pub const LEGEND_KEY: &str = "scatter-chart-legend";
/// The legend's single swatch.
pub const LEGEND_SWATCH_KEY: &str = "scatter-chart-legend-swatch";
/// The legend's single name.
pub const LEGEND_NAME_KEY: &str = "scatter-chart-legend-name";
/// The plain `<Tooltip>` content box.
pub const TOOLTIP_KEY: &str = "scatter-chart-tooltip";

// recharts defaults / React literals, verbatim.
/// recharts' default `YAxis width`.
const Y_AXIS_W: f32 = 60.0;
/// recharts' default `XAxis height`.
const X_AXIS_H: f32 = 30.0;
/// One legend row at `fontSize: 12`.
const LEGEND_H: f32 = 22.0;
/// The legend swatch chip.
const SWATCH: f32 = 8.0;
/// The axis frame's stroke.
const AXIS_LINE_W: f32 = 1.0;
/// The grid/cursor hairline stroke.
const GRID_LINE_W: f32 = 1.0;
/// React `CartesianGrid` `opacity` when `showGrid`.
const GRID_OPACITY: f32 = 0.3;
/// The tooltip's anchor inside the hovered cell.
const TOOLTIP_DX: f32 = 2.0;
/// The tooltip's anchor below the cell's top.
const TOOLTIP_DY: f32 = 8.0;
/// recharts' default numeric-axis `tickCount`.
pub const NUM_TICKS: usize = 5;
/// The dot's radius in view-box fractions (≈20px on a 1000px plot, the
/// `ZAxis range={[40, 40]}` diameter). See the module docs' outline trade.
pub const DOT_RADIUS: f32 = 0.02;
/// The dot disc's polygon resolution (Rune has no circle primitive).
pub const DOT_GON_SIDES: usize = 16;
/// The hit raster's side count.
pub const HIT_GRID: usize = 16;
/// The pick radius in view-box fractions: how close a cell centre must be to
/// a dot for the press/hover to route to it (a little over twice the dot's
/// radius — a forgiving hit, like recharts' marker padding).
pub const HIT_RADIUS: f32 = 0.05;

// ---- The geometry plan ---------------------------------------------------------

/// One numeric-axis tick: the domain value, its `formatCompact` label and its
/// view-box fraction (0 at the left for X, 0 at the top for Y).
#[derive(Clone, Debug, PartialEq)]
pub struct NumericTick {
    pub value: f64,
    pub label: String,
    pub frac: f32,
}

/// Everything one frame paints, computed before any element: the React
/// component's reads (palette, keys, both numeric domains, the dot mapping and
/// the tick ladders) as pure data.
#[derive(Clone, Debug)]
pub struct ScatterChartPlan {
    /// The input rows (the plain tooltip reads their raw slots).
    pub data: Vec<ChartDataPoint>,
    /// `config.xAxis ?? '__index'` — the numeric X column.
    pub x_key: String,
    /// `config.yAxes[0]` — the numeric Y column (absent = no drawable rows).
    pub y_key: Option<String>,
    pub x_domain: [f64; 2],
    pub y_domain: [f64; 2],
    /// One per data row; `None` = null/NaN/absent x or y (recharts' gap).
    pub points: Vec<Option<[f32; 2]>>,
    /// Left to right, five ticks across the X domain.
    pub x_ticks: Vec<NumericTick>,
    /// Top first, five ticks across the Y domain.
    pub y_ticks: Vec<NumericTick>,
    /// Vertical grid fractions (the X tick positions).
    pub grid_xs: Vec<f32>,
    /// Horizontal grid fractions (the Y tick positions).
    pub grid_ys: Vec<f32>,
    pub row_count: usize,
    /// `colors[0]` — the one palette slot every dot shares.
    pub color: rune::Color,
    /// The legend's single entry name (`yAxes[0]` or empty).
    pub legend_name: String,
}

/// The engine's whole plan for one frame, over the area engine's shared lean
/// slices — see the seams table in the module docs.
pub fn scatter_chart_plan(
    data: &[ChartDataPoint],
    config: &ChartConfig,
    dark: bool,
    seams: &FormatSeams,
) -> ScatterChartPlan {
    let x_key = config.x_axis.clone().unwrap_or_else(|| "__index".into());
    let y_key = config.y_axes.first().cloned();

    // Each axis derives its own default `[0, auto]` domain from its finite
    // column values (a flat column widens by one; an empty one is [0, 1]).
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for point in data {
        if let Some(v) = to_number(point.get(&x_key).unwrap_or(&ChartValue::Null))
            .filter(|v| v.is_finite())
        {
            xs.push(v);
        }
        if let Some(v) = y_key
            .as_deref()
            .and_then(|key| to_number(point.get(key).unwrap_or(&ChartValue::Null)))
            .filter(|v| v.is_finite())
        {
            ys.push(v);
        }
    }
    let x_domain = lean_y_domain(&xs);
    let y_domain = lean_y_domain(&ys);
    let x_span = x_domain[1] - x_domain[0];
    let y_span = y_domain[1] - y_domain[0];

    // A row is drawable only when BOTH coordinates are finite numbers —
    // recharts paints a <Scatter> point only when its x and y dataKeys resolve.
    let points = data
        .iter()
        .map(|point| {
            let x = to_number(point.get(&x_key).unwrap_or(&ChartValue::Null))
                .filter(|v| v.is_finite())?;
            let y = y_key
                .as_deref()
                .and_then(|key| to_number(point.get(key).unwrap_or(&ChartValue::Null)))
                .filter(|v| v.is_finite())?;
            Some([
                ((x - x_domain[0]) / x_span) as f32,
                (1.0 - (y - y_domain[0]) / y_span) as f32,
            ])
        })
        .collect();

    // Both axes' tickFormatter is formatCompact (React 43 / 51).
    let x_ticks: Vec<NumericTick> = (0..NUM_TICKS)
        .map(|t| {
            let f = t as f64 / (NUM_TICKS - 1) as f64;
            let value = x_domain[0] + x_span * f;
            NumericTick {
                value,
                label: (seams.format_compact)(value),
                frac: f as f32,
            }
        })
        .collect();
    let y_ticks: Vec<NumericTick> = (0..NUM_TICKS)
        .map(|t| {
            let f = t as f64 / (NUM_TICKS - 1) as f64;
            let value = y_domain[1] - y_span * f; // top first
            NumericTick {
                value,
                label: (seams.format_compact)(value),
                frac: f as f32,
            }
        })
        .collect();
    let grid_xs: Vec<f32> = x_ticks.iter().map(|t| t.frac).collect();
    let grid_ys: Vec<f32> = y_ticks.iter().map(|t| t.frac).collect();

    let k = kit(dark);
    let color = series_palette(k)[0];
    let legend_name = y_key.clone().unwrap_or_default();

    ScatterChartPlan {
        data: data.to_vec(),
        x_key,
        y_key,
        x_domain,
        y_domain,
        points,
        x_ticks,
        y_ticks,
        grid_xs,
        grid_ys,
        row_count: data.len(),
        color,
        legend_name,
    }
}

// ---- The shapes ---------------------------------------------------------------

/// A regular n-gon (the dot disc — Rune has no circle primitive).
pub fn gon(center: [f32; 2], radius: f32, n: usize) -> Vec<[f32; 2]> {
    (0..n)
        .map(|i| {
            let a = i as f32 / n as f32 * std::f32::consts::TAU;
            [center[0] + radius * a.cos(), center[1] + radius * a.sin()]
        })
        .collect()
}

/// The `CartesianGrid`: hairlines at both tick ladders. React mounts it always;
/// `showGrid` false is `opacity: 0`, so the canvas stays and paints nothing.
fn grid_shape(plan: &ScatterChartPlan, k: &Kit, show_grid: bool) -> Shape {
    let mut shape = Shape::new().view_box(1.0, 1.0);
    if show_grid {
        let c = k.alpha(k.c.border_base, GRID_OPACITY);
        for f in &plan.grid_ys {
            shape = shape.line([0.0, *f], [1.0, *f], GRID_LINE_W, LineCap::Butt, c);
        }
        for f in &plan.grid_xs {
            shape = shape.line([*f, 0.0], [*f, 1.0], GRID_LINE_W, LineCap::Butt, c);
        }
    }
    shape
}

/// The axis frame: recharts' `stroke="var(--c-edge)"` Y (left) and X (bottom)
/// axis lines.
fn axes_shape(k: &Kit) -> Shape {
    Shape::new()
        .view_box(1.0, 1.0)
        .line([0.0, 0.0], [0.0, 1.0], AXIS_LINE_W, LineCap::Butt, k.c.border_base)
        .line([0.0, 1.0], [1.0, 1.0], AXIS_LINE_W, LineCap::Butt, k.c.border_base)
}

/// Every drawable dot, in the one palette colour (`colors[0]`).
fn dots_shape(plan: &ScatterChartPlan) -> Shape {
    let mut shape = Shape::new().view_box(1.0, 1.0);
    for point in plan.points.iter().flatten() {
        shape = shape.fill(gon(*point, DOT_RADIUS, DOT_GON_SIDES), plan.color);
    }
    shape
}

/// The hovered point's cursor: a solid crosshair through its view-box position
/// (React's `cursor={{ strokeDasharray: '3 3' }}` — dash traded for solid).
fn cursor_shape(point: [f32; 2], k: &Kit) -> Shape {
    Shape::new()
        .view_box(1.0, 1.0)
        .line([point[0], 0.0], [point[0], 1.0], GRID_LINE_W, LineCap::Butt, k.c.border_base)
        .line([0.0, point[1]], [1.0, point[1]], GRID_LINE_W, LineCap::Butt, k.c.border_base)
}

// ---- The tooltip ---------------------------------------------------------------

/// The raw slot display, the family's plain-tooltip arm (`js_string`).
fn slot_display(point: &ChartDataPoint, key: &str) -> String {
    match point.get(key) {
        Some(ChartValue::Text(s)) => s.clone(),
        Some(ChartValue::Number(n)) => js_string(*n),
        None | Some(ChartValue::Null) => String::new(),
    }
}

/// recharts' default entry line: `name : value`, or the value alone when the
/// name is empty (the `__index` fallback's absent column).
fn entry_line(name: &str, value: &str) -> String {
    if name.is_empty() {
        value.to_string()
    } else {
        format!("{name} : {value}")
    }
}

/// The hovered row's two coordinate lines: x first (recharts' tooltip label),
/// then y (the payload row).
fn tooltip_lines(plan: &ScatterChartPlan, index: usize) -> Vec<String> {
    if !plan.points.get(index).and_then(|p| p.as_ref()).is_some() {
        return Vec::new();
    }
    let point = &plan.data[index];
    let mut lines = vec![entry_line(&plan.x_key, &slot_display(point, &plan.x_key))];
    if let Some(key) = &plan.y_key {
        lines.push(entry_line(key, &slot_display(point, key)));
    }
    lines
}

/// The plain `<Tooltip>`'s content box: React's `contentStyle` chrome on the
/// family's token mapping (surface-alt → `layer_01`, edge → `border_base`,
/// radius 6 → `radius.md`, fg → `text_strong`, 12px → `text_xsmall`).
fn plain_tooltip_box(lines: &[String], k: &Kit) -> El {
    col()
        .key(TOOLTIP_KEY)
        .fill(k.c.layer_01)
        .border(1.0, k.c.border_base)
        .radius(k.radius.md)
        .children(lines.iter().enumerate().map(|(i, line)| {
            text(line.clone())
                .key(format!("{TOOLTIP_KEY}-item-{i}"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_strong)
        }))
}

// ---- The hit raster -------------------------------------------------------------

/// The row of the nearest dot within [`HIT_RADIUS`] of a plot point, or `None`
/// between dots (recharts hit only the marker itself). Ties take the lower row.
pub fn dot_at(plan: &ScatterChartPlan, x: f32, y: f32) -> Option<usize> {
    let r2 = HIT_RADIUS * HIT_RADIUS;
    let mut best: Option<(usize, f64)> = None;
    for (row, point) in plan.points.iter().enumerate() {
        let Some([px, py]) = point else { continue };
        let dx = x - px;
        let dy = y - py;
        let dist2 = dx as f64 * dx as f64 + dy as f64 * dy as f64;
        if dist2 > r2 as f64 {
            continue;
        }
        if best.map(|(_, d)| dist2 < d).unwrap_or(true) {
            best = Some((row, dist2));
        }
    }
    best.map(|(row, _)| row)
}

/// One raster cell: hovering a dot sets the hovered row (`activeTooltipIndex`)
/// and mounts the tooltip in this cell; pressing routes `onDataPointClick`.
#[allow(clippy::too_many_arguments)]
fn hit_cell<S: 'static>(
    ctx: &ChartRenderContext<'_, S>,
    plan: &ScatterChartPlan,
    r: usize,
    c: usize,
    hovered: Option<usize>,
    k: &Kit,
    state: &Rc<RefCell<EngineState>>,
) -> El {
    let centre = [
        (c as f32 + 0.5) / HIT_GRID as f32,
        (r as f32 + 0.5) / HIT_GRID as f32,
    ];
    let owner = dot_at(plan, centre[0], centre[1]);
    let mut cell = el()
        .key(format!("{HIT_CELL_PREFIX}{r}-{c}"))
        .a11y_hidden();
    let hover_state = Rc::clone(state);
    cell = cell.on_hover(move |_s: &mut S| {
        hover_state.borrow_mut().hovered = owner;
    });
    if let (Some(on_click), Some(row)) = (ctx.on_data_point_click.as_ref(), owner) {
        let on_click = Rc::clone(on_click);
        cell = cell.on_press(move |s: &mut S| on_click(s, row));
    }
    // One floating box: mount it only in the single hovered cell.
    if hovered.is_some() && owner == hovered {
        let lines = tooltip_lines(plan, hovered.unwrap_or(0));
        if !lines.is_empty() {
            cell = cell.child(plain_tooltip_box(&lines, k).absolute(TOOLTIP_DX, TOOLTIP_DY));
        }
    }
    cell
}

// ---- The engine -----------------------------------------------------------------

/// The engine's retained state — recharts' hovered point (`activeTooltipIndex`).
/// There is no hidden-series set: the scatter legend does not toggle.
#[derive(Default)]
struct EngineState {
    hovered: Option<usize>,
}

/// The scatter chart engine. Cloning shares [`EngineState`], so a factory that
/// clones one engine per build keeps the hover across frames — the retained
/// state React kept.
#[derive(Clone)]
pub struct ScatterChartRenderer {
    state: Rc<RefCell<EngineState>>,
    seams: FormatSeams,
}

impl ScatterChartRenderer {
    pub fn new() -> Self {
        Self {
            state: Rc::new(RefCell::new(EngineState::default())),
            seams: FormatSeams::default(),
        }
    }

    /// Wire the T079 `formatCompact` port (or a test probe) — both axes'
    /// `tickFormatter`.
    pub fn format_compact(mut self, f: Rc<dyn Fn(f64) -> String>) -> Self {
        self.seams.format_compact = f;
        self
    }

    /// recharts' active point index, as the engine keeps it.
    pub fn hovered_point(&self) -> Option<usize> {
        self.state.borrow().hovered
    }
}

impl Default for ScatterChartRenderer {
    fn default() -> Self {
        Self::new()
    }
}

/// The canvas's renderer-factory injection point: `ChartType::Scatter` gets
/// this engine, every other type keeps the honest placeholder.
pub fn renderer_factory<S: 'static>(
    engine: ScatterChartRenderer,
) -> impl Fn(ChartType) -> Box<dyn ChartRenderer<S> + 'static> {
    move |chart_type| match chart_type {
        ChartType::Scatter => Box::new(engine.clone()),
        other => Box::new(PlaceholderChartRenderer::new(other)),
    }
}

impl<S: 'static> ChartRenderer<S> for ScatterChartRenderer {
    fn render(&self, ctx: &ChartRenderContext<'_, S>) -> El {
        let k = kit(ctx.dark);
        let hovered = self.state.borrow().hovered;
        let plan = scatter_chart_plan(ctx.data, ctx.config, ctx.dark, &self.seams);
        // The hovered index is dropped when the row vanishes or becomes
        // undrawable: recharts clears activeTooltipIndex, the box unmounts.
        let hovered = hovered.filter(|i| plan.points.get(*i).and_then(|p| p.as_ref()).is_some());
        build_el(ctx, &plan, hovered, k, &self.state)
    }
}

/// The tree: the recharts `<ScatterChart>` composition as the family's flexbox
/// frame.
fn build_el<S: 'static>(
    ctx: &ChartRenderContext<'_, S>,
    plan: &ScatterChartPlan,
    hovered: Option<usize>,
    k: &Kit,
    state: &Rc<RefCell<EngineState>>,
) -> El {
    // The Y-axis column: five formatCompact ticks, right-aligned.
    let y_axis = col()
        .key(Y_AXIS_KEY)
        .w(Y_AXIS_W)
        .justify_between()
        .children(plan.y_ticks.iter().enumerate().map(|(t, tick)| {
            text(tick.label.clone())
                .key(format!("{Y_AXIS_KEY}-tick-{t}"))
                .w_full()
                .align(Align::End)
                .font(k.ty.text_xsmall) // tick fontSize: 12
                .color(k.c.text_muted) // fill: var(--c-fg-secondary)
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
        )
        .child(
            canvas(dots_shape(plan))
                .key(DOTS_KEY)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full(),
        );
    if let Some(row) = hovered {
        if let Some(Some(point)) = plan.points.get(row) {
            plot = plot.child(
                canvas(cursor_shape(*point, k))
                    .key(CURSOR_KEY)
                    .absolute(0.0, 0.0)
                    .w_full()
                    .h_full(),
            );
        }
    }

    // The hit raster over everything painted.
    let clear_state = Rc::clone(state);
    let mut zone = el()
        .key(HIT_ZONE_KEY)
        .absolute(0.0, 0.0)
        .w_full()
        .h_full()
        .on_hover_change(move |_s: &mut S, inside: bool, tx: &mut Tx| {
            if !inside {
                clear_state.borrow_mut().hovered = None;
                tx.instant(); // recharts' tooltip unmounts at once
            }
        });
    for r in 0..HIT_GRID {
        let mut grid_row = row()
            .key(format!("scatter-chart-hit-row-{r}"))
            .grow(1.0)
            .min_h(0.0);
        for c in 0..HIT_GRID {
            grid_row = grid_row.child(
                hit_cell(ctx, plan, r, c, hovered, k, state)
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
        .child(
            row()
                .key(PLOT_ROW_KEY)
                .grow(1.0)
                .min_w(0.0)
                .min_h(0.0)
                .child(y_axis)
                .child(plot),
        );

    // The X-axis row: five numeric ticks. Each label sits in the segment
    // bounded by the midpoints to its neighbours (edges clamp to the plot), so
    // an inner tick's label centre is its exact numeric fraction.
    let mut x_axis = row().key(X_AXIS_KEY).h(X_AXIS_H).grow(1.0).min_w(0.0);
    let boundaries: Vec<f32> = (0..=NUM_TICKS)
        .map(|i| match i {
            0 => 0.0,
            i if i == NUM_TICKS => 1.0,
            i => (plan.x_ticks[i - 1].frac + plan.x_ticks[i].frac) * 0.5,
        })
        .collect();
    for (t, tick) in plan.x_ticks.iter().enumerate() {
        let width = boundaries[t + 1] - boundaries[t];
        x_axis = x_axis.child(
            text(tick.label.clone())
                .key(format!("{X_AXIS_KEY}-tick-{t}"))
                .w_frac(width)
                .align(Align::Center)
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .wrap(),
        );
    }
    root = root.child(row().child(el().w(Y_AXIS_W)).child(x_axis));

    // The legend (showLegend gate — the scatter has one series by construction).
    if ctx.config.show_legend {
        root = root.child(
            row()
                .key(LEGEND_KEY)
                .h(LEGEND_H)
                .w_full()
                .center()
                .gap(SWATCH)
                .child(
                    el().key(LEGEND_SWATCH_KEY)
                        .w(SWATCH)
                        .h(SWATCH)
                        .radius(k.radius.sm)
                        .fill(plan.color),
                )
                .child(
                    text(plan.legend_name.clone())
                        .key(LEGEND_NAME_KEY)
                        .font(k.ty.text_xsmall) // wrapperStyle fontSize: 12
                        .color(k.c.text_strong), // color: var(--c-fg, #eee)
                ),
        );
    }
    root
}
