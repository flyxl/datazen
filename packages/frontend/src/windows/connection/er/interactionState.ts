/**
 * View-state transforms for the ER canvas.
 *
 * These are pure functions over nodes and edges so the behaviour can be tested
 * without rendering React Flow, which needs a browser to measure anything.
 */

import type { Edge, Node } from '@xyflow/react';

/** A position the user dragged a node to. */
export interface PinnedPosition {
  x: number;
  y: number;
}

/**
 * Re-apply hand-placed positions over a fresh layout.
 *
 * Only the pinned nodes move; everything else keeps the position the layout gave
 * it, so dragging one table does not disturb the rest of the diagram. A relayout
 * runs on schema load, on collapse and on focus, and without this it would throw
 * away every hand-placed node each time.
 */
export function applyPinnedPositions(
  nodes: readonly Node[],
  pinned: ReadonlyMap<string, PinnedPosition>,
): Node[] {
  if (pinned.size === 0) return [...nodes];
  return nodes.map((node) => {
    const position = pinned.get(node.id);
    return position ? { ...node, position: { ...position } } : node;
  });
}

/**
 * The hovered table and every table it is directly related to.
 *
 * Returns `null` when nothing is hovered, so callers can skip the styling work
 * entirely rather than dimming everything.
 */
export function hoveredNeighbourhood(
  edges: readonly Edge[],
  hovered: string | null,
): Set<string> | null {
  if (!hovered) return null;
  const related = new Set<string>([hovered]);
  for (const edge of edges) {
    if (edge.source === hovered) related.add(edge.target as string);
    else if (edge.target === hovered) related.add(edge.source as string);
  }
  return related;
}

/** Dim every node outside the hovered table's neighbourhood. */
export function applyHoverToNodes(
  nodes: readonly Node[],
  neighbourhood: ReadonlySet<string> | null,
): Node[] {
  if (!neighbourhood) return [...nodes];
  return nodes.map((node) =>
    neighbourhood.has(node.id) ? node : { ...node, data: { ...node.data, dimmed: true } },
  );
}

/** Opacity of an edge that is not related to the hovered table. */
export const HOVER_DIM_OPACITY = 0.15;
/** Stroke width of an edge that is related to the hovered table. */
export const HOVER_EMPHASIS_WIDTH = 2.5;
/** Draw order for emphasised edges, so they sit above the dimmed ones. */
export const HOVER_EMPHASIS_Z_INDEX = 1000;

/**
 * Bring the hovered table's relationships forward and push the rest back.
 *
 * The label rides along with the edge, so a dimmed edge dims its label too.
 */
export function applyHoverToEdges(edges: readonly Edge[], hovered: string | null): Edge[] {
  if (!hovered) return [...edges];
  return edges.map((edge) => {
    const incident = edge.source === hovered || edge.target === hovered;
    return incident
      ? {
          ...edge,
          zIndex: HOVER_EMPHASIS_Z_INDEX,
          style: { ...edge.style, strokeWidth: HOVER_EMPHASIS_WIDTH },
        }
      : { ...edge, style: { ...edge.style, opacity: HOVER_DIM_OPACITY } };
  });
}

/** Rough width of one character at the 10px label size. */
const LABEL_CHAR_WIDTH = 6;
/** Padding either side of the text. */
const LABEL_PADDING = 12;

/**
 * Whether a label has room to be drawn.
 *
 * A label is centred on the edge, so it needs the horizontal gap between the two
 * nodes to be at least as wide as the text.
 *
 * @param gap - Horizontal space between the source and target node edges, in px.
 */
export function labelFits(label: string | undefined, gap: number): boolean {
  if (!label) return false;
  return label.length * LABEL_CHAR_WIDTH + LABEL_PADDING <= gap;
}

/**
 * Shorten `text` to the longest form that fits in `gap`, ellipsis included.
 *
 * The ellipsis occupies a character slot of its own, so the budget is spent on
 * `maxChars - 1` characters of text rather than on `maxChars`.
 */
function truncateToGap(text: string, gap: number): string {
  const maxChars = Math.floor((gap - LABEL_PADDING) / LABEL_CHAR_WIDTH);
  // Not even an ellipsis fits; draw nothing rather than overflow the gap.
  if (maxChars < 1) return '';
  if (text.length <= maxChars) return text;
  if (maxChars === 1) return '…';
  return `${text.slice(0, maxChars - 1)}…`;
}

/**
 * Fit a relationship's column list into the space the edge actually has.
 *
 * Measured against the layered layout, a gap of 110px holds about 16 characters —
 * enough for every single-column foreign key, but not for a composite one
 * (`tenant_id, account_id` needs 138px). Rather than let such a label overlap the
 * nodes on either side, or drop it and lose the relationship's meaning, it is
 * shortened to the column the edge attaches to plus a count of the rest.
 */
export function fitEdgeLabel(columns: readonly string[], gap: number): string {
  const first = columns[0] ?? '';
  if (columns.length === 0) return '';
  const full = columns.join(', ');
  if (labelFits(full, gap)) return full;

  const summarised = columns.length > 1 ? `${first} +${columns.length - 1}` : first;
  if (labelFits(summarised, gap)) return summarised;

  return truncateToGap(summarised, gap);
}
