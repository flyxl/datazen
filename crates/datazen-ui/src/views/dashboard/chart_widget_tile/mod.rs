//! Port of `packages/frontend/src/windows/dashboard/ChartWidgetTile.tsx` (T069).
//!
//! The dashboard tile: a fixed-height card (`h-80`) whose header strip carries
//! the widget title, the fired-alert glyph, the five-way chart-type switch,
//! the chart/table view switch and the refresh / history / expand / edit /
//! delete icon buttons, and whose body paints exactly one of its eight
//! mutually exclusive states (busy / no-run / error / timeout / empty chart /
//! chart / table / empty table). The expand button mounts the near-full-window
//! overlay (React's `TileExpandOverlay` through `createPortal`) with the axis
//! configurator and the large chart or table.
//!
//! ## What maps where
//!
//! - React's `widget` / `run` props are wire types; the tile reads only the
//!   fields below, so [`ChartWidgetVm`] / [`WidgetRunVm`] are the projections
//!   (the host converts, as `DashboardOptionVm` does in T018). Run cells reuse
//!   [`chart_field_inference::ChartValue`], the JS value space the inference
//!   pass already models.
//! - The `chartData` memo — `widgetRunToChartData(run, widget.chartConfig)`
//!   from `lib/dashboard/runToChart.ts` — is a **seam**: the transform
//!   pipeline (`runToChart.ts` → `lib/chart/transform.ts`, T030/T182) is not
//!   landed, so the host delivers the transformed points through
//!   [`ChartWidgetTile::chart_data`] and [`ChartWidgetTile::overlay_chart_data`]
//!   (the latter for the overlay's `localConfig`; while the local config
//!   still equals the widget's, the tile reuses the tile data, which is what
//!   React's recomputation yields). `hasRenderableChart` ports verbatim as
//!   [`has_renderable_chart`].
//! - The overlay's `fields` memo is real: `widgetRunToStatementResult`
//!   (`lib/dashboard/runToResult.ts`, 16 lines) ports as
//!   [`widget_run_to_chart_result`] and `inferAllFields` is the landed
//!   `support/chart_field_inference` port, so [`run_fields`] needs no host.
//! - `AxisConfigurator` (T319, not landed) is the second seam: the overlay
//!   takes a host-fed `El` slot ([`ChartWidgetTile::axis_configurator`]) and
//!   renders it under React's own gate (`viewMode === 'chart' && fields.length
//!   > 0`); with no slot supplied nothing paints there yet.
//! - The tile's two `useState` values — `expanded` and the overlay's
//!   `localConfig` — live in [`ChartWidgetTileState`]; `begin_expand` /
//!   `collapse` / `set_overlay_config` are the transitions (React's overlay
//!   `useState` re-initialises on every mount, so `begin_expand` re-seeds the
//!   config each time). The expand / collapse presses are host mutators
//!   ([`ChartWidgetTile::on_expand`] / `on_collapse`): React's internal
//!   `setExpanded` becomes a callback because the state lives in `S`.
//! - The table mode renders the shared keyed grid `ui/table.rs`, whose row
//!   height is already 28pt (React `rowHeight={28}`); the full `DataTable`
//!   (T165) is not landed, so cell text is the JS `String(value)` form
//!   (`js_string`) and `exportTableName` has no surface yet.
//! - `createPortal(document.body)` maps to the `layer` push every dialog-shaped
//!   port takes; `busy` swaps the refresh glyph for the shared `ui/spinner.rs`
//!   indicator and makes the control inert (React `disabled`).
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; the root merges the React pair
//! `data-testid="dashboard-tile"` + `data-widget-id={id}` into
//! `dashboard-tile-{id}` (Rune keys are single-valued; the suffix is the
//! widget id React carries beside the testid). Inside a tile the React
//! testids repeat per instance — the unique root keeps the subtrees distinct
//! (Rune identity is the key path).
//!
//! | part | key |
//!|------|-----|
//! | tile root | `dashboard-tile-{widget.id}` |
//! | header strip | `dashboard-tile-header` |
//! | title | `dashboard-tile-title` |
//! | fired-alert glyph | `dashboard-tile-alert` |
//! | chart-type switch | `dashboard-chart-type-toggle`, buttons `dashboard-chart-type-toggle-{bar,line,pie,scatter,area}` |
//! | view switch | `dashboard-view-toggle`, buttons `widget-view-chart` / `widget-view-table` |
//! | refresh / history / expand / edit / delete | `dashboard-tile-refresh` / `-history` / `-expand` / `-edit` / `-delete` |
//! | busy state | `dashboard-tile-running` (+ `-spinner`) |
//! | no-run state | `dashboard-tile-no-run` |
//! | error / timeout states | `dashboard-tile-error` / `dashboard-tile-timeout` |
//! | empty-chart state | `dashboard-tile-empty-chart` (+ `-text`, configure button `dashboard-tile-configure`) |
//! | chart body | `dashboard-tile-chart` (then T008's `chart-canvas` keys) |
//! | table body | `dashboard-tile-table`, grid `dashboard-tile-table-grid` (`-row-i`/`-cell-i-j` from `ui/table.rs`) |
//! | empty-table state | `dashboard-tile-empty-table` |
//! | overlay | `dashboard-tile-overlay`, `-header`, `-title`, `-collapse`, `-body`, `-content`, `-empty`, `-table-grid` |
//!
//! Known deltas (documented, not silent): `opacity-60` on a disabled widget is
//! `El::opacity(0.6)`; `bg-surface/95 backdrop-blur-sm` is a 95% kit base fill
//! (no blur primitive); the overlay's `fixed inset-0 top-10` is a pinned layer
//! element stretching from 40pt down; `overflow-hidden` is `El::clip_soft(0.0)`
//! (the `ui/table.rs` mapping); `data-no-drag` is the host grid's drag
//! concern; `capitalize` is dropped (the zh labels are single characters);
//! the delete button's `hover:text-red-300` keeps the static danger ink; the
//! chart body is T008's honest placeholder until the five renderers land.

