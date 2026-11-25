//! Port of `packages/frontend/src/components/chart/ChartView.tsx` (T102, the
//! container the older ledger numbered T104).
//!
//! The React component is the chart family's consumer: it mounts the
//! T059 parts this crate already ports — the toolbar strip, the empty-state
//! pane and the canvas dispatcher — around the app's chart state. Its own
//! semantics are the container ones: derive the fields and recommendation
//! from the statement result, initialise (and re-derive) the chart config,
//! cap the plotted rows at [`MAX_CHART_ROWS`], fold a grouped config into
//! pivot series, render the split-view tile grid or the single canvas, and
//! mount the expanded overlay over the whole window.
//!
//! ## Keys
//!
//! | part               | key                              | React anchor |
//! |--------------------|----------------------------------|--------------|
//! | root               | `chart-view`                     | `flex flex-1 flex-col min-h-0` |
//! | toolbar            | (the T059 strip's own keys)      | `<ChartToolbar/>` |
//! | body row           | `chart-view-body`                | `flex flex-1 min-h-0` |
//! | axis panel slot    | `chart-view-axis-slot` (+ `-note`)| `<AxisConfigurator/>` root |
//! | chart area         | `chart-view-area`                | `div ref={chartRef}` |
//! | split grid         | `chart-view-split-grid`           | `grid grid-cols-1 gap-2 p-2 md:grid-cols-2` |
//! | split tile         | `chart-view-tile-{axis}`          | `div key={sc.yAxes[0]}` |
//! | tile chip          | `chart-view-tile-chip-{axis}`     | the absolute `{sc.yAxes[0]}` label |
//! | tile canvas mount  | `chart-view-tile-canvas-{epoch}-{axis}` | `key={`${paletteEpoch}-${axis}`}` |
//! | main canvas mount  | `chart-view-canvas-{epoch}`      | `key={paletteEpoch}` |
//! | overlay root       | `chart-view-expand`              | `fixed inset-0 top-10 z-50` |
//! | overlay bar        | `chart-view-expand-bar`          | the overlay's toolbar row |
//! | overlay title      | `chart-view-expand-title`        | `t('chart.expandTitle')` span |
//! | overlay spacer     | `chart-view-expand-spacer`       | `flex-1` div |
//! | overlay PNG/SVG    | `chart-view-expand-png` / `-svg` | the export buttons |
//! | overlay separator  | `chart-view-expand-sep`          | `mx-1 h-4 w-px bg-edge` |
//! | overlay close      | `chart-view-expand-close`        | Minimize2 button |
//! | overlay content    | `chart-view-expand-content`      | `flex flex-1 min-h-0` |
//! | overlay chart area | `chart-view-expand-area`         | `div ref={expandedChartRef}` |
//! | overlay canvas     | `chart-view-expand-canvas-{epoch}` | `key={paletteEpoch}` |
//!
//! ## State: the immediate-mode split
//!
//! React's four `useState`s (`config`, `expanded`, `splitView`,
//! `paletteEpoch`) live in the caller here, exactly as the T059 toolbar
//! lifted its dropdown state: the app owns the slice and rebuilds the view
//! from it. Two of React's effects fold into caller wiring, the same move
//! `data_table/pagination.rs` documents for its page-reset effect:
//!
//! * The config initialiser + re-derivation effect (saved config wins, else
//!   the recommendation, else the first numeric column; notify on derive)
//!   becomes [`sync_config`], which the caller applies when `savedConfig` or
//!   the result changes — React's dependency list. The notify it returns is
//!   the `onConfigChange(derived)` side of the effect.
//! * `setConfig` (state + `onConfigChange`) is the single [`Self::on_change`]
//!   seam: the toolbar's and the configurator's edits arrive there, and the
//!   caller's closure sets its state and forwards the notification.
//!
//! ## Seams (honest, not faked)
//!
//! | seam                    | owner                          | shape |
//! |-------------------------|--------------------------------|-------|
//! | [`ChartPipeline`]       | `lib/chart/fieldInference.ts`, `lib/chart/recommend.ts`, `lib/chart/transform.ts` | required trait injection: `infer_fields` / `recommend` / `transform` |
//! | [`Self::on_export`]    | `lib/chart/export.ts`          | the PNG/SVG request only; the encoding, the `chart-${Date.now()}` filename and the clock that seeds it are the caller's |
//! | [`Self::axis_configurator`] | `components/chart/AxisConfigurator.tsx` (T300) | optional slot render; the default is an honest placeholder |
//! | [`Self::renderer_factory`] | the five chart-engine tasks   | passed through to every `ChartCanvas` (the dispatcher's documented injection point) |
//! | `palette_epoch`         | the app's theme-pack track     | the counter the caller bumps on theme-pack change (no Rune event channel exists yet) |
//! | `StatementResult` / `StatementCell` / [`ChartRecommendation`] / [`TransformResult`] | `types/index.ts` (T213) and the lib/chart tasks | lean mirrors; see each type |
//!
//! React's two export paths (the toolbar dropdown on `chartRef`, the overlay
//! buttons on `expandedChartRef`) both end at `exportChartAsPng/Svg`, so both
//! route to the single [`Self::on_export`] seam; the caller knows its own
//! `expanded` state and picks the surface to serialise. The refs themselves
//! only ever fed that path, so they collapse into the stable
//! `chart-view-area` / `chart-view-expand-area` keys.
//!
//! ## Deliberate deviations
//!
//! * `createPortal(…, document.body)` becomes an overlay pushed into the
//!   caller's `layer` (the `rune_kit::root` channel — the repo's top-layer
//!   portal equivalent, as the dialog base uses it): `fixed inset-0 top-10`
//!   pins to the window's edges with a 40pt top inset (`El::pin`), so the
//!   overlay covers the window below the app's 40pt top band.
//! * React `key={paletteEpoch}` (remount identity, never a DOM attribute)
//!   becomes the epoch in the canvas mount keys above, so the tree records
//!   which palette epoch painted the canvas — the observable remount.
//! * `backdrop-blur-sm` has no Rune equivalent and is dropped; the 95%
//!   surface fill (React `bg-surface/95`) stays.
//! * `md:grid-cols-2` resolves to a fixed two columns: the app's minimum
//!   window is 960pt, above Tailwind's `md` (768pt), matching the
//!   `data_table/pagination.rs` breakpoint ruling.
//! * The overlay buttons' `hover:bg-surface-alt` takes the kit's semantic
//!   hover overlay (the same collapse the T059 toolbar documented for its
//!   `hover:bg-surface`); `transition-colors` paints states directly.
//! * `overflow-y-auto` on the chart area is the standard `scroll`
//!   viewport; `shrink-0` has no inverse builder (the toolbar's ruling) and
//!   the fixed-width axis panel keeps its 208pt anyway.
//! * React's whole memo chain — fields, recommendation, `chartResult`
//!   (the row cap), `transformData`, `renderConfig` and `splitConfigs` —
//!   computes above the early returns because hooks must precede them
//!   (source #310): the transform seam therefore fires once per build even
//!   when an empty arm renders. This port evaluates the same chain before
//!   the early returns, and everything downstream is identical.

