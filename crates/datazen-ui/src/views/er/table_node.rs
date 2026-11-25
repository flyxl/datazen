//! Port of `packages/frontend/src/windows/connection/er/TableNode.tsx`.
//!
//! A fixed-width ER table node: header with a collapse chevron, name and column
//! count; one row per column with PK/FK badges; a collapsed footer; and the
//! connection handles xyflow uses to attach edges. React's @xyflow handles and
//! node selection/dragging are host concerns; Rune exposes [`TableNodeHostPort`]
//! callbacks instead, with no fake IPC or drag state in this view.
//!
//! Geometry mirrors `nodeMetrics.ts` exactly — the layout engine packs against
//! these numbers before the node renders, so they are pinned here and shared
//! with the canvas when it lands.
//!
//! ## Keys
//!
//! React's only `data-testid` is `er-table-node`, preserved here. Everything
//! else is a Rune observation handle named after the React element; handle keys
//! are the literal xyflow ids (`name:s-r`, `node:t-l`, …).

use std::rc::Rc;

use rune::{col, icon, row, text, El};
use rune_kit::Kit;

use crate::i18n::t;
use crate::icons::{ICON_CHEVRON_DOWN, ICON_CHEVRON_RIGHT};
use crate::ui::kit::kit;

pub const ROOT_KEY: &str = "er-table-node";

/// `nodeMetrics.ts` constants, owned by the T568 port (`node_metrics.rs`);
/// re-exported here so the T361 audits keep their import paths.
pub use super::node_metrics::{
    ER_AUTO_COLLAPSE_COLUMNS, ER_COLLAPSED_FOOTER_HEIGHT, ER_COLUMN_ROW_HEIGHT,
    ER_HEADER_HEIGHT, ER_NODE_BORDER, ER_NODE_WIDTH,
};

pub fn er_node_body_height(column_count: usize) -> f32 {
    super::node_metrics::er_node_body_height(column_count as i32)
}

pub fn er_column_row_center_y(index: usize) -> f32 {
    // React adds ER_NODE_BORDER here because its CSS border participates in
    // layout. Rune's border is drawn inside the box and adds no layout offset,
    // so applying that 1pt here would put handle dots one point too low.
    // (node_metrics::er_column_row_center_y is the contract-faithful variant
    // for the graph-side consumers; this is the rendering-side one.)
    ER_HEADER_HEIGHT + index as f32 * ER_COLUMN_ROW_HEIGHT + ER_COLUMN_ROW_HEIGHT / 2.0
}

pub fn er_node_height(column_count: usize, collapsed: bool) -> f32 {
    super::node_metrics::er_node_height(column_count as i32, collapsed)
}

/// React `TableNodeData['columns'][i]`.
#[allow(dead_code)] // view VM; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableColumnVm {
    pub name: String,
    pub type_name: String,
    pub is_pk: bool,
    pub is_fk: bool,
}

/// React `TableNodeData` minus the xyflow-injected fields.
#[allow(dead_code)] // view VM; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableNodeVm {
    pub table_name: String,
    pub columns: Vec<TableColumnVm>,
    /// Columns an edge touches; only these get four per-row handles.
    pub handle_columns: Vec<String>,
    pub highlighted: bool,
    pub dimmed: bool,
    pub collapsed: bool,
}

impl Default for TableNodeVm {
    fn default() -> Self {
        Self {
            table_name: String::new(),
            columns: Vec::new(),
            handle_columns: Vec::new(),
            highlighted: false,
            dimmed: false,
            collapsed: false,
        }
    }
}

/// Host ports for the interactions xyflow otherwise owns.
#[allow(dead_code)] // host port; consumer: the ER canvas host (not wired).
pub struct TableNodeHostPort<S> {
    pub on_toggle_collapse: Option<Rc<dyn Fn(&mut S, &str) + 'static>>,
    pub on_select: Option<Rc<dyn Fn(&mut S, &str) + 'static>>,
    pub on_drag: Option<Rc<dyn Fn(&mut S, &str) + 'static>>,
}

impl<S> Default for TableNodeHostPort<S> {
    fn default() -> Self {
        Self {
            on_toggle_collapse: None,
            on_select: None,
            on_drag: None,
        }
    }
}

fn dot(key: &str, size: f32, k: &Kit) -> El {
    row()
        .key(key)
        .w(size)
        .h(size)
        .radius(size / 2.0)
        .fill(k.c.text_accent)
}

fn handle_ids(name: &str) -> [String; 4] {
    [
        format!("{name}:s-r"),
        format!("{name}:s-l"),
        format!("{name}:t-l"),
        format!("{name}:t-r"),
    ]
}