//! The file is split at the 800-line ceiling: `header.rs` carries the header
//! strip half of the React component (title, alert glyph, the two switches,
//! the icon buttons) and `overlay.rs` carries React's `TileExpandOverlay`.

mod header;
mod overlay;

use std::rc::Rc;

use rune::{col, icon, row, text, Color, El};
use rune_kit::Kit;

use crate::components::chart::chart_canvas::{ChartCanvas, ChartConfig, ChartDataPoint, ChartType};
use crate::i18n::t;
use crate::icons::ICON_PENCIL;
use crate::support::chart_field_inference::{self as inference, ChartResult, ChartValue};
use crate::ui::kit::kit;
use crate::ui::palette;
use crate::ui::spinner::{Spinner, SpinnerSize};
use crate::ui::table::{Column as GridColumn, DataTable as GridTable};

// ---- Geometry (Tailwind, verbatim) ------------------------------------------

/// React `h-80`.
const TILE_H: f32 = 320.0;
/// React header `gap-1 px-2 py-1.5`.
const HEADER_GAP: f32 = 4.0;
const HEADER_PAD_X: f32 = 8.0;
const HEADER_PAD_Y: f32 = 6.0;
/// React `h-3.5 w-3.5` header glyphs.
const ICON_SIZE: f32 = 14.0;
/// React `h-3 w-3` type-switch glyphs.
const TYPE_ICON_SIZE: f32 = 12.0;
/// React type switch `gap-0.5` / `p-0.5` (container and buttons alike).
const TYPE_GAP: f32 = 2.0;
const TYPE_PAD: f32 = 2.0;
/// React view switch `px-1.5 py-0.5`.
const VIEW_PX: f32 = 6.0;
const VIEW_PY: f32 = 2.0;
/// React icon buttons `h-6 w-6 px-0`.
const BUTTON_SIDE: f32 = 24.0;
/// React empty states `gap-2` / `px-3`.
const EMPTY_GAP: f32 = 8.0;
const EMPTY_PX: f32 = 12.0;
/// React configure button `h-7 gap-1 px-2`, icon `h-3 w-3`.
const CONFIGURE_H: f32 = 28.0;
const CONFIGURE_GAP: f32 = 4.0;
const CONFIGURE_PX: f32 = 8.0;
const CONFIGURE_ICON: f32 = 12.0;
/// React overlay `top-10` (the band below the title strip).
const OVERLAY_TOP: f32 = 40.0;
/// React `bg-surface/95`.
const OVERLAY_ALPHA: f32 = 0.95;
/// React overlay header `gap-2 px-4 py-2`.
const OVERLAY_HEADER_GAP: f32 = 8.0;
const OVERLAY_HEADER_PX: f32 = 16.0;
const OVERLAY_HEADER_PY: f32 = 8.0;