use std::borrow::Cow;
use std::rc::Rc;

use rune::{col, el, icon, row, text, El, ScrollSpec};
use rune_kit::Kit;

use super::chart_canvas::{
    ChartCanvas, ChartConfig, ChartDataPoint, ChartField, ChartRenderer, ChartType,
    InferredFieldType, RendererFactory,
};
use super::chart_empty_state::{ChartEmptyReason, ChartEmptyState};
use super::chart_toolbar::{ChartToolbar, ExportFormat};
use crate::i18n::t;
use crate::icons::ICON_MINIMIZE_2;
use crate::ui::kit::kit;

/// The view root, React's `flex flex-1 flex-col min-h-0`.
pub const ROOT_KEY: &str = "chart-view";
/// The toolbar + axis panel + chart area row, React's `flex flex-1 min-h-0`.
pub const BODY_KEY: &str = "chart-view-body";
/// The axis-configurator slot (React `<AxisConfigurator/>`).
pub const AXIS_SLOT_KEY: &str = "chart-view-axis-slot";
/// The honest note the unported configurator slot carries.
pub const AXIS_SLOT_NOTE_KEY: &str = "chart-view-axis-slot-note";
/// The scrollable chart area, React's `div ref={chartRef}`.
pub const AREA_KEY: &str = "chart-view-area";
/// The split-view tile grid, React's `grid grid-cols-1 gap-2 p-2 md:grid-cols-2`.
pub const SPLIT_GRID_KEY: &str = "chart-view-split-grid";
/// The expanded overlay, React's `fixed inset-0 top-10 z-50` portal.
pub const OVERLAY_KEY: &str = "chart-view-expand";
/// The overlay's toolbar row.
pub const OVERLAY_BAR_KEY: &str = "chart-view-expand-bar";
/// The overlay's title (`t('chart.expandTitle')`).
pub const OVERLAY_TITLE_KEY: &str = "chart-view-expand-title";
/// The overlay bar's `flex-1` spacer.
pub const OVERLAY_SPACER_KEY: &str = "chart-view-expand-spacer";
/// The overlay's PNG button.
pub const OVERLAY_PNG_KEY: &str = "chart-view-expand-png";
/// The overlay's SVG button.
pub const OVERLAY_SVG_KEY: &str = "chart-view-expand-svg";
/// The overlay bar's hairline before the close button.
pub const OVERLAY_SEP_KEY: &str = "chart-view-expand-sep";
/// The overlay's close button (`t('chart.collapse')`).
pub const OVERLAY_CLOSE_KEY: &str = "chart-view-expand-close";
/// The overlay's configurator + canvas row.
pub const OVERLAY_CONTENT_KEY: &str = "chart-view-expand-content";
/// The overlay's chart area, React's `div ref={expandedChartRef}`.
pub const OVERLAY_AREA_KEY: &str = "chart-view-expand-area";

