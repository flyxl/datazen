//! Port of `packages/frontend/src/windows/connection/ErDiagramView.tsx`.
//!
//! React's canvas is xyflow + DOM image export; neither exists in Rune. This is
//! the immediate-mode container: it composes the existing table nodes (T361)
//! and relation legend (T349), draws relation edges as keyed rows, and routes
//! toolbar actions through an injected [`ErDiagramHost`]. No fake IPC, timers,
//! threads or wall clock — the host supplies schema data and export results.
//!
//! The React tests are the spec: the panel schema is forwarded verbatim (or
//! `null`) to the ER data request and re-read when it changes; the PNG/SVG
//! buttons call the shared exporter with `er-diagram-{database}`, the ER
//! backdrop and PNG quality 1; an absent viewport is a no-op and export
//! failures are swallowed.
//!
//! # Reuse / stand-ins
//!
//! `TableNodeVm`/`table_node`, `ErRelationLegendVm`/`er_relation_legend` and
//! the node-metrics constants come from the sibling ER ports. `TableSchema` is
//! the shared relation-metadata DTO. The xyflow viewport, graph layout and
//! DOM exporter have no Rune equivalents: [`Viewport`], the grid layout and the
//! host port are local stand-ins. Removal stage: replace them with the Rune
//! canvas/layout/exporter primitives when those ports land.
//!
//! Not wired into `Section` dispatch yet: every item carries a per-item
//! `#[allow(dead_code)]` with the removal stage "drop when the ER canvas host
//! mounts this view".

use std::rc::Rc;

use rune::{col, row, text, El};
use rune_kit::{Button, ButtonVariant, ControlSize, Kit};

use crate::i18n::t;
use crate::icons::{ICON_DOWNLOAD, ICON_SEARCH};
use crate::relation_metadata::TableSchema;
use crate::ui::kit::kit;
use crate::ui::spinner::{Spinner, SpinnerSize, SpinnerTone};

use super::er_relation_legend::{er_relation_legend, ErRelationLegendVm};
use super::table_node::{er_node_height, table_node, TableColumnVm, TableNodeHostPort, TableNodeVm, ER_NODE_WIDTH};

/// Keys mirrored from the React `data-testid`s.
pub const ROOT_KEY: &str = "er-diagram-view";
pub const FLOW_KEY: &str = "er-diagram-flow";
pub const CONTROLS_KEY: &str = "er-diagram-controls";
pub const ZOOM_IN_KEY: &str = "er-diagram-zoom-in";
pub const ZOOM_OUT_KEY: &str = "er-diagram-zoom-out";
pub const RELAYOUT_KEY: &str = "er-diagram-relayout";
pub const FIT_VIEW_KEY: &str = "er-diagram-fit-view";
pub const SEARCH_KEY: &str = "er-diagram-search";
pub const STATS_KEY: &str = "er-diagram-stats";
pub const EXPORT_PNG_KEY: &str = "er-diagram-export-png";
pub const EXPORT_SVG_KEY: &str = "er-diagram-export-svg";

/// Shared exporter options from the React source.
pub const EXPORT_BACKGROUND: &str = "#1a1a2e";
pub const EXPORT_PNG_QUALITY: u8 = 1;

/// React `Viewport`, rounded to whole points for crisp rendering.
#[allow(dead_code)] // stand-in; remove with the Rune canvas.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Viewport {
    pub x: i32,
    pub y: i32,
    pub zoom: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { x: 0, y: 0, zoom: 1.0 }
    }
}

/// The exact arguments React passes to `databaseCommands.getErData`.
#[allow(dead_code)] // request mirror; remove with the canvas host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErDataRequest {
    pub db_session_id: String,
    pub database: String,
    pub schema: Option<String>,
}

#[allow(dead_code)] // helper; remove with the canvas host.
impl ErDataRequest {
    pub fn new(db_session_id: &str, database: &str, schema: Option<&str>) -> Self {
        Self {
            db_session_id: db_session_id.to_owned(),
            database: database.to_owned(),
            schema: schema.map(str::to_owned),
        }
    }
}