// ---- Keys -------------------------------------------------------------------

/// The tile root: React's `data-testid="dashboard-tile"` merged with its
/// `data-widget-id`.
pub(crate) fn tile_root_key(id: &str) -> String {
    format!("dashboard-tile-{id}")
}

/// The chart-type switch container (React testid).
const TYPE_TOGGLE_KEY: &str = "dashboard-chart-type-toggle";
/// The view switch container (React testid).
const VIEW_TOGGLE_KEY: &str = "dashboard-view-toggle";
/// The view switch's chart button (React testid).
const VIEW_CHART_KEY: &str = "widget-view-chart";
/// The view switch's table button (React testid).
const VIEW_TABLE_KEY: &str = "widget-view-table";
/// Refresh (React testid).
const REFRESH_KEY: &str = "dashboard-tile-refresh";
/// History (React testid).
const HISTORY_KEY: &str = "dashboard-tile-history";
/// Expand (React testid).
const EXPAND_KEY: &str = "dashboard-tile-expand";
/// Edit (React testid).
const EDIT_KEY: &str = "dashboard-tile-edit";
/// Delete (React testid).
const DELETE_KEY: &str = "dashboard-tile-delete";
/// The chart body wrapper (React testid).
const CHART_KEY: &str = "dashboard-tile-chart";
/// The table body wrapper (React testid).
const TABLE_KEY: &str = "dashboard-tile-table";
/// The expand overlay root (derived; React has no testid on it).
const OVERLAY_KEY: &str = "dashboard-tile-overlay";

// ---- View models ------------------------------------------------------------

/// React `ViewMode` (`'chart' | 'table'`).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TileViewMode {
    Chart,
    Table,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl TileViewMode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Chart => "chart",
            Self::Table => "table",
        }
    }

    /// Parse a wire `ViewMode`; unknown values fall back to React's default.
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "table" => Self::Table,
            _ => Self::Chart,
        }
    }
}

/// The slice of React's `DashboardWidget` the tile reads (the host converts
/// the wire type; `layout` / `refresh` / `alert` are the panel's business).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug)]
pub(crate) struct ChartWidgetVm {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) view_mode: TileViewMode,
    pub(crate) chart_config: Option<ChartConfig>,
    pub(crate) enabled: bool,
}

/// React `WidgetRunStatus`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WidgetRunStatus {
    Ok,
    Error,
    Timeout,
}

impl WidgetRunStatus {
    /// Parse a wire status; unknown values fall back to `error` so a malformed
    /// payload cannot paint a success state. (Also the run-history drawer's
    /// index-entry parser, T175.)
    pub(crate) fn from_str(value: &str) -> Self {
        match value {
            "ok" => Self::Ok,
            "timeout" => Self::Timeout,
            _ => Self::Error,
        }
    }
}

