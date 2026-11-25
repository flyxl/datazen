//! Port of `packages/frontend/src/windows/dashboard/RunHistoryDrawer.tsx`
//! (T175).
//!
//! The run-history drawer: a right-anchored overlay (`fixed inset-0 z-50
//! flex justify-end` — scrim plus a `max-w-3xl` aside with a left edge rule)
//! the dashboard panel mounts over the grid. The left column lists the
//! widget's last [`HISTORY_LIMIT`] runs (the started-at line, the status line
//! in its status colour, the fired-alert glyph); selecting one loads the full
//! run, whose ok / error / timeout outcome paints in the right column
//! alongside the chart↔table view switch the tile also carries.
//!
//! ## What maps where
//!
//! - The two `dashboardCommands` IPC calls are host concerns, not Rune's:
//!   the host watches React's `[open, dashboardId, widget]` dep array through
//!   [`RunHistoryDrawerState::needs_resync`] (the widget's object identity
//!   reduces to its id pair, as T146 documents), applies
//!   [`RunHistoryDrawerState::begin_resync`], then fetches `listWidgetRuns(
//!   dashboardId, widget.id, HISTORY_LIMIT)` out-of-band and delivers the
//!   answer through `deliver_index` / `deliver_index_error`. The run fetch
//!   arms whenever the selection changes ([`RunHistoryDrawerState::select`],
//!   or the auto-selection inside `deliver_index`); the host serves
//!   [`RunHistoryDrawerState::run_fetch_due`] once per `run_epoch` and
//!   delivers through `deliver_run` / `deliver_run_error`.
//! - React's per-effect `cancelled` flags are the two epoch tokens
//!   (`index_epoch` / `run_epoch`, bumped by `begin_resync`, by
//!   `begin_run_fetch` and by `close`): a delivery carrying a superseded
//!   token — or one that lands after close — returns `false` and lands
//!   nothing, exactly as a cancelled promise's `setState` never runs.
//! - The seven `useState` values plus the open flag live in
//!   [`RunHistoryDrawerState`]. `begin_resync` is the index effect's
//!   synchronous half, including the run effect's null-selection arm (a
//!   fresh open always has no selection, so its `setLoadedRun(null)` is the
//!   reset). `select` collapses `handleSelect` and the run effect's fetch
//!   arm; it returns `false` for the already-selected entry because React's
//!   dep array does not change (no re-fetch). `loading_run` deliberately
//!   survives `begin_resync` — React's effect never clears it either, so a
//!   cancelled run fetch leaves the run spinner up until the next fetch
//!   settles, in both trees.
//! - React's `widget` / `RunIndexEntry` / `WidgetRun` wire types project to
//!   the T069 view models: [`ChartWidgetVm`] / [`RunIndexEntryVm`] /
//!   [`WidgetRunVm`] (the host converts, as in T018/T069).
//! - The `chartData` memo — `widgetRunToChartData(loadedRun, chartConfig)`
//!   from `lib/dashboard/runToChart.ts` — is the T030/T182 transform **seam**
//!   (as in T069): the host delivers the transformed points through
//!   [`RunHistoryDrawer::chart_data`], and `hasChart` is
//!   `chartConfig && hasRenderableChart(chartData, chartConfig)` over the
//!   tile's ported predicate.
//! - `viewMode` is state seeded from `widget.viewMode` on every open (React's
//!   effect line `setViewMode(widget.viewMode)`); the switch buttons are the
//!   host callback [`RunHistoryDrawer::on_view_mode_change`] (the
//!   `setViewMode` half). The switch's gate is `loadedRun?.status === 'ok'`
//!   alone — it stays mounted while the next run loads, exactly as React's.
//! - `onClose` is a callback; the scrim is inert (React's has no handler —
//!   only the header button closes; there is no Escape path either).
//! - The table mode reuses `chart_widget_tile::table_grid` (the shared
//!   keyed grid, already 28pt rows for React's `rowHeight={28}`); the full
//!   `DataTable` (T165) is not landed, so cell text is the JS `String(value)`
//!   form and `exportTableName` has no surface yet (T069's documented delta).
//! - The index-error block is the `ui/banner.rs` boxed banner (danger
//!   tokens) with React's `mx-4 mt-3` margins; `formatLastConnected` is
//!   `support::formatters::format_last_connected`; `capitalize` becomes the
//!   pre-capitalised status labels ("Ok" / "Error" / "Timeout"), since Rune
//!   has no text-transform; `shadow-xl` is `k.elevate(elevation.overlay)`,
//!   the dialog mapping.
//!
//! ## Keys
//!
//! React `data-testid` values stay verbatim; the rest derive from the
//! component (an entry's `{id}` is React's `key={entry.id}`):
//!
//! | part | key |
//! |------|-----|
//! | root (`data-testid`) | `run-history-drawer` |
//! | scrim / aside panel | `run-history-drawer-scrim` / `-panel` |
//! | header / title / widget title / close | `run-history-drawer-header` / `-title` / `-widget-title` / `-close` |
//! | index-error banner | `run-history-index-error` (+ `-text`, from `ui/banner.rs`) |
//! | body row / list column | `run-history-drawer-body` / `-list` |
//! | index loading / empty | `run-history-list-loading` (+ `-spinner`), `run-history-list-empty` (+ `-text`) |
//! | scroll list | `run-history-list-scroll` |
//! | an entry | `run-history-entry-{id}` (+ `-time`, `-status`, `-alert`) |
//! | view toggle (`data-testid`) | `run-history-view-toggle`, group `-group`, buttons `run-history-view-chart` / `run-history-view-table` |
//! | content / states column | `run-history-drawer-content` / `-states` |
//! | run-load error / select-run hint | `run-history-load-error` (+ `-text`), `run-history-select-run` (+ `-text`) |
//! | run loading | `run-history-loading` (+ `-spinner`, `-text`) |
//! | error / timeout states | `run-history-error-state` (+ `-text`), `run-history-timeout-state` (+ `-text`) |
//! | chart body | `run-history-chart` (then T008's `chart-canvas` keys) |
//! | table body | `run-history-table`, grid `run-history-table-grid` (`-row-i`/`-cell-i-j` from `ui/table.rs`) |
//! | empty-chart / empty-table states | `run-history-empty-chart` (+ `-text`), `run-history-empty-table` (+ `-text`) |
//!
//! Known deltas (documented, not silent): the drawer is two pushed layer
//! elements (inert scrim, then the `justify_end` root) — the `ui/dialog.rs`
//! convention for React's portal-less overlay; React's `absolute inset-0`
//! state overlays paint exclusively as filled states (every content branch
//! is `!loadingRun`-gated in React too, so the observable tree matches); the
//! boxed banner's uniform 8pt padding stands in for React's `px-3 py-2`
//! (a 4pt horizontal delta); `hover:bg-surface-raised` on unselected entries
//! is the hover fill with no transition counterpart; the aside's `w-full`
//! resolves against the layer row so `max-w-3xl` caps it at 768pt exactly as
//! the flex container does.

