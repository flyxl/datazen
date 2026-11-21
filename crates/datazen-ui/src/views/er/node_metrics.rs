//! Port of `packages/frontend/src/windows/connection/er/nodeMetrics.ts`.
//!
//! Node geometry for the ER diagram, defined once. The layout engine needs
//! each node's exact size *before* the node renders, so the size cannot be
//! left to emerge from padding and line-heights. Every dimension here is also
//! applied explicitly in `TableNode` (`table_node.rs`), and the two are pinned
//! together by test.
//!
//! # Contract fidelity
//!
//! The functions mirror the React signatures' tolerance for out-of-range
//! input: a negative column count never produces a body above the header, and
//! a negative row index clamps to the first row. Callers holding `usize` casts
//! are unaffected.
//!
//! # Relationship to `table_node.rs`
//!
//! The constants are the single source of truth; `table_node.rs` re-exports
//! them for its own audits. `er_column_row_center_y` here follows the React
//! contract (the CSS border participates in layout, so it is part of the
//! offset), while `table_node.rs` keeps a rendering-local variant that omits
//! the 1pt border because Rune draws borders inside the box — the divergence
//! is deliberate and documented there.

/// Fixed node width. The header and column names truncate to fit.
pub(crate) const ER_NODE_WIDTH: f32 = 260.0;
/// Table-name header.
pub(crate) const ER_HEADER_HEIGHT: f32 = 36.0;
/// One column row.
pub(crate) const ER_COLUMN_ROW_HEIGHT: f32 = 24.0;
/// The "N columns" strip shown in place of the column list when collapsed.
pub(crate) const ER_COLLAPSED_FOOTER_HEIGHT: f32 = 24.0;
/// The container border, top and bottom.
pub(crate) const ER_NODE_BORDER: f32 = 1.0;
/// Above this many columns a table starts collapsed.
///
/// The column list does not scroll (see [`er_node_body_height`]), so a
/// 60-column table would be a 1478px node and would space its whole rank that
/// far apart. Collapsing is the release valve, and the node says how many
/// columns it is hiding. A column count is never negative, so this stays a
/// `usize` (the clamping functions below are the ones React tests with
/// out-of-range values).
pub(crate) const ER_AUTO_COLLAPSE_COLUMNS: usize = 30;

/// Height of the column list.
///
/// The list deliberately does **not** scroll. A connection point has to be at
/// a predictable offset inside its node, and a row inside a scroll container
/// moves with `scrollTop` — so a handle on a scrolled-out column would put the
/// edge endpoint outside the node, on top of whatever is drawn there.
/// Precise per-column edges and an internal scroll are mutually exclusive;
/// the diagram keeps the edges and collapses wide tables instead.
pub(crate) fn er_node_body_height(column_count: i32) -> f32 {
    column_count.max(0) as f32 * ER_COLUMN_ROW_HEIGHT
}

/// Vertical centre of column `index` inside the node, in node-local
/// coordinates.
///
/// This is where that column's handles sit, so an edge can meet the exact row.
pub(crate) fn er_column_row_center_y(index: i32) -> f32 {
    ER_NODE_BORDER
        + ER_HEADER_HEIGHT
        + index.max(0) as f32 * ER_COLUMN_ROW_HEIGHT
        + ER_COLUMN_ROW_HEIGHT / 2.0
}

/// Total rendered node height, matching `TableNode` exactly.
pub(crate) fn er_node_height(column_count: i32, collapsed: bool) -> f32 {
    let inner = if collapsed {
        ER_HEADER_HEIGHT + ER_COLLAPSED_FOOTER_HEIGHT
    } else {
        ER_HEADER_HEIGHT + er_node_body_height(column_count)
    };
    inner + ER_NODE_BORDER * 2.0
}