/// The slice of React's `WidgetRun` the tile reads. `rows` keeps the JS value
/// space ([`ChartValue`]) so the inference pass sees real cells; `row_count`
/// is the run's own count (it can exceed `rows.len()` when truncated).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug)]
pub(crate) struct WidgetRunVm {
    pub(crate) status: WidgetRunStatus,
    pub(crate) error: Option<String>,
    pub(crate) row_count: u64,
    pub(crate) columns: Vec<String>,
    pub(crate) rows: Vec<Vec<ChartValue>>,
    pub(crate) alert_fired: bool,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl WidgetRunVm {
    /// Build from the wire fields (`status` is the wire string).
    pub(crate) fn from_wire(
        status: &str,
        error: Option<String>,
        row_count: u64,
        columns: Vec<String>,
        rows: Vec<Vec<ChartValue>>,
        alert_fired: bool,
    ) -> Self {
        Self {
            status: WidgetRunStatus::from_str(status),
            error,
            row_count,
            columns,
            rows,
            alert_fired,
        }
    }
}

// ---- State ------------------------------------------------------------------

/// The tile's two `useState` values, kept in the owning state `S`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, Default)]
pub(crate) struct ChartWidgetTileState {
    /// React's `expanded` (the overlay mounts only while this and an ok run
    /// hold).
    pub(crate) expanded: bool,
    /// The overlay's `localConfig`, re-seeded on every expand exactly as
    /// React's `useState` initialiser re-runs per mount.
    pub(crate) local_config: ChartConfig,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl ChartWidgetTileState {
    /// The expand button: `setExpanded(true)` plus the overlay state's mount
    /// initialiser (`widget.chartConfig ?? DEFAULT_CHART_CONFIG`).
    pub(crate) fn begin_expand(&mut self, widget: &ChartWidgetVm) {
        self.expanded = true;
        self.local_config = widget.chart_config.clone().unwrap_or_default();
    }

    /// The overlay's collapse button: `setExpanded(false)`. The local config
    /// survives (the next expand re-seeds it, as React's fresh mount does).
    pub(crate) fn collapse(&mut self) {
        self.expanded = false;
    }

    /// The overlay's `setLocalConfig` half of `handleConfigChange` (the
    /// `onChartConfigChange` half is the host's callback, as in React).
    pub(crate) fn set_overlay_config(&mut self, config: ChartConfig) {
        self.local_config = config;
    }
}

// ---- Pure derivations -------------------------------------------------------

/// `hasRenderableChart` (`lib/dashboard/runToChart.ts`), verbatim:
/// `!!(chartData && chartData.data.length > 0 && chartConfig.yAxes.length > 0)`.
/// The seam hands over the points slice, so `chartData.data` is the slice.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) fn has_renderable_chart(data: Option<&[ChartDataPoint]>, config: &ChartConfig) -> bool {
    data.is_some_and(|points| !points.is_empty()) && !config.y_axes.is_empty()
}

/// `widgetRunToStatementResult` (`lib/dashboard/runToResult.ts`): the run
/// snapshot adapted for the chart pipeline — columns typed `unknown`
/// (`nullable`, which inference never reads), rows passed through. T030 will
/// promote this to the shared support module; until then the tile owns its
/// copy.
pub(crate) fn widget_run_to_chart_result(run: &WidgetRunVm) -> ChartResult {
    ChartResult {
        columns: run
            .columns
            .iter()
            .map(|name| inference::ChartColumn {
                name: name.clone(),
                data_type: "unknown".to_owned(),
            })
            .collect(),
        rows: run.rows.clone(),
    }
}

/// The overlay's `fields` memo: `inferAllFields(widgetRunToStatementResult(run))`.
/// React wraps it in `try { … } catch { [] }`; the port cannot throw, so the
/// catch arm is unreachable (and documented).
pub(crate) fn run_fields(run: &WidgetRunVm) -> Vec<inference::ChartField> {
    inference::infer_all_fields(&widget_run_to_chart_result(run))
}

/// The table mode's `tableColumns` memo: one `ColumnDef` per run column
/// (`{ id: name, name, type: 'unknown' }`).
fn table_columns(run: &WidgetRunVm) -> Vec<GridColumn> {
    run.columns.iter().map(|name| GridColumn::new(name.clone())).collect()
}

// ---- Builder ----------------------------------------------------------------

type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type ViewModeFn<S> = Rc<dyn Fn(&mut S, TileViewMode) + 'static>;
type ChartTypeFn<S> = Rc<dyn Fn(&mut S, ChartType) + 'static>;
type ChartConfigFn<S> = Rc<dyn Fn(&mut S, ChartConfig) + 'static>;

/// Builder for the tile, generic over the app state `S` that owns the tile
/// state slice (the ported-dialog pattern).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub(crate) struct ChartWidgetTile<'a, S: 'static> {
    widget: &'a ChartWidgetVm,
    state: &'a ChartWidgetTileState,
    run: Option<&'a WidgetRunVm>,
    busy: bool,
    /// The `chartData` memo's output for `widget.chartConfig` — the
    /// T030/T182 transform seam (host-fed).
    chart_data: Option<&'a [ChartDataPoint]>,
    /// The overlay memo's output for the local config; falls back to
    /// [`Self::chart_data`] while the local config equals the widget's.
    overlay_chart_data: Option<&'a [ChartDataPoint]>,
    /// The `AxisConfigurator` slot (T319 seam, host-fed).
    axis_configurator: Option<El>,
    on_edit: Option<ActionFn<S>>,
    on_delete: Option<ActionFn<S>>,
    on_history: Option<ActionFn<S>>,
    on_refresh: Option<ActionFn<S>>,
    /// React's internal `setExpanded(true)`: the host applies
    /// `ChartWidgetTileState::begin_expand`.
    on_expand: Option<ActionFn<S>>,
    /// React's internal `setExpanded(false)`: the host applies
    /// `ChartWidgetTileState::collapse`.
    on_collapse: Option<ActionFn<S>>,
    on_view_mode_change: Option<ViewModeFn<S>>,
    on_chart_type_change: Option<ChartTypeFn<S>>,
    /// React `onChartConfigChange` — the forwarding half of the overlay's
    /// `handleConfigChange`; the axis slot's host fires it.
    on_chart_config_change: Option<ChartConfigFn<S>>,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl<'a, S: 'static> ChartWidgetTile<'a, S> {
    pub(crate) fn new(widget: &'a ChartWidgetVm, state: &'a ChartWidgetTileState) -> Self {
        Self {
            widget,
            state,
            run: None,
            busy: false,
            chart_data: None,
            overlay_chart_data: None,
            axis_configurator: None,
            on_edit: None,
            on_delete: None,
            on_history: None,
            on_refresh: None,
            on_expand: None,
            on_collapse: None,
            on_view_mode_change: None,
            on_chart_type_change: None,
            on_chart_config_change: None,
        }
    }