// Tailwind geometry, verbatim.
/// React `top-10` — the overlay starts below the app's 40pt top band.
const OVERLAY_TOP: f32 = 40.0;
/// React `bg-surface/95`.
const OVERLAY_BG_ALPHA: f32 = 0.95;
/// React `gap-2` on the overlay bar.
const BAR_GAP: f32 = 8.0;
/// React `px-4` on the overlay bar.
const BAR_PAD_X: f32 = 16.0;
/// React `py-2` on the overlay bar.
const BAR_PAD_Y: f32 = 8.0;
/// React `mx-1` around the overlay separator.
const SEP_MX: f32 = 4.0;
/// React `h-4` on the overlay separator.
const SEP_H: f32 = 16.0;
/// React `w-px` on the overlay separator.
const SEP_W: f32 = 1.0;
/// React `px-2` on the overlay buttons.
const BUTTON_PAD_X: f32 = 8.0;
/// React `py-1` on the overlay buttons.
const BUTTON_PAD_Y: f32 = 4.0;
/// React `h-3.5 w-3.5` — the close glyph.
const ICON_SIZE: f32 = 14.0;
/// React `gap-2` on the split grid.
const SPLIT_GAP: f32 = 8.0;
/// React `p-2` on the split grid.
const SPLIT_PAD: f32 = 8.0;
/// React `md:grid-cols-2` — see the module doc's breakpoint ruling.
const SPLIT_COLUMNS: usize = 2;
/// React `h-56` — the split tile height.
const TILE_H: f32 = 224.0;
/// React's grid `minHeight: splitConfigs.length * 200`.
const TILE_MIN_H_BASE: f32 = 200.0;
/// React `left-2` on the tile chip.
const CHIP_X: f32 = 8.0;
/// React `top-1` on the tile chip.
const CHIP_Y: f32 = 4.0;
/// React `w-52` — the axis configurator panel (and its placeholder slot).
const AXIS_SLOT_W: f32 = 208.0;

/// React `MAX_CHART_ROWS` — the plotted-rows cap (`chartResult`'s slice).
pub const MAX_CHART_ROWS: usize = 1000;

type ChangeFn<S> = Rc<dyn Fn(&mut S, ChartConfig)>;
type ActionFn<S> = Rc<dyn Fn(&mut S)>;
type ExportFn<S> = Rc<dyn Fn(&mut S, ExportFormat)>;
type DataPointClickFn<S> = Rc<dyn Fn(&mut S, usize)>;

// ---- Lean mirrors ------------------------------------------------------------

/// Lean mirror of `StatementResult` (the wire shape `types/index.ts` re-exports
/// from `@datazen/backend-client`; the full port is the T213 types track). The
/// chart pipeline reads only the column descriptors and the row cells, so the
/// mirror carries exactly those — `sql`, provenance, timing and flags are out.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct StatementResult {
    /// React `columns: ColumnInfo[]`.
    pub columns: Vec<ColumnInfo>,
    /// React `rows: (Value | null)[][]`.
    pub rows: Vec<Vec<StatementCell>>,
}

/// Lean mirror of `ColumnInfo`: field inference reads `name` and `dataType`
/// only (`nullable` stays with the T213 full mirror).
#[derive(Clone, Debug, PartialEq)]
pub struct ColumnInfo {
    pub name: String,
    pub data_type: String,
}

/// Lean mirror of the wire `Value` union's chart-relevant cell space. The
/// closed scalar members plus a member for the union's non-scalars (records
/// and arrays): inference distinguishes those classes, but their payloads
/// never cross into the chart VM (the same ruling `ChartField.sampleValues`
/// documents).
#[derive(Clone, Debug, PartialEq)]
pub enum StatementCell {
    Text(String),
    Number(f64),
    Bool(bool),
    Null,
    /// A record or array cell — present, non-scalar, payload opaque here.
    NonScalar,
}

/// Lean mirror of `ChartRecommendation`: the five fields
/// `recommendationToConfig` copies. The full wire shape (confidence, reason)
/// lives with the `datazen-ui-api` chart vocabulary (T090) and the
/// `lib/chart/recommend.ts` track.
#[derive(Clone, Debug, PartialEq)]
pub struct ChartRecommendation {
    pub chart_type: ChartType,
    pub x_axis: Option<String>,
    pub y_axes: Vec<String>,
    pub group_by: Option<String>,
    pub aggregation: super::chart_canvas::AggregationType,
}