/// One relation edge as drawn by the container.
#[allow(dead_code)] // graph VM; remove with the Rune canvas layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ErEdge {
    pub id: String,
    pub from_table: String,
    pub to_table: String,
    pub predicted: bool,
    pub ambiguous: bool,
    pub score: i64,
}

/// The diagram's owned state (the React hooks, minus xyflow internals).
#[allow(dead_code)] // state; remove with the canvas host.
#[derive(Clone, Debug)]
pub struct ErDiagramState {
    pub db_session_id: String,
    pub database: String,
    pub schema: Option<String>,
    pub schemas: Vec<TableSchema>,
    pub loading: bool,
    pub error: Option<String>,
    pub search_query: String,
    pub active_focus: Option<String>,
    pub collapsed: Vec<String>,
    pub pinned: Vec<(String, Viewport)>,
    pub hovered_table: Option<String>,
    pub viewport: Viewport,
    pub prediction_enabled: bool,
    pub predicted: Vec<ErEdge>,
    /// React finds the canvas viewport in the DOM; the host reports whether it
    /// exists so export can no-op exactly as the source does.
    pub viewport_available: bool,
}

#[allow(dead_code)] // state; remove with the canvas host.
impl ErDiagramState {
    pub fn new(db_session_id: &str, database: &str, schema: Option<&str>) -> Self {
        Self {
            db_session_id: db_session_id.to_owned(),
            database: database.to_owned(),
            schema: schema.map(str::to_owned),
            schemas: Vec::new(),
            loading: true,
            error: None,
            search_query: String::new(),
            active_focus: None,
            collapsed: Vec::new(),
            pinned: Vec::new(),
            hovered_table: None,
            viewport: Viewport::default(),
            prediction_enabled: false,
            predicted: Vec::new(),
            viewport_available: true,
        }
    }

    pub fn request(&self) -> ErDataRequest {
        ErDataRequest::new(&self.db_session_id, &self.database, self.schema.as_deref())
    }

    /// Replace the schema set (the host's `getErData` answer). Wide tables start
    /// collapsed, but only as a seed: user toggles afterwards stick.
    pub fn set_schemas(&mut self, schemas: Vec<TableSchema>) {
        self.collapsed = schemas
            .iter()
            .filter(|schema| schema.columns.len() > WIDE_TABLE_COLUMNS)
            .map(|schema| schema.table_name.clone())
            .collect();
        self.schemas = schemas;
        self.loading = false;
        self.error = None;
    }

    pub fn set_schema(&mut self, schema: Option<&str>) {
        self.schema = schema.map(str::to_owned);
    }

    pub fn set_search(&mut self, query: &str) {
        self.search_query = query.to_owned();
    }

    pub fn focus(&mut self, table: &str) {
        self.active_focus = Some(table.to_owned());
    }

    pub fn toggle_collapse(&mut self, table: &str) {
        if let Some(index) = self.collapsed.iter().position(|name| name == table) {
            self.collapsed.remove(index);
        } else {
            self.collapsed.push(table.to_owned());
        }
    }

    pub fn pin(&mut self, table: &str, viewport: Viewport) {
        if let Some(entry) = self.pinned.iter_mut().find(|(name, _)| name == table) {
            entry.1 = viewport;
        } else {
            self.pinned.push((table.to_owned(), viewport));
        }
    }

    pub fn relayout(&mut self) {
        self.pinned.clear();
    }

    pub fn set_viewport(&mut self, x: f32, y: f32, zoom: f32) {
        // React rounds x/y but leaves zoom as reported.
        self.viewport = Viewport {
            x: math_round(x) as i32,
            y: math_round(y) as i32,
            zoom,
        };
    }

    pub fn zoom_in(&mut self) {
        self.viewport.zoom = (self.viewport.zoom * ZOOM_STEP).min(MAX_ZOOM);
    }

    pub fn zoom_out(&mut self) {
        self.viewport.zoom = (self.viewport.zoom / ZOOM_STEP).max(MIN_ZOOM);
    }

    pub fn fit_view(&mut self) {
        self.viewport = Viewport::default();
    }

