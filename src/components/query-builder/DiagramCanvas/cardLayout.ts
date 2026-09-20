/**
 * Canvas card placement and geometry.
 *
 * This module is the **single source of truth for card metrics**. `TableCard`
 * consumes these constants for its own inline sizes, and the SVG relation layer
 * computes column anchors from them — so a style tweak can never make the
 * connecting lines miss the column they point at.
 *
 * Cards are a fixed width (their height follows the column count) which makes
 * the anchor math exact and keeps the DOM and the SVG in agreement without
 * measuring anything.
 */

/** Grid step for card positions. */
export const CARD_GRID = 24;
/**
 * Horizontal stride between cards sharing a row (max card width + gap).
 * The 72px gap must stay above the relation router's bypass threshold
 * (`LANE_STEP * 3` in fkGeometry) — a narrower gap forces the trunk line to
 * detour around the right edge of the second card instead of crossing the gap.
 */
export const CARD_STRIDE = 312;
/** Vertical tolerance for "this drop belongs to the same row". */
export const CARD_ROW_TOLERANCE = 56;

// ── Card metrics (mirrored by TableCard's inline styles) ──────

/** Card width. Fixed so column anchors are deterministic. */
export const CARD_WIDTH = 240;
/** Card header height: py-2 (8+8) + 20px content + 1px bottom border. */
export const CARD_HEADER_HEIGHT = 37;
/** Column list vertical padding (py-1). */
export const CARD_LIST_PADDING_Y = 4;
/** One column row. */
export const CARD_ROW_HEIGHT = 24;
/** Horizontal inset of row content (px-3). */
export const CARD_ROW_INSET_X = 12;

/**
 * Maximum height of a card's column list. Beyond this the list scrolls
 * internally — a card is a fixed-height object, as in Navicat, rather than
 * growing with the table's column count.
 */
export const CARD_LIST_MAX_HEIGHT = 200;

/** Padding kept around the cards inside the scrollable canvas content. */
export const CANVAS_PADDING = 240;

export type CardPositions = Record<string, { x: number; y: number }>;

/** Round a coordinate to the canvas grid, never negative. */
export function snapToGrid(value: number, grid: number = CARD_GRID): number {
  return Math.max(0, Math.round(value / grid) * grid);
}

/**
 * Height of the column list box: the columns' natural height, capped at
 * {@link CARD_LIST_MAX_HEIGHT}. Anything beyond the cap is scrolled internally.
 */
export function cardListHeight(columnCount: number): number {
  const natural = CARD_LIST_PADDING_Y * 2 + columnCount * CARD_ROW_HEIGHT;
  return Math.min(natural, CARD_LIST_MAX_HEIGHT);
}

/** True when the card's list needs an internal scrollbar. */
export function cardListScrolls(columnCount: number): boolean {
  return CARD_LIST_PADDING_Y * 2 + columnCount * CARD_ROW_HEIGHT > CARD_LIST_MAX_HEIGHT;
}

/** Rendered height of a card (fixed once the list reaches its cap). */
export function cardHeight(columnCount: number): number {
  return CARD_HEADER_HEIGHT + cardListHeight(columnCount);
}

/** Vertical center of column row `index` inside a card placed at `cardY`. */
export function rowCenterY(cardY: number, index: number): number {
  return (
    cardY + CARD_HEADER_HEIGHT + CARD_LIST_PADDING_Y + index * CARD_ROW_HEIGHT + CARD_ROW_HEIGHT / 2
  );
}

/**
 * Size of the scrollable canvas content: the bounding box of every card plus
 * padding, never smaller than the viewport (so the empty canvas still fills the
 * panel and cannot be scrolled away).
 */
export function canvasContentSize(
  cards: Array<{ pos: { x: number; y: number }; columnCount: number }>,
  viewport: { width: number; height: number },
): { width: number; height: number } {
  let right = 0;
  let bottom = 0;
  for (const card of cards) {
    right = Math.max(right, card.pos.x + CARD_WIDTH + CANVAS_PADDING);
    bottom = Math.max(bottom, card.pos.y + cardHeight(card.columnCount) + CANVAS_PADDING);
  }
  return {
    width: Math.max(right, viewport.width),
    height: Math.max(bottom, viewport.height),
  };
}

/**
 * Resolve the position for a newly dropped card.
 *
 * Returns a grid-snapped position that shares the top edge of any nearby row
 * and sits to the right of the cards already in it.
 */
export function alignDroppedCard(pos: { x: number; y: number }, existing: CardPositions) {
  const x = snapToGrid(pos.x);
  const y = snapToGrid(pos.y);

  const sameRow = Object.values(existing).filter((p) => Math.abs(p.y - y) <= CARD_ROW_TOLERANCE);
  if (sameRow.length === 0) return { x, y };

  const top = sameRow[0]!.y;
  const rightMost = Math.max(...sameRow.map((p) => p.x));
  return { x: Math.max(x, rightMost + CARD_STRIDE), y: top };
}

/**
 * Resolve a manual drag position: only snap to grid.
 *
 * Unlike {@link alignDroppedCard}, this does **not** force row alignment —
 * the user is freely repositioning an existing card and should not be
 * surprised by a horizontal jump to the right of another card.
 */
export function resolveDragPosition(
  pos: { x: number; y: number },
  _others: CardPositions,
  _selfKey: string,
) {
  return { x: snapToGrid(pos.x), y: snapToGrid(pos.y) };
}
