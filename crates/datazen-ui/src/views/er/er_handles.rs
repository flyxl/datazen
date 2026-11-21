//! The handle-routing half of `buildErGraph.ts` (T562 split).
//!
//! `TableNode` renders connection points only for the columns an edge
//! actually touches — a wide table would otherwise carry four handles per
//! row, and a schema of a few hundred tables would carry tens of thousands
//! of them. This module records those columns per node and points each edge
//! at the rows it joins.
//!
//! Which side an edge leaves and enters depends only on which node is
//! further right: every edge after a layered `LR` layout runs between two
//! different ranks, so it is horizontal — measured across acyclic, chained
//! and cyclic shapes, none was vertical. A reversed edge (a cycle) simply
//! mirrors the pair. A collapsed node renders no rows, and a self-reference
//! has no left/right to speak of, so both fall back to the node-level
//! handles and let the canvas draw the loop.
//!
//! Split from the builder (the React file exports both families to both
//! consumers) because the view-side canvas host needs the same routing when
//! it re-derives handles after a collapse or a pin — keeping it separately
//! testable is the point of the shard's split mark.

use std::collections::HashMap;

use crate::views::er::edge_labels::fit_edge_label;
use crate::views::er::graph::{ErEdge, ErNode};

/// Handle id for a column, or the node-level fallback when there is no row
/// to point at.
///
/// `column` empty or `collapsed` → `node:{role}-{side}`; otherwise
/// `{column}:{role}-{side}` — the literal xyflow ids `TableNode` renders.
fn handle_id(column: Option<&str>, role: &str, side: &str, collapsed: bool) -> String {
    match column {
        Some(name) if !name.is_empty() && !collapsed => format!("{name}:{role}-{side}"),
        _ => format!("node:{role}-{side}"),
    }
}

/// Record, per node, which columns an edge actually touches.
///
/// The list is insertion-ordered and deduplicated — the same order a JS
/// `Set` spreads in — so the handle keys are stable for a given edge list.
pub(crate) fn with_handle_columns(nodes: Vec<ErNode>, edges: &[ErEdge]) -> Vec<ErNode> {
    let mut touched: HashMap<String, Vec<String>> = HashMap::new();
    let mut touch = |table: &str, column: &Option<String>| {
        let Some(column) = column else { return };
        if column.is_empty() {
            return;
        }
        let set = touched.entry(table.to_owned()).or_default();
        if !set.iter().any(|name| name == column) {
            set.push(column.clone());
        }
    };

    for edge in edges {
        touch(&edge.source, &edge.data.source_column);
        touch(&edge.target, &edge.data.target_column);
    }

    nodes
        .into_iter()
        .map(|mut node| {
            node.data.handle_columns = touched.remove(&node.id).unwrap_or_default();
            node
        })
        .collect()
}

/// Point each edge at the rows it actually joins, and fit its label to the
/// gap it has to sit in.
///
/// Edges whose endpoints are not on the canvas (a focus filter raced a
/// relayout) keep whatever they had rather than being dropped. An ambiguous
/// inference carries no label at all: a clique of identical labels is
/// precisely what made this diagram unreadable.
pub(crate) fn with_handles(mut edges: Vec<ErEdge>, nodes: &[ErNode]) -> Vec<ErEdge> {
    let by_id: HashMap<&str, &ErNode> =
        nodes.iter().map(|node| (node.id.as_str(), node)).collect();

    for edge in &mut edges {
        let (Some(source), Some(target)) = (by_id.get(edge.source.as_str()), by_id.get(edge.target.as_str()))
        else {
            continue;
        };

        // A self-reference has no left/right to speak of; leave it on the
        // node-level handles and let the canvas draw the loop.
        if edge.source == edge.target {
            edge.source_handle = Some(handle_id(
                edge.data.source_column.as_deref(),
                "s",
                "r",
                true,
            ));
            edge.target_handle = Some(handle_id(
                edge.data.target_column.as_deref(),
                "t",
                "l",
                true,
            ));
            continue;
        }

        let source_cx = source.position.x + source.width / 2.0;
        let target_cx = target.position.x + target.width / 2.0;
        let forward = target_cx >= source_cx;
        // The label sits between the two nodes, so it can only be as wide as
        // the gap between them.
        let gap = if forward {
            target.position.x - (source.position.x + source.width)
        } else {
            source.position.x - (target.position.x + target.width)
        };

        if !edge.data.ambiguous {
            edge.label = Some(fit_edge_label(&edge.data.columns, gap.max(0.0)));
        } else {
            edge.label = None;
        }
        edge.source_handle = Some(handle_id(
            edge.data.source_column.as_deref(),
            "s",
            if forward { "r" } else { "l" },
            source.data.collapsed,
        ));
        edge.target_handle = Some(handle_id(
            edge.data.target_column.as_deref(),
            "t",
            if forward { "l" } else { "r" },
            target.data.collapsed,
        ));
    }

    edges
}