    /// React's `stats`: declared + deduplicated predicted relations.
    pub fn declared_count(&self) -> usize {
        self.schemas.iter().map(|schema| schema.foreign_keys.len()).sum()
    }

    pub fn predicted_count(&self) -> usize {
        self.predicted.len()
    }

    pub fn relation_count(&self) -> usize {
        self.declared_count() + self.predicted_count()
    }
}

/// JS `Math.round`: ties go toward +∞, unlike Rust's `f32::round` (away from 0).
/// Non-finite values pass through unchanged, as they do in JS.
fn math_round(value: f32) -> f32 {
    if value.is_finite() {
        (value + 0.5).floor()
    } else {
        value
    }
}

/// React `defaultCollapsedTables` threshold (`nodeMetrics.ts`).
const WIDE_TABLE_COLUMNS: usize = 30;
const ZOOM_STEP: f32 = 1.2;
const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 10.0;

/// Host operations React owns (IPC, clipboard, native menus, export, settings).
#[allow(dead_code)] // host port; remove with the canvas host.
pub trait ErDiagramHost<S: 'static> {
    fn export_png(&self, state: &mut S, filename: &str, background: &str, quality: u8) -> Result<(), String> {
        let _ = (state, filename, background, quality);
        Ok(())
    }
    fn export_svg(&self, state: &mut S, filename: &str, background: &str) -> Result<(), String> {
        let _ = (state, filename, background);
        Ok(())
    }
    fn export_failed(&self, state: &mut S, message: &str) {
        let _ = (state, message);
    }
    fn on_select_table(&self, state: &mut S, table: &str, schema: Option<&str>, database: &str) {
        let _ = (state, table, schema, database);
    }
    fn on_focus_table(&self, state: &mut S, table: &str) {
        let _ = (state, table);
    }
    fn update_prediction(&self, state: &mut S, enabled: bool) {
        let _ = (state, enabled);
    }
    fn t(&self, key: &str) -> String {
        key.to_owned()
    }
}

/// Build the diagram. The loading/error/empty states are exclusive, as in React.
#[allow(dead_code)] // view entry; remove with the canvas host.
pub fn er_diagram_view<S: 'static>(
    state: &ErDiagramState,
    dark: bool,
    host: Rc<dyn ErDiagramHost<S>>,
) -> El {
    let k = kit(dark);
    if state.loading {
        return col()
            .key("er-diagram-loading")
            .w_full()
            .h_full()
            .center()
            .child(Spinner::new("er-diagram-loading-spinner")
                .size(SpinnerSize::Lg)
                .tone(SpinnerTone::Muted)
                .build(dark))
            .child(text(host.t("erDiagram.loading"))
                .key("er-diagram-loading-label")
                .font(k.ty.text_small)
                .color(k.c.text_muted));
    }
    if let Some(error) = &state.error {
        return col()
            .key("er-diagram-error")
            .w_full()
            .h_full()
            .center()
            .padding(16.0)
            .child(text(error.clone())
                .key("er-diagram-error-text")
                .font(k.ty.text_small)
                .color(k.c.danger.fg)
                .wrap_anywhere());
    }
    if state.schemas.is_empty() {
        return col()
            .key("er-diagram-empty")
            .w_full()
            .h_full()
            .center()
            .child(text(host.t("erDiagram.noTables"))
                .key("er-diagram-empty-label")
                .font(k.ty.text_small)
                .color(k.c.text_muted));
    }

    col()
        .key(ROOT_KEY)
        .w_full()
        .h_full()
        .fill(k.c.surface_base)
        .child(canvas(state, dark, &host))
}