    /// React's `run` prop (`null` = never run).
    pub(crate) fn run(mut self, run: Option<&'a WidgetRunVm>) -> Self {
        self.run = run;
        self
    }

    /// React's `busy` prop.
    pub(crate) fn busy(mut self, busy: bool) -> Self {
        self.busy = busy;
        self
    }

    /// The `chartData` memo for `widget.chartConfig` (the transform seam).
    pub(crate) fn chart_data(mut self, data: Option<&'a [ChartDataPoint]>) -> Self {
        self.chart_data = data;
        self
    }

    /// The overlay's `chartData` memo for the local config (the same seam).
    pub(crate) fn overlay_chart_data(mut self, data: Option<&'a [ChartDataPoint]>) -> Self {
        self.overlay_chart_data = data;
        self
    }

    /// The `AxisConfigurator` slot: a host-built element standing in for the
    /// T319 component.
    pub(crate) fn axis_configurator(mut self, slot: El) -> Self {
        self.axis_configurator = Some(slot);
        self
    }

    /// React `onEdit`.
    pub(crate) fn on_edit(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_edit = Some(Rc::new(f));
        self
    }

    /// React `onDelete` (the delete button only exists when wired).
    pub(crate) fn on_delete(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_delete = Some(Rc::new(f));
        self
    }

    /// React `onHistory`.
    pub(crate) fn on_history(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_history = Some(Rc::new(f));
        self
    }

    /// React `onRefresh`.
    pub(crate) fn on_refresh(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_refresh = Some(Rc::new(f));
        self
    }

    /// The expand button's `setExpanded(true)` (host-applied).
    pub(crate) fn on_expand(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_expand = Some(Rc::new(f));
        self
    }

    /// The overlay collapse button's `setExpanded(false)` (host-applied).
    pub(crate) fn on_collapse(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_collapse = Some(Rc::new(f));
        self
    }

