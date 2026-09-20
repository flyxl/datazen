/**
 * Relation routing: turns relation groups into SVG shapes.
 *
 * Pure and DOM-free on purpose — this is where the composite-FK topology lives,
 * so it can be unit tested exhaustively without rendering a canvas.
 *
 * Topology rules
 * --------------
 * - One **group** = one constraint (or one manual join) = one visual object.
 * - A single-column group is a straight line (the common case).
 * - A composite group merges its source stubs into **one trunk**, then splits
 *   the trunk into one stub per target column: many → one → many.
 * - Groups sharing the same table pair get **parallel lanes** (14px apart) so
 *   they never overlap; lane order is derived from row indices, not from card
 *   positions, so it stays stable while a card is dragged.
 * - A self-referencing group loops out of the card's right edge.
 * - Every target end carries an arrowhead pointing into the referenced column.
 */

import {
  CARD_HEADER_HEIGHT,
  CARD_LIST_PADDING_Y,
  CARD_ROW_HEIGHT,
  CARD_WIDTH,
  cardHeight,
  cardListHeight,
} from './cardLayout';
import type { QbJoinType } from '../types';

export type RelationKind = 'fk' | 'manual';
export type RelationState = 'candidate' | 'confirmed' | 'partial';

export interface RelationPair {
  fromTable: string;
  fromColumn: string;
  toTable: string;
  toColumn: string;
  /** Whether this particular column pair is already part of the SQL. */
  confirmed: boolean;
}

export interface RelationGroup {
  /** Stable identity: the FK constraint key, or the manual join id. */
  id: string;
  kind: RelationKind;
  type: QbJoinType;
  /** FK constraint name, when known (used by the popover header). */
  constraint?: string;
  pairs: RelationPair[];
}

export interface RelationGeometryInput {
  groups: RelationGroup[];
  positions: Record<string, { x: number; y: number }>;
  /** table → column names in row order. */
  columnOrder: Record<string, string[]>;
  /**
   * Per-table scroll offset (px) of a card's column list. A card is a fixed
   * height and scrolls internally, so an anchor's visible position depends on
   * it; a column scrolled out of view is clamped to the list edge and flagged.
   */
  scrollTops?: Record<string, number>;
}

export interface RelationSegment {
  /** Polyline path; `stroke-linejoin: round` rounds the corners. */
  d: string;
  part: 'stub' | 'trunk';
  state: RelationState;
}

export interface RelationShape {
  groupId: string;
  kind: RelationKind;
  /** Aggregate state: all pairs confirmed, none, or in between. */
  state: RelationState;
  /** Confirmed pairs / total pairs — shown by the popover. */
  pairCount: number;
  confirmedCount: number;
  type: QbJoinType;
  constraint?: string;
  segments: RelationSegment[];
  /**
   * Round terminals at **both** ends of every connection. Deliberately
   * symmetric: the line shows a relationship, not a direction.
   */
  dots: Array<{
    x: number;
    y: number;
    state: RelationState;
    /** Set when the column itself is scrolled out of the card's list. */
    offscreen: 'up' | 'down' | null;
  }>;
  /** `table.column` keys of every column this group touches. */
  columnKeys: string[];
  /** Stable sort key, exported for tests. */
  lane: number;
}

/** Horizontal gap between two lanes of the same table pair. */
export const LANE_STEP = 14;
/** Extra length added beyond the outermost anchor so a trunk is always visible. */
export const TRUNK_OVERHANG = 10;
/** Minimum trunk length, so aligned rows still read as "merged". */
export const MIN_TRUNK = 24;
/** Distance a self-loop bulges out of the card. */
export const SELF_LOOP_OFFSET = 40;
/** Setback used when two cards leave no room for a trunk between them. */
const BYPASS_MARGIN = 28;

/** Unordered key identifying a card pair (used for lane grouping). */
function pairKey(fromTable: string, toTable: string): string {
  return [fromTable, toTable].sort().join('::');
}