//! The file is split at the 800-line ceiling: `body.rs` carries the body row
//! half of the React component (the run-index column, the entry rows, the
//! view switch and the loaded run's states).

mod body;

use std::rc::Rc;

use rune::{col, el, row, text, Color, El, TextStyle};
use rune_kit::{ControlSize, IconButton, IconButtonVariant, Kit};

use crate::components::chart::chart_canvas::ChartDataPoint;
use crate::i18n::t;
use crate::icons::ICON_X;
use crate::ui::banner::{Banner, BannerVariant};
use crate::ui::kit::kit;
use crate::ui::palette;
use crate::views::dashboard::chart_widget_tile::{
    ChartWidgetVm, TileViewMode, WidgetRunStatus, WidgetRunVm,
};

// ---- Geometry (Tailwind, verbatim) ------------------------------------------

/// React's `HISTORY_LIMIT` — the `listWidgetRuns` page size the host passes.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub const HISTORY_LIMIT: usize = 50;
/// React `max-w-3xl` on the aside.
const DRAWER_MAX_W: f32 = 768.0;
/// React `bg-black/40` on the scrim.
const SCRIM_ALPHA: f32 = 0.40;
/// React header `px-4 py-3`.
const HEADER_PX: f32 = 16.0;
const HEADER_PY: f32 = 12.0;
/// React index-error `mx-4 mt-3`.
const INDEX_ERROR_MX: f32 = 16.0;
const INDEX_ERROR_MT: f32 = 12.0;
/// React list column `w-56`.
const LIST_W: f32 = 224.0;
/// React `ul` `py-1`.
const LIST_PY: f32 = 4.0;
/// React entry `gap-1.5 px-3 py-2`.
const ENTRY_GAP: f32 = 6.0;
const ENTRY_PX: f32 = 12.0;
const ENTRY_PY: f32 = 8.0;
/// React entry alert glyph `h-3.5 w-3.5 mt-0.5`.
const ALERT_ICON: f32 = 14.0;
const ALERT_MT: f32 = 2.0;
/// React toggle row `px-3 py-1.5`.
const TOGGLE_PX: f32 = 12.0;
const TOGGLE_PY: f32 = 6.0;
/// React toggle buttons `px-1.5 py-0.5`.
const VIEW_PX: f32 = 6.0;
const VIEW_PY: f32 = 2.0;
/// React centered states `px-4`.
const NOTE_PX: f32 = 16.0;
/// React spinner rows `mr-1.5`.
const SPINNER_GAP: f32 = 6.0;