    /// React `onViewModeChange` (the switch only exists when wired).
    pub(crate) fn on_view_mode_change(
        mut self,
        f: impl Fn(&mut S, TileViewMode) + 'static,
    ) -> Self {
        self.on_view_mode_change = Some(Rc::new(f));
        self
    }

    /// React `onChartTypeChange` (the switch only exists when wired).
    pub(crate) fn on_chart_type_change(mut self, f: impl Fn(&mut S, ChartType) + 'static) -> Self {
        self.on_chart_type_change = Some(Rc::new(f));
        self
    }

    /// React `onChartConfigChange`.
    pub(crate) fn on_chart_config_change(
        mut self,
        f: impl Fn(&mut S, ChartConfig) + 'static,
    ) -> Self {
        self.on_chart_config_change = Some(Rc::new(f));
        self
    }

    /// Builds the tile element; the expand overlay (when due) rides `layer`,
    /// the `createPortal` convention.
    pub(crate) fn build(self, dark: bool, layer: &mut Vec<El>) -> El {
        let k = kit(dark);
        let mut tile = col()
            .key(tile_root_key(&self.widget.id))
            .w_full()
            .h(TILE_H) // React h-80
            .radius(k.radius.lg) // rounded-lg
            .border(1.0, k.c.border_base) // border border-edge
            .fill(k.c.layer_02) // bg-surface-raised (wapp_card mapping)
            .clip_soft(0.0) // React overflow-hidden (ui/table mapping)
            .child(self.header(&k, dark))
            .child(self.body(&k, dark));
        if !self.widget.enabled {
            tile = tile.opacity(0.6); // React opacity-60
        }
        self.overlay(&k, dark, layer);
        tile
    }

    // ---- body ---------------------------------------------------------------

    /// The tile's `chartData` memo gates: the run exists, the view is chart
    /// and the widget carries a config (the transform's own ok/rowCount gates
    /// live behind the seam).
    fn tile_chart_data(&self) -> Option<&'a [ChartDataPoint]> {
        if self.run.is_some()
            && matches!(self.widget.view_mode, TileViewMode::Chart)
            && self.widget.chart_config.is_some()
        {
            self.chart_data
        } else {
            None
        }
    }

    fn body(&self, k: &Kit, dark: bool) -> El {
        let body = col().key("dashboard-tile-body").w_full().grow(1.0).min_h(0.0);
        let Some(run) = self.run else {
            // React's two no-run branches, gated on `busy`.
            if self.busy {
                return body.child(running_state(k, dark));
            }
            return body.child(centered_note(
                "dashboard-tile-no-run",
                k,
                t("dashboard.noRunYet"),
                k.c.text_muted,
            ));
        };
        match run.status {
            WidgetRunStatus::Error => body.child(centered_note(
                "dashboard-tile-error",
                k,
                run.error.as_deref().unwrap_or(t("dashboard.runError")),
                k.c.danger.fg, // React text-red-400
            )),
            WidgetRunStatus::Timeout => body.child(centered_note(
                "dashboard-tile-timeout",
                k,
                t("dashboard.runTimeout"),
                palette::timeout_amber(dark), // React text-amber-400
            )),
            WidgetRunStatus::Ok => match self.widget.view_mode {
                TileViewMode::Chart => {
                    // React's `hasChart && chartData && viewMode === 'chart'`:
                    // the config, the data and the predicate together.
                    let chart = match (self.widget.chart_config.as_ref(), self.tile_chart_data())
                    {
                        (Some(config), Some(data))
                            if has_renderable_chart(Some(data), config) =>
                        {
                            Some((config, data))
                        }
                        _ => None,
                    };
                    match chart {
                        Some((config, data)) => body.child(
                            col()
                                .key(CHART_KEY)
                                .w_full()
                                .grow(1.0)
                                .min_h(0.0)
                                .child(
                                    ChartCanvas::<S>::new(config, data)
                                        .dark(dark)
                                        .compact(true)
                                        .build(),
                                ),
                        ),
                        None => body.child(self.empty_chart_state(k, run)),
                    }
                }
                TileViewMode::Table => {
                    if run.rows.is_empty() {
                        body.child(centered_note(
                            "dashboard-tile-empty-table",
                            k,
                            t("dashboard.emptyResult"),
                            k.c.text_muted,
                        ))
                    } else {
                        body.child(
                            col()
                                .key(TABLE_KEY)
                                .w_full()
                                .grow(1.0)
                                .min_h(0.0)
                                .child(table_grid(run, TABLE_KEY, dark)),
                        )
                    }
                }
            },
        }
    }

    /// The ok-but-no-chart state: `rowCount === 0` says empty, anything else
    /// says the axes are not configured (with the configure shortcut).
    fn empty_chart_state(&self, k: &Kit, run: &WidgetRunVm) -> El {
        let mut state = col()
            .key("dashboard-tile-empty-chart")
            .w_full()
            .grow(1.0)
            .center()
            .gap(EMPTY_GAP)
            .px(EMPTY_PX)
            .child(
                text(if run.row_count == 0 {
                    t("dashboard.emptyResult")
                } else {
                    t("dashboard.chartNotConfigured")
                })
                .key("dashboard-tile-empty-chart-text")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted)
                .wrap(),
            );
        if run.row_count > 0 {
            state = state.child(self.configure_button(k));
        }
        state
    }

    /// The accent "edit widget" shortcut in the empty-chart state.
    fn configure_button(&self, k: &Kit) -> El {
        let label = t("dashboard.editWidget");
        let mut button = row()
            .key("dashboard-tile-configure")
            .role("button")
            .label(label)
            .help(label)
            .items_center()
            .gap(CONFIGURE_GAP)
            .h(CONFIGURE_H)
            .px(CONFIGURE_PX)
            .radius(k.radius.sm)
            .focusable(true)
            .focus_ring(2.0, 2.0, k.c.border_focus)
            .hover_group()
            .hover(k.c.hover)
            .child(
                icon(&ICON_PENCIL, CONFIGURE_ICON)
                    .key("dashboard-tile-configure-icon")
                    .color(k.c.text_accent),
            )
            .child(
                text(label)
                    .key("dashboard-tile-configure-label")
                    .font(k.ty.text_xsmall)
                    .color(k.c.text_accent),
            );
        if let Some(on_edit) = self.on_edit.as_ref() {
            let on_edit = Rc::clone(on_edit);
            button = button.on_press(move |s: &mut S| on_edit(s));
        }
        button
    }
}