function resolveIndex(
  columnOrder: Record<string, string[]>,
  table: string,
  column: string,
): number {
  return (columnOrder[table] ?? []).indexOf(column);
}

function stateOf(pairs: RelationPair[]): RelationState {
  const confirmed = pairs.filter((p) => p.confirmed).length;
  if (confirmed === 0) return 'candidate';
  if (confirmed === pairs.length) return 'confirmed';
  return 'partial';
}

/**
 * Assign each group a lane index within its table pair.
 *
 * Ordered by the smallest source row index, then target table + group id: the
 * ordering is position-independent, so lanes never swap while dragging a card.
 */
export function assignLanes(
  groups: RelationGroup[],
  columnOrder: Record<string, string[]>,
): Map<string, number> {
  const byPair = new Map<string, RelationGroup[]>();
  for (const group of groups) {
    const first = group.pairs[0];
    if (!first) continue;
    const key = pairKey(first.fromTable, first.toTable);
    const list = byPair.get(key) ?? [];
    list.push(group);
    byPair.set(key, list);
  }

  const lanes = new Map<string, number>();
  for (const list of byPair.values()) {
    const sorted = [...list].sort((a, b) => {
      const aRow = Math.min(
        ...a.pairs.map((p) => {
          const i = resolveIndex(columnOrder, p.fromTable, p.fromColumn);
          return i === -1 ? Number.MAX_SAFE_INTEGER : i;
        }),
      );
      const bRow = Math.min(
        ...b.pairs.map((p) => {
          const i = resolveIndex(columnOrder, p.fromTable, p.fromColumn);
          return i === -1 ? Number.MAX_SAFE_INTEGER : i;
        }),
      );
      if (aRow !== bRow) return aRow - bRow;
      const aTarget = a.pairs[0]?.toTable ?? '';
      const bTarget = b.pairs[0]?.toTable ?? '';
      if (aTarget !== bTarget) return aTarget.localeCompare(bTarget);
      return a.id.localeCompare(b.id);
    });
    sorted.forEach((group, index) => lanes.set(group.id, index));
  }
  return lanes;
}

/** Offset (px) applied to the trunk of the group at `lane` among `count` lanes. */
export function laneOffset(lane: number, count: number): number {
  return (lane - (count - 1) / 2) * LANE_STEP;
}

/** Build every relation shape for the current canvas state. */
export function buildRelationShapes(input: RelationGeometryInput): RelationShape[] {
  const { positions, columnOrder } = input;
  const lanes = assignLanes(input.groups, columnOrder);

  // Lane counts per table pair, needed to centre the lanes in the gap.
  const laneCounts = new Map<string, number>();
  for (const group of input.groups) {
    const first = group.pairs[0];
    if (!first) continue;
    const key = pairKey(first.fromTable, first.toTable);
    laneCounts.set(key, Math.max(laneCounts.get(key) ?? 0, (lanes.get(group.id) ?? 0) + 1));
  }

  const shapes: RelationShape[] = [];
  for (const group of input.groups) {
    const first = group.pairs[0];
    if (!first) continue;

    // Only draw pairs whose tables are on the canvas and whose columns exist.
    const resolved = group.pairs
      .map((pair) => ({
        pair,
        fromPos: positions[pair.fromTable],
        toPos: positions[pair.toTable],
        fromIndex: resolveIndex(columnOrder, pair.fromTable, pair.fromColumn),
        toIndex: resolveIndex(columnOrder, pair.toTable, pair.toColumn),
      }))
      .filter(
        (entry) =>
          entry.fromPos !== undefined &&
          entry.toPos !== undefined &&
          entry.fromIndex !== -1 &&
          entry.toIndex !== -1,
      );
    if (resolved.length === 0) continue;

    const key = pairKey(first.fromTable, first.toTable);
    const lane = lanes.get(group.id) ?? 0;
    const offset = laneOffset(lane, laneCounts.get(key) ?? 1);
    const state = stateOf(group.pairs);

    const shape: PartialShape =
      first.fromTable === first.toTable
        ? buildSelfLoop(resolved, offset, state, columnOrder, positions, input.scrollTops)
        : buildBetweenCards(resolved, offset, state, columnOrder, positions, input.scrollTops);

    shapes.push({
      ...shape,
      groupId: group.id,
      kind: group.kind,
      type: group.type,
      constraint: group.constraint,
      state,
      pairCount: group.pairs.length,
      confirmedCount: group.pairs.filter((p) => p.confirmed).length,
      lane,
      columnKeys: resolved.flatMap((entry) => [
        `${entry.pair.fromTable}.${entry.pair.fromColumn}`,
        `${entry.pair.toTable}.${entry.pair.toColumn}`,
      ]),
    });
  }
  return shapes;
}