/// Lean mirror of `lib/chart/transform.ts`'s `TransformResult` — the pivot
/// output the renderers and this view consume. Owned by that lib task; the
/// family's `ChartDataPoint` (in `chart_canvas.rs`) is the row shape.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransformResult {
    pub data: Vec<ChartDataPoint>,
    pub series_keys: Vec<String>,
}

// ---- The lib/chart pipeline seam ---------------------------------------------

/// The `lib/chart` pure functions React imports statically, as one injected
/// pipeline. The three members are owned by their own migration tracks
/// (`fieldInference.ts`, `recommend.ts`, `transform.ts`); until they land, a
/// caller wires its own implementation and this module stays out of the
/// inference/aggregation business.
pub trait ChartPipeline {
    /// React `inferAllFields(result)` → the result's field descriptors.
    fn infer_fields(&self, result: &StatementResult) -> Vec<ChartField>;
    /// React `recommendChart(fields, rowCount)` → `None` when no rule fires.
    fn recommend(&self, fields: &[ChartField], row_count: usize) -> Option<ChartRecommendation>;
    /// React `transformData(result, config)` → the plot rows and series keys.
    fn transform(&self, result: &StatementResult, config: &ChartConfig) -> TransformResult;
}

// ---- Config derivation (React's initialiser + re-derivation effect) -----------

/// What [`sync_config`] resolves to: the config to adopt, plus the
/// `onConfigChange` payload when a derivation (not a saved config) produced
/// it — React's effect notifies only on that path.
#[derive(Clone, Debug)]
pub struct ConfigSync {
    pub config: ChartConfig,
    /// `Some(config)` when the caller should forward it to `onConfigChange`.
    pub notify: Option<ChartConfig>,
}

/// React's config initialiser and its re-derivation effect, folded into one
/// pure function the caller applies when `savedConfig` or the result changes
/// (React's `[savedConfig, recommendation, fields, onConfigChange]`
/// dependency list): a saved config wins outright; otherwise the
/// recommendation, else the default, and the derived value is notified.
pub fn sync_config(
    saved: Option<&ChartConfig>,
    recommendation: Option<&ChartRecommendation>,
    fields: &[ChartField],
) -> ConfigSync {
    match saved {
        Some(saved) => ConfigSync {
            config: saved.clone(),
            notify: None,
        },
        None => {
            let derived = match recommendation {
                Some(rec) => recommendation_to_config(rec),
                None => default_config(fields),
            };
            ConfigSync {
                notify: Some(derived.clone()),
                config: derived,
            }
        }
    }
}

/// React `recommendationToConfig`: the default config with the
/// recommendation's five fields copied over.
pub fn recommendation_to_config(rec: &ChartRecommendation) -> ChartConfig {
    ChartConfig {
        chart_type: rec.chart_type,
        x_axis: rec.x_axis.clone(),
        y_axes: rec.y_axes.clone(),
        group_by: rec.group_by.clone(),
        aggregation: rec.aggregation,
        ..ChartConfig::default()
    }
}

/// React `defaultConfig`: the default config plotting the first numeric
/// column, or no series when the result has none.
pub fn default_config(fields: &[ChartField]) -> ChartConfig {
    let numerics: Vec<&ChartField> = fields
        .iter()
        .filter(|f| f.inferred_type == InferredFieldType::Numeric)
        .collect();
    ChartConfig {
        y_axes: numerics.first().map(|f| f.name.clone()).into_iter().collect(),
        ..ChartConfig::default()
    }
}

/// React `chartResult`: the result with its rows capped at
/// [`MAX_CHART_ROWS`] (the same object when under the cap).
fn truncate(result: &StatementResult) -> Cow<'_, StatementResult> {
    if result.rows.len() > MAX_CHART_ROWS {
        Cow::Owned(StatementResult {
            columns: result.columns.clone(),
            rows: result.rows[..MAX_CHART_ROWS].to_vec(),
        })
    } else {
        Cow::Borrowed(result)
    }
}

/// React `renderConfig`: a grouped config folds to its pivot series (the
/// transform's `seriesKeys`) with the grouping cleared; an ungrouped config
/// passes through.
fn render_config(config: &ChartConfig, series_keys: &[String]) -> ChartConfig {
    match config.group_by.as_deref() {
        None => config.clone(),
        Some(_) => ChartConfig {
            y_axes: series_keys.to_vec(),
            group_by: None,
            ..config.clone()
        },
    }
}

// ---- The axis-configurator slot -----------------------------------------------

/// What the AxisConfigurator slot receives — React's `AxisConfiguratorProps`
/// (fields, config, recommendation, `onChange` as the view's own
/// [`ChartView::on_change`] seam). T300 owns the panel; the slot closure can
/// clone the `on_change` handle into its own press handlers.
pub struct AxisSlot<'a, S: 'static> {
    pub fields: &'a [ChartField],
    pub config: &'a ChartConfig,
    pub recommendation: Option<&'a ChartRecommendation>,
    pub on_change: Option<ChangeFn<S>>,
}

