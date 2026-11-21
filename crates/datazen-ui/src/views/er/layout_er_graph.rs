//! Port of `packages/frontend/src/windows/connection/er/layoutErGraph.ts`.
//!
//! Layered (Sugiyama-style) layout for the ER diagram. The React source
//! delegates to dagre; the Rust engine lives in `layout_layers.rs` (T566
//! split) and answers the same contract: related tables sit on neighbouring
//! ranks, a referenced table lands to the right of the table referencing it
//! (`LR`) or below it (`TB`), nothing overlaps, nothing sits on the canvas
//! edge, and the same schema always produces the same diagram.
//!
//! ## Why not a grid
//!
//! The previous layout placed table *i* at `(i % cols) * 300, floor(i / cols)
//! * height` in the order the backend returned them — which is `ORDER BY
//! relname`, i.e. the alphabet. Nothing about that relates to the foreign
//! keys, so related tables scattered, a wide table above a narrow one
//! overlapped it, and edges were routed across unrelated nodes. A layered
//! layout fixes the cause rather than the symptoms: it ranks tables by the
//! direction of the foreign keys and minimises crossings by construction.
//!
//! ## Why `LR`
//!
//! Edges run child → parent (the child holds the foreign key). With `LR` the
//! parent lands at a higher rank, i.e. to the **right**, so an edge leaves
//! the child's right side and enters the parent's left side — the handles
//! `TableNode` renders, so the common edge is a straight horizontal run.
//!
//! ## Fallback
//!
//! When the engine reports a shape it cannot order, the layout degrades to
//! the deterministic grid instead of blanking the view — the same safety net
//! the React source keeps around dagre's throws. React also warns once per
//! fallback; Rune has no console here, so the degradation is silent and the
//! grid is pinned by its own tests.

use crate::views::er::graph::{ErEdge, ErNode, ErPosition};
use crate::views::er::layout_layers::{grid_centres, layered_centres};

/// Gap between nodes in the same rank.
pub(crate) const NODE_SEP: f32 = 40.0;
/// Gap between ranks — the band each layer occupies.
pub(crate) const RANK_SEP: f32 = 110.0;
/// Gap between edges sharing a rank span.
///
/// The React source hands this to dagre's edge routing; the Rune canvas draws
/// its own edge paths between node handles, so the engine reserves no route
/// space — the constant documents the shared configuration.
pub(crate) const EDGE_SEP: f32 = 20.0;
/// Padding around the whole graph, so nothing sits on the canvas edge.
pub(crate) const ER_LAYOUT_MARGIN: f32 = 24.0;

/// `LR` (default) puts referenced tables to the right; `TB` puts them below.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ErLayoutDirection {
    LR,
    TB,
}

#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ErLayoutOptions {
    pub direction: Option<ErLayoutDirection>,
}

/// Position `nodes` with a layered layout; `edges` shape it but are returned
/// untouched.
///
/// Nodes must already carry `width` and `height`; the layout cannot infer
/// them, and guessing is what the previous grid got wrong. New node values
/// are returned — the input is not mutated — and the caller's order is kept:
/// the canvas draws in array order, so the layout must not reorder.
///
/// Disconnected tables are laid out too, in their own components, rather than
/// being dropped or stacked at the origin. A node the engine could not place
/// keeps the position it arrived with.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
pub(crate) fn layout_er_graph(
    nodes: &[ErNode],
    edges: &[ErEdge],
    options: ErLayoutOptions,
) -> Vec<ErNode> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let centres = match layered_centres(nodes, edges, &options) {
        Ok(centres) => centres,
        Err(_failure) => grid_centres(nodes),
    };

    nodes
        .iter()
        .map(|node| {
            let Some(&(cx, cy)) = centres.get(node.id.as_str()) else {
                return node.clone();
            };
            // The engine reports centres; the canvas positions by the
            // top-left corner.
            let mut placed = node.clone();
            placed.position = ErPosition::new(
                (cx - node.width / 2.0).round(),
                (cy - node.height / 2.0).round(),
            );
            placed
        })
        .collect()
}

/// [`layout_er_graph`] with the default left-to-right direction.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
pub(crate) fn layout_er_graph_default(nodes: &[ErNode], edges: &[ErEdge]) -> Vec<ErNode> {
    layout_er_graph(nodes, edges, ErLayoutOptions::default())
}

/// React's `ErPosition` helper for tests and hosts: whole-point corners.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
pub(crate) fn corner_of(node: &ErNode) -> ErPosition {
    node.position
}