/**
 * Where a column's anchor is drawn, in canvas coordinates.
 *
 * The list scrolls internally, so the anchor is the row's position minus the
 * list's scroll offset. A row scrolled out of view is **clamped to the visible
 * edge** (and reported) instead of being placed outside the card, where the line
 * would appear to point at nothing.
 */
function visibleAnchorY(
  table: string,
  columnIndex: number,
  positions: Record<string, { x: number; y: number }>,
  columnOrder: Record<string, string[]>,
  scrollTops: Record<string, number> | undefined,
): { y: number; offscreen: 'up' | 'down' | null } {
  const cardY = positions[table]!.y;
  const listTop = cardY + CARD_HEADER_HEIGHT;
  const listHeight = cardListHeight((columnOrder[table] ?? []).length);
  const contentY =
    listTop + CARD_LIST_PADDING_Y + columnIndex * CARD_ROW_HEIGHT + CARD_ROW_HEIGHT / 2;
  const y = contentY - (scrollTops?.[table] ?? 0);

  const top = listTop + CARD_ROW_HEIGHT / 2;
  const bottom = listTop + listHeight - CARD_ROW_HEIGHT / 2;
  if (y < top) return { y: top, offscreen: 'up' };
  if (y > bottom) return { y: bottom, offscreen: 'down' };
  return { y, offscreen: null };
}

interface ResolvedPair {
  pair: RelationPair;
  fromPos: { x: number; y: number };
  toPos: { x: number; y: number };
  fromIndex: number;
  toIndex: number;
}

type PartialShape = Pick<RelationShape, 'segments' | 'dots'>;

/** Card height for a table (a stacked route leaves its bottom edge). */
function cardHeightOf(table: string, columnOrder: Record<string, string[]>): number {
  return cardHeight((columnOrder[table] ?? []).length);
}

/**
 * A group spanning two different cards, drawn as **orthogonal polylines**.
 *
 * One constraint = one elbow (out of the column, across, into the other column).
 * A composite constraint merges its source stubs into a single trunk which then
 * splits into one stub per referenced column (`many → one → many`). There are no
 * arrowheads: the line expresses the relationship, not a direction.
 */