// ---- shared pieces ----------------------------------------------------------

/// React's busy state: the large spinner plus `dashboard.running`.
fn running_state(k: &Kit, dark: bool) -> El {
    row()
        .key("dashboard-tile-running")
        .w_full()
        .grow(1.0)
        .center()
        .gap(6.0) // React mr-1.5
        .child(
            Spinner::new("dashboard-tile-running-spinner")
                .size(SpinnerSize::Lg)
                .build(dark),
        )
        .child(
            text(t("dashboard.running"))
                .key("dashboard-tile-running-text")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        )
}

/// React's `absolute inset-0 flex items-center justify-center px-3 text-center
/// text-xs` states (the body paints exactly one, so it fills instead). The
/// text child carries `{key}-text` so audits can read the message.
fn centered_note(key: &str, k: &Kit, message: &str, color: Color) -> El {
    col()
        .key(key)
        .w_full()
        .grow(1.0)
        .center()
        .px(EMPTY_PX)
        .child(
            text(message)
                .key(format!("{key}-text"))
                .font(k.ty.text_xsmall)
                .color(color)
                .wrap(),
        )
}

/// The table mode: the shared keyed grid over the run's columns and rows.
/// `prefix` keys the grid (`{prefix}-grid`) so the tile, the overlay and the
/// run-history drawer keep distinct subtrees.
pub(crate) fn table_grid(run: &WidgetRunVm, prefix: &str, dark: bool) -> El {
    let columns = table_columns(run);
    let rows: Vec<Vec<String>> = run
        .rows
        .iter()
        .map(|cells| cells.iter().map(inference::js_string).collect())
        .collect();
    GridTable::new(format!("{prefix}-grid"), &columns)
        .rows(rows)
        .grow()
        .build(dark)
}
