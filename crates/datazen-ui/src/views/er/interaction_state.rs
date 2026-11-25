//! Port of `packages/frontend/src/windows/connection/er/interactionState.ts`.
//!
//! View-state transforms for the ER canvas. These are pure functions over
//! nodes and edges so the behaviour can be tested without rendering a canvas
//! (Rune measures nothing here either — the host applies the transforms to
//! whatever it drew).
//!
//! Every function returns new values and leaves its inputs alone, exactly as
//! the React sources do; the callers rely on that to re-derive styling from
//! the pristine graph on every interaction.

use std::collections::{HashMap, HashSet};

use crate::views::er::graph::{ErEdge, ErNode, ErPosition};

/// A position the user dragged a node to.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PinnedPosition {
    pub x: f32,
    pub y: f32,
}

/// Opacity of an edge that is not related to the hovered table.
pub const HOVER_DIM_OPACITY: f32 = 0.15;
/// Stroke width of an edge that is related to the hovered table.
pub const HOVER_EMPHASIS_WIDTH: f32 = 2.5;
/// Draw order for emphasised edges, so they sit above the dimmed ones.
pub const HOVER_EMPHASIS_Z_INDEX: i32 = 1000;

/// Re-apply hand-placed positions over a fresh layout.
///
/// Only the pinned nodes move; everything else keeps the position the layout
/// gave it, so dragging one table does not disturb the rest of the diagram. A
/// relayout runs on schema load, on collapse and on focus, and without this
/// it would throw away every hand-placed node each time. A pin for a table
/// that is no longer on the canvas is simply ignored.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
pub fn apply_pinned_positions(
    nodes: &[ErNode],
    pinned: &HashMap<String, PinnedPosition>,
) -> Vec<ErNode> {
    nodes
        .iter()
        .map(|node| {
            let Some(position) = pinned.get(&node.id) else {
                return node.clone();
            };
            let mut moved = node.clone();
            moved.position = ErPosition::new(position.x, position.y);
            moved
        })
        .collect()
}

/// The hovered table and every table it is directly related to.
///
/// Returns `None` when nothing is hovered, so callers can skip the styling
/// work entirely rather than dimming everything.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
pub fn hovered_neighbourhood(
    edges: &[ErEdge],
    hovered: Option<&str>,
) -> Option<HashSet<String>> {
    let hovered = hovered?;
    let mut related = HashSet::new();
    related.insert(hovered.to_owned());
    for edge in edges {
        if edge.source == hovered {
            related.insert(edge.target.clone());
        } else if edge.target == hovered {
            related.insert(edge.source.clone());
        }
    }
    Some(related)
}

/// Dim every node outside the hovered table's neighbourhood.
///
/// A node the search already dimmed stays dimmed: hovering must not un-dim
/// what an earlier pass darkened.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
pub fn apply_hover_to_nodes(
    nodes: &[ErNode],
    neighbourhood: Option<&HashSet<String>>,
) -> Vec<ErNode> {
    let Some(neighbourhood) = neighbourhood else {
        return nodes.to_vec();
    };
    nodes
        .iter()
        .map(|node| {
            if neighbourhood.contains(&node.id) {
                node.clone()
            } else {
                let mut dimmed = node.clone();
                dimmed.data.dimmed = true;
                dimmed
            }
        })
        .collect()
}

/// Bring the hovered table's relationships forward and push the rest back.
///
/// The label rides along with the edge, so a dimmed edge dims its label too.
/// Styling the edge already had (its stroke, say) is preserved — only the
/// hover fields are touched.
#[allow(dead_code)] // canvas model; consumer: the ER canvas host (not wired).
pub fn apply_hover_to_edges(edges: &[ErEdge], hovered: Option<&str>) -> Vec<ErEdge> {
    let Some(hovered) = hovered else {
        return edges.to_vec();
    };
    edges
        .iter()
        .map(|edge| {
            let incident = edge.source == hovered || edge.target == hovered;
            let mut styled = edge.clone();
            if incident {
                styled.z_index = Some(HOVER_EMPHASIS_Z_INDEX);
                styled.style.stroke_width = Some(HOVER_EMPHASIS_WIDTH);
            } else {
                styled.style.opacity = Some(HOVER_DIM_OPACITY);
            }
            styled
        })
        .collect()
}