function buildBetweenCards(
  resolved: ResolvedPair[],
  offset: number,
  state: RelationState,
  columnOrder: Record<string, string[]>,
  positions: Record<string, { x: number; y: number }>,
  scrollTops: Record<string, number> | undefined,
): PartialShape {
  const first = resolved[0]!;
  const fromCenterX = first.fromPos.x + CARD_WIDTH / 2;
  const toCenterX = first.toPos.x + CARD_WIDTH / 2;
  const horizontal = Math.abs(toCenterX - fromCenterX) >= Math.abs(first.toPos.y - first.fromPos.y);

  const segments: RelationSegment[] = [];
  const dots: PartialShape['dots'] = [];

  const anchorOf = (entry: ResolvedPair, side: 'from' | 'to') => {
    const table = side === 'from' ? entry.pair.fromTable : entry.pair.toTable;
    const index = side === 'from' ? entry.fromIndex : entry.toIndex;
    return visibleAnchorY(table, index, positions, columnOrder, scrollTops);
  };

  if (horizontal) {
    const fromIsLeft = fromCenterX <= toCenterX;
    const leftCardX = fromIsLeft ? first.fromPos.x : first.toPos.x;
    const rightCardX = fromIsLeft ? first.toPos.x : first.fromPos.x;
    const gapLeft = leftCardX + CARD_WIDTH;
    const gapRight = rightCardX;

    // Cards too close (or overlapping): bypass to the right of both.
    const hasGap = gapRight - gapLeft >= LANE_STEP * 3;
    const trunkX = hasGap
      ? (gapLeft + gapRight) / 2 + offset
      : Math.max(gapLeft, rightCardX + CARD_WIDTH) + BYPASS_MARGIN + offset;

    const ends = resolved.map((entry) => {
      const from = anchorOf(entry, 'from');
      const to = anchorOf(entry, 'to');
      return {
        fromX: fromIsLeft ? entry.fromPos.x + CARD_WIDTH : entry.fromPos.x,
        toX: fromIsLeft ? entry.toPos.x : entry.toPos.x + CARD_WIDTH,
        fromY: from.y,
        toY: to.y,
        fromOffscreen: from.offscreen,
        toOffscreen: to.offscreen,
        pairState: (entry.pair.confirmed ? 'confirmed' : 'candidate') as RelationState,
      };
    });

    if (ends.length === 1) {
      const only = ends[0]!;
      segments.push({
        d:
          `M ${only.fromX} ${only.fromY} L ${trunkX} ${only.fromY}` +
          ` L ${trunkX} ${only.toY} L ${only.toX} ${only.toY}`,
        part: 'stub',
        state: only.pairState,
      });
      dots.push({
        x: only.fromX,
        y: only.fromY,
        state: only.pairState,
        offscreen: only.fromOffscreen,
      });
      dots.push({
        x: only.toX,
        y: only.toY,
        state: only.pairState,
        offscreen: only.toOffscreen,
      });
      return { segments, dots };
    }

    const ys = ends.flatMap((end) => [end.fromY, end.toY]);
    let trunkTop = Math.min(...ys) - TRUNK_OVERHANG;
    let trunkBottom = Math.max(...ys) + TRUNK_OVERHANG;
    if (trunkBottom - trunkTop < MIN_TRUNK) {
      const mid = (trunkTop + trunkBottom) / 2;
      trunkTop = mid - MIN_TRUNK / 2;
      trunkBottom = mid + MIN_TRUNK / 2;
    }

    segments.push({
      d: `M ${trunkX} ${trunkTop} L ${trunkX} ${trunkBottom}`,
      part: 'trunk',
      state,
    });
    for (const end of ends) {
      segments.push({
        d: `M ${end.fromX} ${end.fromY} L ${trunkX} ${end.fromY}`,
        part: 'stub',
        state: end.pairState,
      });
      segments.push({
        d: `M ${trunkX} ${end.toY} L ${end.toX} ${end.toY}`,
        part: 'stub',
        state: end.pairState,
      });
      dots.push({ x: end.fromX, y: end.fromY, state: end.pairState, offscreen: end.fromOffscreen });
      dots.push({ x: end.toX, y: end.toY, state: end.pairState, offscreen: end.toOffscreen });
    }
    return { segments, dots };
  }

  // Vertically stacked cards: the elbow leaves the facing top/bottom edges at
  // each column's X and runs across at a shared Y.
  const fromIsTop = first.fromPos.y <= first.toPos.y;
  const ends = resolved.map((entry) => ({
    fromX: entry.fromPos.x + CARD_WIDTH / 2,
    toX: entry.toPos.x + CARD_WIDTH / 2,
    fromY: fromIsTop
      ? entry.fromPos.y + cardHeightOf(entry.pair.fromTable, columnOrder)
      : entry.fromPos.y,
    toY: fromIsTop ? entry.toPos.y : entry.toPos.y + cardHeightOf(entry.pair.toTable, columnOrder),
    pairState: (entry.pair.confirmed ? 'confirmed' : 'candidate') as RelationState,
  }));

  const trunkY = (ends[0]!.fromY + ends[0]!.toY) / 2 + offset;

  if (ends.length === 1) {
    const only = ends[0]!;
    segments.push({
      d:
        `M ${only.fromX} ${only.fromY} L ${only.fromX} ${trunkY}` +
        ` L ${only.toX} ${trunkY} L ${only.toX} ${only.toY}`,
      part: 'stub',
      state: only.pairState,
    });
    dots.push({ x: only.fromX, y: only.fromY, state: only.pairState, offscreen: null });
    dots.push({ x: only.toX, y: only.toY, state: only.pairState, offscreen: null });
    return { segments, dots };
  }

  const xs = ends.flatMap((end) => [end.fromX, end.toX]);
  let trunkLeft = Math.min(...xs) - TRUNK_OVERHANG;
  let trunkRight = Math.max(...xs) + TRUNK_OVERHANG;
  if (trunkRight - trunkLeft < MIN_TRUNK) {
    const mid = (trunkLeft + trunkRight) / 2;
    trunkLeft = mid - MIN_TRUNK / 2;
    trunkRight = mid + MIN_TRUNK / 2;
  }

  segments.push({ d: `M ${trunkLeft} ${trunkY} L ${trunkRight} ${trunkY}`, part: 'trunk', state });
  for (const end of ends) {
    segments.push({
      d: `M ${end.fromX} ${end.fromY} L ${end.fromX} ${trunkY}`,
      part: 'stub',
      state: end.pairState,
    });
    segments.push({
      d: `M ${end.toX} ${trunkY} L ${end.toX} ${end.toY}`,
      part: 'stub',
      state: end.pairState,
    });
    dots.push({ x: end.fromX, y: end.fromY, state: end.pairState, offscreen: null });
    dots.push({ x: end.toX, y: end.toY, state: end.pairState, offscreen: null });
  }
  return { segments, dots };
}