// ---- Keys -------------------------------------------------------------------

/// The drawer root (React `data-testid="run-history-drawer"`).
const ROOT_KEY: &str = "run-history-drawer";
/// The view toggle row (React `data-testid="run-history-view-toggle"`).
const VIEW_TOGGLE_KEY: &str = "run-history-view-toggle";

// ---- View models ------------------------------------------------------------

/// The slice of React's `RunIndexEntry` the drawer reads (the host converts
/// the wire type; `started_at` stays the raw ISO text for the formatter).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunIndexEntryVm {
    pub id: String,
    pub started_at: String,
    pub status: WidgetRunStatus,
    pub alert_fired: bool,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl RunIndexEntryVm {
    /// Build from the wire fields (`status` is the wire string; unknown
    /// values fall back to `error`, the tile's stance, so a malformed
    /// payload cannot paint a success state).
    pub fn from_wire(id: &str, started_at: &str, status: &str, alert_fired: bool) -> Self {
        Self {
            id: id.to_owned(),
            started_at: started_at.to_owned(),
            status: WidgetRunStatus::from_str(status),
            alert_fired,
        }
    }
}

// ---- State ------------------------------------------------------------------

/// Every `useState` of the React component plus the open flag and the two
/// fetch tokens, kept in the owning state `S`.
#[allow(dead_code)] // reusable port; wired by the dashboard host track
#[derive(Clone, Debug)]
pub struct RunHistoryDrawerState {
    /// React's `open` prop; `false` builds nothing at all.
    pub open: bool,
    /// Bumped on each resync; the index fetch's cancel token.
    pub index_epoch: u64,
    /// Bumped on each run fetch (and by resync/close); the run fetch's
    /// cancel token.
    pub run_epoch: u64,
    /// The pair the current index epoch was issued for (React's dep array);
    /// the host compares through [`Self::needs_resync`].
    pub dashboard_id: String,
    pub widget_id: String,
    /// React's `index`.
    pub index: Vec<RunIndexEntryVm>,
    /// React's `selectedId` (`None` = nothing selected).
    pub selected_id: Option<String>,
    /// React's `loadedRun`.
    pub loaded_run: Option<WidgetRunVm>,
    pub loading_index: bool,
    pub loading_run: bool,
    pub index_error: Option<String>,
    pub run_error: Option<String>,
    /// React's `viewMode`, seeded from the widget on every open.
    pub view_mode: TileViewMode,
}