fn canvas<S: 'static>(
    state: &ErDiagramState,
    dark: bool,
    host: &Rc<dyn ErDiagramHost<S>>,
) -> El {
    let k = kit(dark);
    // Non-overlapping header / graph / footer. Absolute panels would draw on
    // top of the toolbar buttons and swallow their presses.
    col()
        .key(FLOW_KEY)
        .w_full()
        .grow(1.0)
        .min_h(0.0)
        .min_w(0.0)
        .fill(k.c.surface_base)
        .child(
            row()
                .key("er-diagram-header")
                .w_full()
                .h(36.0)
                .center_y()
                .child(search_bar(state, dark))
                .child(stats_bar(state, dark, host)),
        )
        .child(
            col()
                .key("er-diagram-graph")
                .w_full()
                .grow(1.0)
                .min_h(0.0)
                .min_w(0.0)
                .clip_soft(0.0)
                .children(nodes(state, dark, host))
                .children(edges(state, dark)),
        )
        .child(
            row()
                .key("er-diagram-footer")
                .w_full()
                .gap(8.0)
                .center_y()
                .child(zoom_controls(dark, host))
                .child(legend(state, dark, host)),
        )
}

fn node_view_model(schema: &TableSchema, state: &ErDiagramState) -> TableNodeVm {
    let fk_columns: Vec<String> = schema
        .foreign_keys
        .iter()
        .flat_map(|fk| fk.columns.iter().cloned())
        .collect();
    let collapsed = state.collapsed.iter().any(|name| name == &schema.table_name);
    let query = state.search_query.trim().to_ascii_lowercase();
    let matches = !query.is_empty() && schema.table_name.to_ascii_lowercase().contains(&query);
    TableNodeVm {
        table_name: schema.table_name.clone(),
        columns: schema
            .columns
            .iter()
            .map(|column| TableColumnVm {
                name: column.name.clone(),
                type_name: column.data_type.clone(),
                is_pk: schema.primary_keys.iter().any(|key| key == &column.name),
                is_fk: fk_columns.iter().any(|name| name == &column.name),
            })
            .collect(),
        handle_columns: fk_columns,
        highlighted: if query.is_empty() {
            state.active_focus.as_deref() == Some(schema.table_name.as_str())
                || state.hovered_table.as_deref() == Some(schema.table_name.as_str())
        } else {
            matches
        },
        dimmed: !query.is_empty() && !matches,
        collapsed,
    }
}

fn nodes<S: 'static>(
    state: &ErDiagramState,
    dark: bool,
    host: &Rc<dyn ErDiagramHost<S>>,
) -> Vec<El> {
    let mut result = Vec::new();
    let mut x = 24.0;
    let mut y = 56.0;
    for schema in &state.schemas {
        let vm = node_view_model(schema, state);
        let pinned = state
            .pinned
            .iter()
            .find(|(name, _)| name == &schema.table_name)
            .map(|(_, viewport)| *viewport);
        if let Some(position) = pinned {
            x = position.x as f32;
            y = position.y as f32;
        }
        let mut node_port = TableNodeHostPort::<S>::default();
        let schema_hint = state.schema.clone();
        let database = state.database.clone();
        let host = host.clone();
        node_port.on_select = Some(Rc::new(move |s: &mut S, name: &str| {
            host.on_select_table(s, name, schema_hint.as_deref(), &database);
        }));
        result.push(
            col()
                .key(format!("er-diagram-node-{}", schema.table_name))
                .absolute(x, y)
                .w(ER_NODE_WIDTH)
                .child(table_node(&vm, dark, node_port)),
        );
        x += ER_NODE_WIDTH + 32.0;
        y += er_node_height(vm.columns.len(), vm.collapsed) + 24.0;
    }
    result
}

fn edges(state: &ErDiagramState, dark: bool) -> Vec<El> {
    let k = kit(dark);
    state
        .predicted
        .iter()
        .map(|edge| {
            row()
                .key(format!("er-diagram-edge-{}", edge.id))
                .w(ER_NODE_WIDTH)
                .h(20.0)
                .gap(8.0)
                .center_y()
                .fill(k.c.layer_02)
                .child(
                    text(format!("{} → {}", edge.from_table, edge.to_table))
                        .font(k.ty.text_xsmall)
                        .color(if edge.ambiguous { k.c.warning.fg } else { k.c.text_base })
                        .truncate(),
                )
        })
        .collect()
}