/** Self-referencing group: bulges out of the card's right edge. */
function buildSelfLoop(
  resolved: ResolvedPair[],
  offset: number,
  state: RelationState,
  columnOrder: Record<string, string[]>,
  positions: Record<string, { x: number; y: number }>,
  scrollTops: Record<string, number> | undefined,
): PartialShape {
  const first = resolved[0]!;
  const anchorX = first.fromPos.x + CARD_WIDTH;
  const loopX = anchorX + SELF_LOOP_OFFSET + offset;

  const segments: RelationSegment[] = [];
  const dots: PartialShape['dots'] = [];

  const ends = resolved.map((entry) => {
    const from = visibleAnchorY(
      entry.pair.fromTable,
      entry.fromIndex,
      positions,
      columnOrder,
      scrollTops,
    );
    const to = visibleAnchorY(
      entry.pair.toTable,
      entry.toIndex,
      positions,
      columnOrder,
      scrollTops,
    );
    return {
      fromY: from.y,
      toY: to.y,
      fromOffscreen: from.offscreen,
      toOffscreen: to.offscreen,
      pairState: (entry.pair.confirmed ? 'confirmed' : 'candidate') as RelationState,
    };
  });

  const ys = ends.flatMap((end) => [end.fromY, end.toY]);
  const top = Math.min(...ys);
  const bottom = Math.max(...ys);
  segments.push({ d: `M ${loopX} ${top} L ${loopX} ${bottom}`, part: 'trunk', state });

  for (const end of ends) {
    segments.push({
      d: `M ${anchorX} ${end.fromY} L ${loopX} ${end.fromY}`,
      part: 'stub',
      state: end.pairState,
    });
    segments.push({
      d: `M ${loopX} ${end.toY} L ${anchorX} ${end.toY}`,
      part: 'stub',
      state: end.pairState,
    });
    dots.push({ x: anchorX, y: end.fromY, state: end.pairState, offscreen: end.fromOffscreen });
    dots.push({ x: anchorX, y: end.toY, state: end.pairState, offscreen: end.toOffscreen });
  }
  return { segments, dots };
}