impl Default for RunHistoryDrawerState {
    fn default() -> Self {
        Self {
            open: false,
            index_epoch: 0,
            run_epoch: 0,
            dashboard_id: String::new(),
            widget_id: String::new(),
            index: Vec::new(),
            selected_id: None,
            loaded_run: None,
            loading_index: false,
            loading_run: false,
            index_error: None,
            run_error: None,
            // Unused before `begin_resync` seeds it; React's literal default
            // is the widget's view mode.
            view_mode: TileViewMode::Chart,
        }
    }
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl RunHistoryDrawerState {
    /// React's dep-array check for the index effect: `[open, dashboardId,
    /// widget]` changed — the drawer is closed, or the id pair differs (the
    /// empty default never matches a real pair, so the mount counts too).
    pub fn needs_resync(&self, dashboard_id: &str, widget_id: &str) -> bool {
        !self.open || self.dashboard_id != dashboard_id || self.widget_id != widget_id
    }

    /// The index effect's synchronous half (the drawer opening, or the
    /// widget changing while open): reset the selection-side state, seed the
    /// view mode, arm the index fetch and cancel anything in flight. The
    /// stale `index` and `loading_run` deliberately survive — React's effect
    /// clears neither (the loading gates hide both).
    pub fn begin_resync(&mut self, dashboard_id: &str, widget: &ChartWidgetVm) {
        self.open = true;
        self.dashboard_id = dashboard_id.to_owned();
        self.widget_id = widget.id.clone();
        self.index_epoch = self.index_epoch.saturating_add(1);
        self.run_epoch = self.run_epoch.saturating_add(1);
        self.selected_id = None;
        self.loaded_run = None;
        self.index_error = None;
        self.run_error = None;
        self.view_mode = widget.view_mode;
        self.loading_index = true;
    }

    /// The close path (React's `onClose` prop flipping `open` false): the
    /// drawer builds nothing, and both effects' cleanups cancel anything in
    /// flight. The remaining state survives exactly as React's does — the
    /// next open's effect resets it.
    pub fn close(&mut self) {
        self.open = false;
        self.index_epoch = self.index_epoch.saturating_add(1);
        self.run_epoch = self.run_epoch.saturating_add(1);
    }

    /// `handleSelect` plus the run effect's synchronous half
    /// (`setLoadingRun(true); setRunError(null)`). React's effect re-runs
    /// only when the id actually changes, so re-selecting the current entry
    /// returns `false` — nothing fetched, nothing re-armed. `loaded_run`
    /// survives a new fetch (React keeps the previous run mounted under the
    /// absolute loading overlay).
    pub fn select(&mut self, id: &str) -> bool {
        if self.selected_id.as_deref() == Some(id) {
            return false;
        }
        self.selected_id = Some(id.to_owned());
        self.begin_run_fetch();
        true
    }

    /// The run effect's fetch arm under a fresh cancel token.
    fn begin_run_fetch(&mut self) {
        self.loading_run = true;
        self.run_error = None;
        self.run_epoch = self.run_epoch.saturating_add(1);
    }

    /// The index fetch's `.then` + `.finally`: the list lands, the first
    /// entry auto-selects — arming the run fetch, exactly as React's effect
    /// cascade does — and the spinner stops. A stale token (or a closed
    /// drawer) lands nothing, which is the `cancelled` guard.
    pub fn deliver_index(&mut self, entries: Vec<RunIndexEntryVm>, token: u64) -> bool {
        if !self.open || token != self.index_epoch {
            return false;
        }
        self.index = entries;
        if let Some(first) = self.index.first() {
            let id = first.id.clone();
            self.select(&id);
        }
        self.loading_index = false;
        true
    }

    /// The index fetch's `.catch` + `.finally`: the error text lands and the
    /// spinner stops. The stale list survives (React's does).
    pub fn deliver_index_error(&mut self, reason: String, token: u64) -> bool {
        if !self.open || token != self.index_epoch {
            return false;
        }
        self.index_error = Some(reason);
        self.loading_index = false;
        true
    }

    /// The run fetch's `.then` + `.finally`: the run lands and the spinner
    /// stops.
    pub fn deliver_run(&mut self, run: WidgetRunVm, token: u64) -> bool {
        if !self.open || token != self.run_epoch {
            return false;
        }
        self.loaded_run = Some(run);
        self.loading_run = false;
        true
    }

    /// The run fetch's `.catch` + `.finally`: the run clears (so the view
    /// switch goes with it) and the error text lands.
    pub fn deliver_run_error(&mut self, reason: String, token: u64) -> bool {
        if !self.open || token != self.run_epoch {
            return false;
        }
        self.loaded_run = None;
        self.run_error = Some(reason);
        self.loading_run = false;
        true
    }

    /// The run fetch the state has armed: the run id the host should fetch,
    /// with the current `run_epoch` as the delivery token. `None` while
    /// nothing is due; the host serves each epoch once.
    pub fn run_fetch_due(&self) -> Option<&str> {
        if self.open && self.loading_run {
            self.selected_id.as_deref()
        } else {
            None
        }
    }

    /// The view switch's `setViewMode`.
    pub fn set_view_mode(&mut self, mode: TileViewMode) {
        self.view_mode = mode;
    }
}

// ---- Pure derivations -------------------------------------------------------

/// React's `statusClass` ink table: ok → `text-emerald-500`, error →
/// `text-red-400` (the T069 kit danger mapping), timeout → `text-amber-400`
/// (the tile's `timeout_amber`).
fn status_color(status: WidgetRunStatus, k: &Kit, dark: bool) -> Color {
    match status {
        WidgetRunStatus::Ok => palette::run_ok_emerald(dark),
        WidgetRunStatus::Error => k.c.danger.fg,
        WidgetRunStatus::Timeout => palette::timeout_amber(dark),
    }
}

/// React's `{entry.status}` under CSS `capitalize`: 'ok' → "Ok", 'error' →
/// "Error", 'timeout' → "Timeout" (Rune has no text-transform).
fn status_label(status: WidgetRunStatus) -> String {
    match status {
        WidgetRunStatus::Ok => "Ok".to_owned(),
        WidgetRunStatus::Error => "Error".to_owned(),
        WidgetRunStatus::Timeout => "Timeout".to_owned(),
    }
}

/// React's `text-xs` (12px / 400).
fn type_xs() -> TextStyle {
    TextStyle::new(12.0, 400.0)
}

// ---- Builder ----------------------------------------------------------------

type ActionFn<S> = Rc<dyn Fn(&mut S) + 'static>;
type SelectFn<S> = Rc<dyn Fn(&mut S, String) + 'static>;
type ViewModeFn<S> = Rc<dyn Fn(&mut S, TileViewMode) + 'static>;

/// Builder for the drawer, generic over the app state `S` that owns the
/// state slice (the ported-dialog pattern of T018/T069/T146).
#[allow(dead_code)] // reusable port; wired by the dashboard host track
pub struct RunHistoryDrawer<'a, S: 'static> {
    state: &'a RunHistoryDrawerState,
    widget: &'a ChartWidgetVm,
    /// The `chartData` memo's output for `(loadedRun, widget.chartConfig)` —
    /// the T030/T182 transform seam (host-fed).
    chart_data: Option<&'a [ChartDataPoint]>,
    on_close: Option<ActionFn<S>>,
    on_select: Option<SelectFn<S>>,
    on_view_mode_change: Option<ViewModeFn<S>>,
}

#[allow(dead_code)] // reusable port; wired by the dashboard host track
impl<'a, S: 'static> RunHistoryDrawer<'a, S> {
    pub fn new(state: &'a RunHistoryDrawerState, widget: &'a ChartWidgetVm) -> Self {
        Self {
            state,
            widget,
            chart_data: None,
            on_close: None,
            on_select: None,
            on_view_mode_change: None,
        }
    }