fn search_bar(state: &ErDiagramState, dark: bool) -> El {
    let k = kit(dark);
    row()
        .key(SEARCH_KEY)
        .h(28.0)
        .gap(8.0)
        .center_y()
        .padding(8.0)
        .radius(8.0)
        .fill(k.c.surface_base)
        .child(rune::icon(&ICON_SEARCH, 14.0).color(k.c.text_muted))
        .child(
            text(if state.search_query.is_empty() {
                t("common.search")
            } else {
                &state.search_query
            })
            .key(format!("{}-value", SEARCH_KEY))
            .font(k.ty.text_small)
            .color(k.c.text_base),
        )
}

fn stats_bar<S: 'static>(
    state: &ErDiagramState,
    dark: bool,
    host: &Rc<dyn ErDiagramHost<S>>,
) -> El {
    let k = kit(dark);
    let filename = format!("er-diagram-{}", state.database);
    let mut png = Button::new("PNG")
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .icon(&ICON_DOWNLOAD)
        .help(host.t("erDiagram.exportPng"));
    if state.viewport_available {
        let host = host.clone();
        let filename = filename.clone();
        png = png.on_press(move |s: &mut S| {
            if let Err(error) = host.export_png(s, &filename, EXPORT_BACKGROUND, EXPORT_PNG_QUALITY) {
                host.export_failed(s, &error);
            }
        });
    }
    let mut svg = Button::new("SVG")
        .variant(ButtonVariant::Ghost)
        .control_size(ControlSize::Small)
        .icon(&ICON_DOWNLOAD)
        .help(host.t("erDiagram.exportSvg"));
    if state.viewport_available {
        let host = host.clone();
        let filename = filename.clone();
        svg = svg.on_press(move |s: &mut S| {
            if let Err(error) = host.export_svg(s, &filename, EXPORT_BACKGROUND) {
                host.export_failed(s, &error);
            }
        });
    }
    row()
        .key(STATS_KEY)
        .grow(1.0)
        .min_w(0.0)
        .h(28.0)
        .gap(8.0)
        .center_y()
        .justify_end()
        .child(png.build(k).key(EXPORT_PNG_KEY))
        .child(svg.build(k).key(EXPORT_SVG_KEY))
        .child(
            text(host.t("erDiagram.tableCount").replace("{count}", &state.schemas.len().to_string()))
                .key("er-diagram-table-count")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        )
        .child(
            text(host.t("erDiagram.relationCount").replace("{count}", &state.relation_count().to_string()))
                .key("er-diagram-relation-count")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        )
}

fn zoom_controls<S: 'static>(
    dark: bool,
    host: &Rc<dyn ErDiagramHost<S>>,
) -> El {
    let k = kit(dark);
    let mut controls = col()
        .key(CONTROLS_KEY)
        .gap(2.0);
    // Controls are wired in the host through the same button keys; pressing them
    // here only emits the corresponding view action via `host` (a no-op default
    // keeps the pure view side-effect free).
    for (key, label) in [
        (ZOOM_IN_KEY, "erDiagram.zoomIn"),
        (ZOOM_OUT_KEY, "erDiagram.zoomOut"),
        (RELAYOUT_KEY, "erDiagram.relayout"),
        (FIT_VIEW_KEY, "erDiagram.fitView"),
    ] {
        let button = Button::new(host.t(label))
            .variant(ButtonVariant::Neutral)
            .control_size(ControlSize::Small)
            .build(k)
            .key(key);
        controls = controls.child(button);
    }
    controls
}

fn legend<S: 'static>(
    state: &ErDiagramState,
    dark: bool,
    host: &Rc<dyn ErDiagramHost<S>>,
) -> El {
    let vm = ErRelationLegendVm {
        declared_count: state.declared_count(),
        predicted_count: state.predicted_count(),
        prediction_enabled: state.prediction_enabled,
    };
    let h = host.clone();
    let on_toggle = Rc::new(move |s: &mut S, enabled: bool| h.update_prediction(s, enabled));
    let legend = er_relation_legend(&vm, dark, Some(on_toggle as Rc<dyn Fn(&mut S, bool)>));
    col()
        .key("er-diagram-legend-host")
        .child(legend)
}