/// The slot render seam: an injected configurator builds its element from the
/// slot context.
pub type AxisSlotFn<S> = Rc<dyn for<'a> Fn(&'a AxisSlot<'a, S>) -> El>;

// ---- The view ----------------------------------------------------------------

/// The chart family's container: toolbar, axis slot, chart area (single
/// canvas, split tiles, or the no-config empty state) and the expanded
/// overlay.
pub struct ChartView<'a, S: 'static> {
    result: &'a StatementResult,
    pipeline: &'a dyn ChartPipeline,
    config: &'a ChartConfig,
    expanded: bool,
    split_view: bool,
    palette_epoch: u32,
    export_open: bool,
    on_change: Option<ChangeFn<S>>,
    on_toggle_split: Option<ActionFn<S>>,
    on_expand: Option<ActionFn<S>>,
    on_close: Option<ActionFn<S>>,
    on_export_toggle: Option<ActionFn<S>>,
    on_export_close: Option<ActionFn<S>>,
    on_export: Option<ExportFn<S>>,
    on_data_point_click: Option<DataPointClickFn<S>>,
    axis_slot: Option<AxisSlotFn<S>>,
    renderer_factory: Option<Rc<RendererFactory<S>>>,
}

impl<'a, S: 'static> ChartView<'a, S> {
    /// The React props' required trio: the result, the `lib/chart` pipeline
    /// and the live config.
    pub fn new(
        result: &'a StatementResult,
        pipeline: &'a dyn ChartPipeline,
        config: &'a ChartConfig,
    ) -> Self {
        Self {
            result,
            pipeline,
            config,
            expanded: false,
            split_view: false,
            palette_epoch: 0,
            export_open: false,
            on_change: None,
            on_toggle_split: None,
            on_expand: None,
            on_close: None,
            on_export_toggle: None,
            on_export_close: None,
            on_export: None,
            on_data_point_click: None,
            axis_slot: None,
            renderer_factory: None,
        }
    }

    /// React `expanded` — the overlay state, owned by the caller.
    pub fn expanded(mut self, expanded: bool) -> Self {
        self.expanded = expanded;
        self
    }

    /// React `splitView` — the split-tile state, owned by the caller.
    pub fn split_view(mut self, split: bool) -> Self {
        self.split_view = split;
        self
    }

    /// React `paletteEpoch` — bumped by the caller when the theme pack
    /// changes (React listens for `datazen:theme-pack-changed`; no Rune
    /// channel exists yet, so the counter is the seam).
    pub fn palette_epoch(mut self, epoch: u32) -> Self {
        self.palette_epoch = epoch;
        self
    }

    /// React `exportOpen` (the toolbar dropdown's state, owned by the caller
    /// per the T059 immediate-mode split).
    pub fn export_open(mut self, open: bool) -> Self {
        self.export_open = open;
        self
    }

    /// React `setConfig`: toolbar/configurator edits arrive as a full
    /// replacement config; the caller sets its state and forwards the
    /// `onConfigChange` notification in this closure.
    pub fn on_change(mut self, f: impl Fn(&mut S, ChartConfig) + 'static) -> Self {
        self.on_change = Some(Rc::new(f));
        self
    }

    /// React `onToggleSplit` (the toolbar's split button).
    pub fn on_toggle_split(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_toggle_split = Some(Rc::new(f));
        self
    }

    /// React `onExpand` (the toolbar's expand button: `setExpanded(true)`).
    pub fn on_expand(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_expand = Some(Rc::new(f));
        self
    }

    /// The overlay's close button: React `onClose` (`setExpanded(false)`).
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// The toolbar export trigger's press (React flips `exportOpen`).
    pub fn on_export_toggle(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_export_toggle = Some(Rc::new(f));
        self
    }

    /// The toolbar export dropdown's dismissal.
    pub fn on_export_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_export_close = Some(Rc::new(f));
        self
    }

    /// The export request — React's `exportChartAsPng/Svg(el, filename)` on
    /// the chart element. The encoding and the `chart-${Date.now()}` filename
    /// (with its clock) are the caller's; both the toolbar dropdown and the
    /// overlay's buttons route here.
    pub fn on_export(mut self, f: impl Fn(&mut S, ExportFormat) + 'static) -> Self {
        self.on_export = Some(Rc::new(f));
        self
    }

    /// React `onDataPointClick`, threaded to every non-overlay canvas.
    pub fn on_data_point_click(mut self, f: impl Fn(&mut S, usize) + 'static) -> Self {
        self.on_data_point_click = Some(Rc::new(f));
        self
    }

    /// The AxisConfigurator slot (T300): an injected panel renders from
    /// [`AxisSlot`]. Unwired, the layout keeps an honest placeholder.
    pub fn axis_configurator(
        mut self,
        f: impl for<'b> Fn(&'b AxisSlot<'b, S>) -> El + 'static,
    ) -> Self {
        self.axis_slot = Some(Rc::new(f));
        self
    }

    /// The renderer factory passed through to every canvas — the
    /// dispatcher's documented injection point for the five chart engines.
    pub fn renderer_factory(
        mut self,
        f: impl Fn(ChartType) -> Box<dyn ChartRenderer<S> + 'static> + 'static,
    ) -> Self {
        self.renderer_factory = Some(Rc::new(f));
        self
    }

    /// Builds the view. `layer` is the caller's top overlay channel (the
    /// `document.body` portal target): the toolbar's dropdowns and the
    /// expanded overlay mount there, the overlay last.
    pub fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        // React's whole memo chain evaluates above the early returns — hooks
        // must precede them (source #310): fields → recommendation →
        // chartResult (the row cap) → transformData → renderConfig →
        // splitConfigs. The transform seam therefore fires once per build
        // even when an empty arm renders, over the capped result and with
        // the live config.
        let fields = self.pipeline.infer_fields(self.result);
        let recommendation = self.pipeline.recommend(&fields, self.result.rows.len());
        let chart_result = truncate(self.result);
        let transform = self.pipeline.transform(&chart_result, self.config);
        let render_config = render_config(self.config, &transform.series_keys);
        let show_split = self.split_view && self.config.y_axes.len() > 1;
        let split_configs: Vec<ChartConfig> = if show_split {
            self.config
                .y_axes
                .iter()
                .map(|y_key| ChartConfig {
                    y_axes: vec![y_key.clone()],
                    ..render_config.clone()
                })
                .collect()
        } else {
            Vec::new()
        };

        // React's early returns, order preserved: empty rows first, then a
        // result with no numeric column. Either replaces the whole view.
        if self.result.rows.is_empty() {
            return ChartEmptyState::new(ChartEmptyReason::NoData).build(dark);
        }
        if !fields
            .iter()
            .any(|f| f.inferred_type == InferredFieldType::Numeric)
        {
            return ChartEmptyState::new(ChartEmptyReason::NoNumericField).build(dark);
        }

        let derived = Derived {
            fields,
            recommendation,
            transform,
            render_config,
            show_empty_canvas: self.config.y_axes.is_empty(),
            show_split,
            split_configs,
        };

        let k = kit(dark);
        let page = col()
            .key(ROOT_KEY)
            .grow(1.0) // React `flex-1`
            .min_w(0.0)
            .min_h(0.0) // React `min-h-0`
            .child(self.toolbar(dark, layer))
            .child(self.body(k, dark, &derived));
        if self.expanded {
            layer.push(self.overlay(k, dark, &derived));
        }
        page
    }

    /// The T059 strip with this view's state and seams wired through.
    fn toolbar(&self, dark: bool, layer: &mut Vec<El>) -> El {
        let mut toolbar = ChartToolbar::<S>::new(self.config)
            .split_view(self.split_view)
            .export_open(self.export_open);
        if let Some(on_change) = self.on_change.clone() {
            toolbar = toolbar.on_change(move |s: &mut S, c| on_change(s, c));
        }
        if let Some(on_toggle_split) = self.on_toggle_split.clone() {
            toolbar = toolbar.on_toggle_split(move |s: &mut S| on_toggle_split(s));
        }
        if let Some(on_expand) = self.on_expand.clone() {
            toolbar = toolbar.on_expand(move |s: &mut S| on_expand(s));
        }
        if let Some(on_export_toggle) = self.on_export_toggle.clone() {
            toolbar = toolbar.on_export_toggle(move |s: &mut S| on_export_toggle(s));
        }
        if let Some(on_export_close) = self.on_export_close.clone() {
            toolbar = toolbar.on_export_close(move |s: &mut S| on_export_close(s));
        }
        if let Some(on_export) = self.on_export.clone() {
            toolbar = toolbar.on_export(move |s: &mut S, f| on_export(s, f));
        }
        toolbar.build(dark, layer)
    }

    /// React's `flex flex-1 min-h-0` row: the axis panel beside the chart
    /// area.
    fn body(&self, k: &Kit, dark: bool, d: &Derived) -> El {
        row()
            .key(BODY_KEY)
            .grow(1.0)
            .min_w(0.0)
            .min_h(0.0)
            .child(self.axis_panel(k, d))
            .child(self.area(k, dark, d))
    }

    /// The AxisConfigurator slot, or its honest placeholder in the React
    /// panel's geometry (`flex w-52 shrink-0 flex-col border-r border-edge
    /// bg-surface-alt`).
    fn axis_panel(&self, k: &Kit, d: &Derived) -> El {
        if let Some(slot) = self.axis_slot.as_ref() {
            let ctx = AxisSlot {
                fields: &d.fields,
                config: self.config,
                recommendation: d.recommendation.as_ref(),
                on_change: self.on_change.clone(),
            };
            return slot(&ctx);
        }
        col()
            .key(AXIS_SLOT_KEY)
            .w(AXIS_SLOT_W)
            .min_w(0.0)
            .border_sides([0.0, 1.0, 0.0, 0.0], k.c.border_base) // `border-r border-edge`
            .fill(k.c.layer_01) // `bg-surface-alt` (the toolbar's mapping)
            .child(
                text("AxisConfigurator not ported")
                    .key(AXIS_SLOT_NOTE_KEY)
                    .font(k.ty.text_small)
                    .color(k.c.text_muted)
                    .wrap(),
            )
    }

    /// React's `div ref={chartRef} className="relative flex-1 min-h-0
    /// overflow-y-auto"`: the empty-config pane, the split grid or the
    /// single canvas.
    fn area(&self, k: &Kit, dark: bool, d: &Derived) -> El {
        let mut area = col()
            .key(AREA_KEY)
            .grow(1.0)
            .min_w(0.0)
            .min_h(0.0)
            .scroll(ScrollSpec { stick_bottom: false }); // `overflow-y-auto`
        if d.show_empty_canvas {
            area = area.child(ChartEmptyState::new(ChartEmptyReason::NoConfig).build(dark));
        } else if d.show_split {
            area = area.child(self.split_grid(k, dark, d));
        } else {
            let mount = format!("chart-view-canvas-{}", self.palette_epoch);
            area = area.child(self.canvas_mount(dark, &d.render_config, &d.transform.data, false, mount, true));
        }
        area
    }

    /// React's split tile grid: `grid grid-cols-1 gap-2 p-2 md:grid-cols-2`
    /// with `minHeight: splitConfigs.length * 200`, one tile per Y axis.
    fn split_grid(&self, k: &Kit, dark: bool, d: &Derived) -> El {
        let mut grid = el()
            .key(SPLIT_GRID_KEY)
            .w_full()
            .equal_columns(SPLIT_COLUMNS) // `md:grid-cols-2` (see the ruling)
            .gap(SPLIT_GAP)
            .padding(SPLIT_PAD)
            .min_h(d.split_configs.len() as f32 * TILE_MIN_H_BASE);
        for sc in &d.split_configs {
            let axis = sc.y_axes[0].clone();
            let mount = format!("chart-view-tile-canvas-{}-{axis}", self.palette_epoch);
            let tile = col()
                .key(format!("chart-view-tile-{axis}")) // React `key={sc.yAxes[0]}`
                .h(TILE_H) // `h-56`
                .min_w(0.0)
                .radius(k.radius.sm) // `rounded`
                .border(1.0, k.c.border_base) // `border border-edge`
                .child(self.canvas_mount(dark, sc, &d.transform.data, true, mount, true))
                .child(
                    // React `absolute left-2 top-1 z-10 text-[11px] font-medium
                    // text-fg-muted`; z-10 is child order (the chip paints last).
                    text(axis.clone())
                        .key(format!("chart-view-tile-chip-{axis}"))
                        .font(k.ty.small_medium)
                        .color(k.c.text_muted)
                        .absolute(CHIP_X, CHIP_Y),
                );
            grid = grid.child(tile);
        }
        grid
    }

    /// One canvas under a mount wrapper whose key carries the palette epoch —
    /// the observable side of React's `key={paletteEpoch}` remount. The tile
    /// canvases are compact; the main and overlay canvases are not.
    fn canvas_mount(
        &self,
        dark: bool,
        config: &ChartConfig,
        data: &[ChartDataPoint],
        compact: bool,
        mount_key: String,
        with_click: bool,
    ) -> El {
        let mut canvas = ChartCanvas::<S>::new(config, data).dark(dark).compact(compact);
        if with_click {
            if let Some(on_click) = self.on_data_point_click.clone() {
                canvas = canvas.on_data_point_click(move |s: &mut S, i| on_click(s, i));
            }
        }
        if let Some(factory) = self.renderer_factory.clone() {
            canvas = canvas.renderer_factory(move |t| factory(t));
        }
        col()
            .key(mount_key)
            .w_full()
            .grow(1.0)
            .min_w(0.0)
            .min_h(0.0)
            .child(canvas.build())
    }

    /// The `ChartExpandOverlay` portal: the window-covering column (below the
    /// 40pt top band), its toolbar row and the configurator + canvas row.
    fn overlay(&self, k: &Kit, dark: bool, d: &Derived) -> El {
        let c = &k.c;
        let mut close = close_button(k);
        if let Some(on_close) = self.on_close.clone() {
            close = close.on_press(move |s: &mut S| on_close(s));
        }
        col()
            .key(OVERLAY_KEY)
            // React `fixed inset-0 top-10 z-50`: pinned to the window's edges
            // with the 40pt top inset; the layer's tail position is the z-50.
            .pin(Some(0.0), Some(OVERLAY_TOP), Some(0.0), Some(0.0))
            .fill(k.alpha(c.bg_base, OVERLAY_BG_ALPHA)) // `bg-surface/95`
            .child(
                row()
                    .key(OVERLAY_BAR_KEY)
                    .items_center()
                    .gap(BAR_GAP)
                    .border_sides([0.0, 0.0, 1.0, 0.0], c.border_base) // `border-b border-edge`
                    .px(BAR_PAD_X)
                    .py(BAR_PAD_Y)
                    .child(
                        text(t("chart.expandTitle"))
                            .key(OVERLAY_TITLE_KEY)
                            .font(k.ty.body_medium) // React `text-sm font-medium`
                            .color(c.text_strong), // React `text-fg`
                    )
                    .child(el().key(OVERLAY_SPACER_KEY).grow(1.0).min_w(0.0))
                    .child(self.export_button(k, OVERLAY_PNG_KEY, ExportFormat::Png))
                    .child(self.export_button(k, OVERLAY_SVG_KEY, ExportFormat::Svg))
                    .child(separator(c.border_base))
                    .child(close),
            )
            .child(
                row()
                    .key(OVERLAY_CONTENT_KEY)
                    .grow(1.0)
                    .min_w(0.0)
                    .min_h(0.0)
                    .child(self.axis_panel(k, d))
                    .child(
                        col()
                            .key(OVERLAY_AREA_KEY)
                            .grow(1.0)
                            .min_w(0.0)
                            .min_h(0.0)
                            .child(self.canvas_mount(
                                dark,
                                &d.render_config,
                                &d.transform.data,
                                false,
                                format!("chart-view-expand-canvas-{}", self.palette_epoch),
                                false, // the overlay canvas carries no click handler
                            )),
                    ),
            )
    }

    /// One overlay export button: React's `rounded px-2 py-1 text-xs
    /// text-fg-muted hover:text-fg-secondary hover:bg-surface-alt` text
    /// button with the literal PNG/SVG label.
    fn export_button(&self, k: &Kit, key: &str, format: ExportFormat) -> El {
        let c = &k.c;
        let label = format.menu_label();
        let mut button = row()
            .key(key)
            .role("button")
            .label(label)
            .help(label)
            .items_center()
            .px(BUTTON_PAD_X)
            .py(BUTTON_PAD_Y)
            .radius(k.radius.sm) // React `rounded`
            .focusable(true)
            .focus_ring(2.0, 2.0, c.border_focus)
            .hover_group()
            .hover(c.hover) // `hover:bg-surface-alt` (the kit's semantic hover)
            .child(
                text(label)
                    .key(format!("{key}-label"))
                    .font(k.ty.text_xsmall) // React `text-xs` = 12px
                    .color(c.text_muted) // React `text-fg-muted`
                    .group_color(c.text_weak), // `hover:text-fg-secondary`
            );
        if let Some(on_export) = self.on_export.clone() {
            button = button.on_press(move |s: &mut S| on_export(s, format));
        }
        button
    }
}