    /// The `chartData` memo for `widget.chartConfig` over the loaded run
    /// (the transform seam; `None` while no run is loaded or the widget
    /// carries no config, which is React's memo returning null).
    pub fn chart_data(mut self, data: Option<&'a [ChartDataPoint]>) -> Self {
        self.chart_data = data;
        self
    }

    /// React `onClose` (the header button; React's scrim has no handler).
    pub fn on_close(mut self, f: impl Fn(&mut S) + 'static) -> Self {
        self.on_close = Some(Rc::new(f));
        self
    }

    /// An index entry pressed — React's `handleSelect(entry.id)`. The host
    /// applies [`RunHistoryDrawerState::select`] (which arms the run fetch
    /// when the id changed) and dispatches `getWidgetRun` for the due epoch.
    pub fn on_select(mut self, f: impl Fn(&mut S, String) + 'static) -> Self {
        self.on_select = Some(Rc::new(f));
        self
    }

    /// The view switch's `setViewMode` half (host-applied).
    pub fn on_view_mode_change(
        mut self,
        f: impl Fn(&mut S, TileViewMode) + 'static,
    ) -> Self {
        self.on_view_mode_change = Some(Rc::new(f));
        self
    }

    /// Paints the drawer into `layer`; nothing at all while closed (React's
    /// `if (!open || !widget) return null`).
    pub fn build(self, dark: bool, layer: &mut Vec<El>) {
        if !self.state.open {
            return;
        }
        let k = kit(dark);
        // React's scrim: `absolute inset-0 bg-black/40`, `aria-hidden`, and
        // inert — only the header button closes the drawer.
        layer.push(
            el()
                .key(format!("{ROOT_KEY}-scrim"))
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .fill(Color::BLACK.alpha(SCRIM_ALPHA))
                .a11y_hidden(),
        );
        // React's root: `fixed inset-0 z-50 flex justify-end`; the aside is
        // the in-flow child the flex end-aligns.
        layer.push(
            row()
                .key(ROOT_KEY)
                .absolute(0.0, 0.0)
                .w_full()
                .h_full()
                .justify_end()
                .child(self.aside(&k, dark)),
        );
    }

