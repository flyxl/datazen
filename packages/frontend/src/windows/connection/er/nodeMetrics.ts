/**
 * Node geometry for the ER diagram, defined once.
 *
 * The layout engine needs each node's exact size *before* the node renders, so the
 * size cannot be left to emerge from padding and line-heights — that is how the
 * previous grid came to reserve 1000px for a table that renders at 340px, and how
 * it drew nodes on top of each other. Every dimension here is therefore also
 * applied explicitly in `TableNode`, and the two are pinned together by test.
 *
 * Width is fixed rather than fluid for the same reason: a node that renders
 * anywhere between 180px and 280px cannot be packed without guessing.
 */

/** Fixed node width. The header and column names truncate to fit. */
export const ER_NODE_WIDTH = 260;
/** Table-name header. */
export const ER_HEADER_HEIGHT = 36;
/** One column row. */
export const ER_COLUMN_ROW_HEIGHT = 24;
/** The "N columns" strip shown in place of the column list when collapsed. */
export const ER_COLLAPSED_FOOTER_HEIGHT = 24;
/** The container border, top and bottom. */
export const ER_NODE_BORDER = 1;
/**
 * Above this many columns a table starts collapsed.
 *
 * The column list does not scroll (see below), so a 60-column table would be a
 * 1478px node and would space its whole rank that far apart. Collapsing is the
 * release valve, and the node says how many columns it is hiding.
 */
export const ER_AUTO_COLLAPSE_COLUMNS = 30;

/**
 * Height of the column list.
 *
 * The list deliberately does **not** scroll. A connection point has to be at a
 * predictable offset inside its node, and a row inside a scroll container moves
 * with `scrollTop` — so a handle on a scrolled-out column would put the edge
 * endpoint outside the node, on top of whatever is drawn there. Precise per-column
 * edges and an internal scroll are mutually exclusive; the diagram keeps the edges
 * and collapses wide tables instead.
 */
export function erNodeBodyHeight(columnCount: number): number {
  return Math.max(columnCount, 0) * ER_COLUMN_ROW_HEIGHT;
}

/**
 * Vertical centre of column `index` inside the node, in node-local coordinates.
 *
 * This is where that column's handles sit, so an edge can meet the exact row.
 */
export function erColumnRowCenterY(index: number): number {
  return (
    ER_NODE_BORDER +
    ER_HEADER_HEIGHT +
    Math.max(index, 0) * ER_COLUMN_ROW_HEIGHT +
    ER_COLUMN_ROW_HEIGHT / 2
  );
}

/** Total rendered node height, matching `TableNode` exactly. */
export function erNodeHeight(columnCount: number, collapsed = false): number {
  const inner = collapsed
    ? ER_HEADER_HEIGHT + ER_COLLAPSED_FOOTER_HEIGHT
    : ER_HEADER_HEIGHT + erNodeBodyHeight(columnCount);
  return inner + ER_NODE_BORDER * 2;
}