fn column_row(
    column: &TableColumnVm,
    index: usize,
    touched: bool,
    dark: bool,
) -> El {
    let k = kit(dark);
    let mut content = row()
        .key(format!("er-table-node-column-{index}-content"))
        .gap(8.0)
        .center_y()
        .w_full();
    if column.is_pk {
        content = content.child(
            text("PK")
                .key(format!("er-table-node-column-{index}-pk"))
                .font(k.ty.text_xsmall)
                .color(k.c.warning.fg),
        );
    }
    if column.is_fk {
        content = content.child(
            text("FK")
                .key(format!("er-table-node-column-{index}-fk"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_accent),
        );
    }
    content = content
        .child(
            text(column.name.clone())
                .key(format!("er-table-node-column-{index}-name"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_base)
                .wrap(),
        )
        .child(
            text(column.type_name.clone())
                .key(format!("er-table-node-column-{index}-type"))
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        );

    let mut line = row()
        .key(format!("er-table-node-column-{index}"))
        .w_full()
        .h(ER_COLUMN_ROW_HEIGHT)
        .center_y()
        .padding(8.0)
        .fill(if column.is_pk {
            k.alpha(k.c.warning.fg, 0.05)
        } else if column.is_fk {
            k.alpha(k.c.text_accent, 0.05)
        } else {
            k.c.surface_base
        });
    if touched {
        let ids = handle_ids(&column.name);
        // The four variants let an edge leave or enter on either side (a cycle
        // can point leftwards); all sit at this row's vertical centre.
        line = line.child(dot(&ids[1], 6.0, k)).child(dot(&ids[2], 6.0, k));
        line = line.child(content);
        line = line.child(dot(&ids[0], 6.0, k)).child(dot(&ids[3], 6.0, k));
    } else {
        line = line.child(content);
    }
    line
}

/// Build the node. Callbacks are injected by the host; no IPC, thread or drag
/// simulation happens here.
#[allow(dead_code)] // view entry point; consumer: the ER canvas host (not wired).
pub fn table_node<S: 'static>(
    vm: &TableNodeVm,
    dark: bool,
    host: TableNodeHostPort<S>,
) -> El {
    let k = kit(dark);
    let table_name = vm.table_name.clone();

    let mut header = row()
        .key("er-table-node-header")
        .w_full()
        .h(ER_HEADER_HEIGHT)
        .gap(4.0)
        .center_y()
        .padding(8.0)
        .fill(if vm.highlighted {
            k.alpha(k.c.text_accent, 0.15)
        } else {
            k.c.surface_weak
        });
    let mut toggle = row()
        .key("er-table-node-collapse")
        .role("button")
        .label(if vm.collapsed {
            t("erDiagram.expand").to_owned()
        } else {
            t("erDiagram.collapse").to_owned()
        })
        .w(16.0)
        .h(16.0)
        .center()
        .focusable(true)
        .opacity(0.6)
        .child(icon(
            if vm.collapsed { &ICON_CHEVRON_RIGHT } else { &ICON_CHEVRON_DOWN },
            12.0,
        )
        .color(if vm.highlighted { k.c.text_accent } else { k.c.text_muted }));
    if let Some(on_toggle_collapse) = host.on_toggle_collapse.clone() {
        let name = table_name.clone();
        toggle = toggle.on_press(move |s: &mut S| on_toggle_collapse(s, &name));
    }
    // Node-level fallbacks: a collapsed table has no rows, and a self-reference
    // has no left/right column to point at. The ids match xyflow exactly. The
    // wrapper is zero-height inside the fixed-height header, so these
    // absolutely-positioned handles add nothing to the node's packed height.
    let mut fallbacks = row().key("er-table-node-fallback-handles").h(0.0).gap(4.0);
    for id in ["node:t-l", "node:s-l", "node:s-r", "node:t-r"] {
        fallbacks = fallbacks.child(dot(id, 8.0, k));
    }

    header = header
        .child(toggle)
        .child(
            text(table_name.clone())
                .key("er-table-node-name")
                .font(k.ty.text_xsmall)
                .color(if vm.highlighted {
                    k.c.text_accent
                } else {
                    k.c.text_base
                })
                .wrap(),
        )
        .child(
            text(vm.columns.len().to_string())
                .key("er-table-node-count")
                .font(k.ty.text_xsmall)
                .color(k.c.text_muted),
        )
        .child(fallbacks);
    if let Some(on_select) = host.on_select.clone() {
        let name = table_name.clone();
        header = header.on_press(move |s: &mut S| on_select(s, &name));
    }
    if let Some(on_drag) = host.on_drag {
        let name = table_name;
        header = header.on_drag_start(move |s: &mut S, _tx: &mut rune::Tx| on_drag(s, &name));
    }

    let mut root = col()
        .key(ROOT_KEY)
        .w(ER_NODE_WIDTH)
        .min_w(ER_NODE_WIDTH)
        // The layout engine packs against this exact metric; pin the rendered
        // height to it instead of letting handle decorations grow the node.
        .h(er_node_height(vm.columns.len(), vm.collapsed))
        .radius(8.0)
        .border(ER_NODE_BORDER, if vm.highlighted { k.c.text_accent } else { k.c.border_base })
        .fill(if vm.highlighted {
            k.alpha(k.c.text_accent, 0.05)
        } else {
            k.c.surface_base
        })
        .opacity(if vm.dimmed { 0.3 } else { 1.0 })
        .child(header);

    if vm.collapsed {
        root = root.child(
            row()
                .key("er-table-node-footer")
                .w_full()
                .h(ER_COLLAPSED_FOOTER_HEIGHT)
                .center_y()
                .padding(8.0)
                .child(
                    text(format!("{} {}", vm.columns.len(), t("erDiagram.columns")))
                        .key("er-table-node-footer-text")
                        .font(k.ty.text_xsmall)
                        .color(k.c.text_muted),
                ),
        );
    } else {
        for (index, column) in vm.columns.iter().enumerate() {
            let touched = vm.handle_columns.iter().any(|name| name == &column.name);
            root = root.child(column_row(column, index, touched, dark));
        }
    }

    root
}

// Leftover L1 (removal stage): when the ER canvas view lands, drive collapse /
// selection / drag from its state slice with these host ports, import the node
// metrics from the shared layout module, and drop the per-item allows.