    // ---- the aside ------------------------------------------------------------

    /// React's `<aside>`: `relative flex h-full w-full max-w-3xl flex-col
    /// border-l border-edge bg-surface shadow-xl`.
    fn aside(&self, k: &Kit, dark: bool) -> El {
        let mut aside = col()
            .key(format!("{ROOT_KEY}-panel"))
            .h_full()
            .w_full()
            .max_w(DRAWER_MAX_W) // React max-w-3xl
            .border_sides([0.0, 0.0, 0.0, 1.0], k.c.border_base) // border-l
            .fill(k.c.bg_base) // bg-surface
            .child(self.header(k));
        if let Some(banner) = self.index_error_banner(k) {
            aside = aside.child(banner);
        }
        // React's `shadow-xl`; the dialog's elevation mapping.
        let aside = k.elevate(aside, &k.elevation.overlay);
        aside.child(self.body_row(k, dark))
    }

    /// The header strip: `flex shrink-0 items-center justify-between
    /// border-b border-edge px-4 py-3` with the title block and the ghost
    /// close button.
    fn header(&self, k: &Kit) -> El {
        let mut close = IconButton::new(&ICON_X)
            .variant(IconButtonVariant::Ghost)
            .control_size(ControlSize::Large) // React h-7 w-7, glyph h-4 w-4
            .label(t("common.close"));
        if let Some(on_close) = self.on_close.as_ref() {
            let on_close = Rc::clone(on_close);
            close = close.on_press(move |s: &mut S| on_close(s));
        }
        let close = close.build(k).key(format!("{ROOT_KEY}-close"));

        row()
            .key(format!("{ROOT_KEY}-header"))
            .w_full()
            .items_center()
            .justify_between()
            .border_sides([0.0, 0.0, 1.0, 0.0], k.c.border_base) // border-b
            .px(HEADER_PX)
            .py(HEADER_PY)
            .child(
                col()
                    .key(format!("{ROOT_KEY}-header-titles"))
                    .min_w(0.0) // React min-w-0
                    .child(
                        text(t("dashboard.runHistory"))
                            .key(format!("{ROOT_KEY}-title"))
                            .font(TextStyle::new(14.0, 600.0)) // text-sm font-semibold
                            .color(k.c.text_base), // text-fg
                    )
                    .child(
                        text(self.widget.title.clone())
                            .key(format!("{ROOT_KEY}-widget-title"))
                            .font(type_xs()) // text-xs
                            .color(k.c.text_muted) // text-fg-muted
                            .truncate(),
                    ),
            )
            .child(close)
    }

    /// React's index-error block: `mx-4 mt-3 rounded-md border
    /// border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-400` — the
    /// boxed danger banner (an empty message builds nothing, as React's
    /// falsy `indexError && …` renders nothing).
    fn index_error_banner(&self, k: &Kit) -> Option<El> {
        let error = self.state.index_error.clone()?;
        Banner::new(format!("{ROOT_KEY}-index-error"), error)
            .variant(BannerVariant::Boxed)
            .build(k)
            .map(|banner| banner.mx(INDEX_ERROR_MX).mt(INDEX_ERROR_MT))
    }
}