/// The per-build derivations React holds in memos.
struct Derived {
    fields: Vec<ChartField>,
    recommendation: Option<ChartRecommendation>,
    transform: TransformResult,
    render_config: ChartConfig,
    show_empty_canvas: bool,
    show_split: bool,
    split_configs: Vec<ChartConfig>,
}

/// React's `mx-1 h-4 w-px bg-edge` span: a 16pt hairline with 4pt margins.
fn separator(color: rune::Color) -> El {
    el()
        .key(OVERLAY_SEP_KEY)
        .ml(SEP_MX)
        .mr(SEP_MX)
        .h(SEP_H)
        .w(SEP_W)
        .fill(color)
        .a11y_hidden()
}

/// The overlay's close button: the toolbar's expand-button shape with the
/// Minimize2 glyph and the `chart.collapse` label. React always renders it;
/// the press wires only when the caller owns a close handler.
fn close_button(k: &Kit) -> El {
    let c = &k.c;
    let label = t("chart.collapse");
    row()
        .key(OVERLAY_CLOSE_KEY)
        .role("button")
        .label(label)
        .help(label)
        .items_center()
        .px(BUTTON_PAD_X)
        .py(BUTTON_PAD_Y)
        .radius(k.radius.sm) // React `rounded`
        .focusable(true)
        .focus_ring(2.0, 2.0, c.border_focus)
        .hover_group()
        .hover(c.hover) // `hover:bg-surface-alt`
        .child(
            icon(&ICON_MINIMIZE_2, ICON_SIZE)
                .key(format!("{OVERLAY_CLOSE_KEY}-icon"))
                .color(c.text_muted) // React `text-fg-muted`
                .group_color(c.text_weak), // `hover:text-fg-secondary`
        )
}
