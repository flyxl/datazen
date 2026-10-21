/**
 * Grid selection geometry and state transitions (DB-01).
 *
 * Pure logic only: no React, no store imports, so the whole selection model can be
 * unit-tested without a component tree. Coordinates are `(rowIndex, columnName)`:
 * the column *name* stays stable across `visibleColumns` changes and column moves,
 * while a column index does not (00-contracts §4.1).
 */

export interface CellCoord {
  /** 0-based row index inside the current page. Render-only: never used to build SQL. */
  rowIndex: number;
  /** Column name rather than index: indices drift when columns are hidden or reordered. */
  columnName: string;
}

export interface CellRange {
  /** Both corners; their order is meaningless and normalized by the helpers below. */
  start: CellCoord;
  end: CellCoord;
}

export interface GridSelection {
  /** Anchor corner, fixed on pointerdown / when Shift-extension starts. */
  anchor: CellCoord | null;
  /** Moving corner; follows the pointer drag or Shift+arrow. */
  focus: CellCoord | null;
  /** Extra non-contiguous single-cell ranges (Cmd/Ctrl+click). */
  extraRanges: CellRange[];
  mode: 'none' | 'cell' | 'row';
}

/**
 * Frozen empty selection shared by every "no selection" site, so memo dependencies
 * stay referentially stable. Never write into it: all transitions build new objects.
 */
const EMPTY_EXTRA_RANGES: CellRange[] = Object.freeze([]) as unknown as CellRange[];

export const EMPTY_GRID_SELECTION: GridSelection = Object.freeze({
  anchor: null,
  focus: null,
  extraRanges: EMPTY_EXTRA_RANGES,
  mode: 'none',
});

/** Row set used when a mode has no rows to report. */
const EMPTY_ROW_SET: ReadonlySet<number> = new Set<number>();

/**
 * The selection plus the row set that `mode: 'row'` keeps in `TableState.selectedRows`.
 *
 * `GridSelection` intentionally owns no row set (00-contracts §4.1), yet §4.2.1 requires
 * `mode: 'row'` to report `selectedRows` verbatim. `selectedRows` is therefore **required**
 * (00-contracts §4.2.1, 落地校正): an optional field would let a caller pass a bare
 * `GridSelection` and silently get an empty row set in row mode — the exact contract breach
 * the freeze is meant to make impossible. Callers hand over the panel's own set; the
 * parameter stays a single object, so the frozen signature is unchanged.
 */
export type GridSelectionState = GridSelection & {
  readonly selectedRows: ReadonlySet<number>;
};

function coordEquals(a: CellCoord, b: CellCoord): boolean {
  return a.rowIndex === b.rowIndex && a.columnName === b.columnName;
}

/** Column name → index in render order; `-1` when the column is not displayed. */
export function columnIndexFromCoord(
  coord: CellCoord,
  columnOrder: readonly string[],
): number {
  return columnOrder.indexOf(coord.columnName);
}

export function cellCoordEquals(a: CellCoord | null, b: CellCoord | null): boolean {
  if (a === null || b === null) return a === b;
  return coordEquals(a, b);
}

/** Row/cell coordinate → coordinate, or `null` when either index falls outside the grid. */
export function coordAt(
  rowIndex: number,
  columnIndex: number,
  columnOrder: readonly string[],
): CellCoord | null {
  if (!Number.isInteger(rowIndex) || rowIndex < 0) return null;
  if (!Number.isInteger(columnIndex) || columnIndex < 0 || columnIndex >= columnOrder.length) {
    return null;
  }
  const columnName = columnOrder[columnIndex];
  return columnName === undefined ? null : { rowIndex, columnName };
}

/**
 * Pull a coordinate back into the grid. An unknown column (hidden while referenced) or an
 * empty grid has no legal coordinate at all, hence `null`; a row index is clamped.
 */
export function clampCellCoord(
  coord: CellCoord,
  rowCount: number,
  columnOrder: readonly string[],
): CellCoord | null {
  if (rowCount <= 0) return null;
  if (!Number.isInteger(coord.rowIndex)) return null;
  if (columnIndexFromCoord(coord, columnOrder) < 0) return null;
  const rowIndex = Math.min(Math.max(coord.rowIndex, 0), rowCount - 1);
  return { rowIndex, columnName: coord.columnName };
}

/**
 * Normalize a range so rows grow downwards and columns grow in render order — never in
 * lexicographic order, because `id` sorts before `name` while the grid may show the
 * opposite. An unknown column has no position to compare, so the range is "not found".
 */
export function normalizeCellRange(
  range: CellRange,
  columnOrder: readonly string[],
): CellRange | null {
  const startColumn = columnOrder.indexOf(range.start.columnName);
  const endColumn = columnOrder.indexOf(range.end.columnName);
  if (startColumn < 0 || endColumn < 0) return null;
  const topRow = Math.min(range.start.rowIndex, range.end.rowIndex);
  const bottomRow = Math.max(range.start.rowIndex, range.end.rowIndex);
  if (!Number.isInteger(topRow) || topRow < 0) return null;
  const left = startColumn <= endColumn ? range.start : range.end;
  const right = startColumn <= endColumn ? range.end : range.start;
  return {
    start: { rowIndex: topRow, columnName: left.columnName },
    end: { rowIndex: bottomRow, columnName: right.columnName },
  };
}

/** Normalized anchor→focus rectangle; `null` unless both corners exist and are displayable. */
export function normalizeSelectionRange(
  selection: GridSelection,
  columnOrder: readonly string[],
): CellRange | null {
  if (selection.anchor === null || selection.focus === null) return null;
  return normalizeCellRange({ start: selection.anchor, end: selection.focus }, columnOrder);
}

/** Every region the selection covers, main region first, then the extra ranges (all normalized). */
export function selectionRanges(
  selection: GridSelection,
  columnOrder: readonly string[],
): CellRange[] {
  if (selection.mode !== 'cell') return [];
  const ranges: CellRange[] = [];
  const main = normalizeSelectionRange(selection, columnOrder);
  if (main) ranges.push(main);
  for (const extra of selection.extraRanges) {
    const normalized = normalizeCellRange(extra, columnOrder);
    if (normalized) ranges.push(normalized);
  }
  return ranges;
}

export function cellRangeContains(
  range: CellRange,
  coord: CellCoord,
  columnOrder: readonly string[],
): boolean {
  const normalized = normalizeCellRange(range, columnOrder);
  if (!normalized) return false;
  const column = columnIndexFromCoord(coord, columnOrder);
  if (column < 0) return false;
  const startColumn = columnOrder.indexOf(normalized.start.columnName);
  const endColumn = columnOrder.indexOf(normalized.end.columnName);
  return (
    coord.rowIndex >= normalized.start.rowIndex &&
    coord.rowIndex <= normalized.end.rowIndex &&
    column >= startColumn &&
    column <= endColumn
  );
}

export function gridSelectionContains(
  selection: GridSelection,
  coord: CellCoord,
  columnOrder: readonly string[],
): boolean {
  return selectionRanges(selection, columnOrder).some((range) =>
    cellRangeContains(range, coord, columnOrder),
  );
}

/** Total cell area of every region, overlapping regions counted twice (render/summary metric). */
export function gridSelectionCellCount(
  selection: GridSelection,
  columnOrder: readonly string[],
): number {
  let total = 0;
  for (const range of selectionRanges(selection, columnOrder)) {
    const startColumn = columnOrder.indexOf(range.start.columnName);
    const endColumn = columnOrder.indexOf(range.end.columnName);
    if (startColumn < 0 || endColumn < 0) continue;
    const rows = range.end.rowIndex - range.start.rowIndex + 1;
    const columns = endColumn - startColumn + 1;
    total += rows * columns;
  }
  return total;
}

/** Distinct displayed columns covered by the selection (the "M" of "N rows × M columns"). */
export function columnsCoveredBySelection(
  selection: GridSelection,
  columnOrder: readonly string[],
): number {
  const covered = new Set<number>();
  for (const range of selectionRanges(selection, columnOrder)) {
    const startColumn = columnOrder.indexOf(range.start.columnName);
    const endColumn = columnOrder.indexOf(range.end.columnName);
    if (startColumn < 0 || endColumn < 0) continue;
    for (let column = startColumn; column <= endColumn; column += 1) covered.add(column);
  }
  return covered.size;
}

/**
 * Row indices covered by the selection (unordered, de-duplicated). This is the only source
 * for row-level batch actions in `cell` mode; it must never be written back into
 * `selectedRows` (00-contracts §4.2, rule 3).
 *
 * Row indices are UI indices: any write path still has to resolve a row identity through
 * `rowIdentityAnchors` before touching the database.
 */
export function rowsCoveredBySelection(state: GridSelectionState): ReadonlySet<number> {
  // No `?? EMPTY_ROW_SET` here: the type guarantees the field, and a fallback would swallow
  // exactly the contract violation this signature exists to prevent.
  if (state.mode === 'row') return state.selectedRows;
  if (state.mode !== 'cell') return EMPTY_ROW_SET;
  if (state.anchor === null || state.focus === null) return EMPTY_ROW_SET;
  // Row coverage never depends on column order, so the rectangle is rebuilt directly from
  // the two corners instead of going through `selectionRanges(selection, columnOrder)`.
  const rows = new Set<number>();
  const addRangeRows = (start: CellCoord, end: CellCoord) => {
    const top = Math.min(start.rowIndex, end.rowIndex);
    const bottom = Math.max(start.rowIndex, end.rowIndex);
    if (!Number.isInteger(top) || top < 0) return;
    for (let rowIndex = top; rowIndex <= bottom; rowIndex += 1) rows.add(rowIndex);
  };
  addRangeRows(state.anchor, state.focus);
  for (const extra of state.extraRanges) addRangeRows(extra.start, extra.end);
  return rows;
}

/** Structural equality; extra ranges are compared as an unordered set. */
export function gridSelectionEquals(
  a: GridSelection,
  b: GridSelection,
  columnOrder: readonly string[],
): boolean {
  if (a.mode !== b.mode) return false;
  if (!cellCoordEquals(a.anchor, b.anchor)) return false;
  if (!cellCoordEquals(a.focus, b.focus)) return false;
  if (a.extraRanges.length !== b.extraRanges.length) return false;
  return a.extraRanges.every((rangeA) => {
    const normalizedA = normalizeCellRange(rangeA, columnOrder);
    if (!normalizedA) return false;
    return b.extraRanges.some((rangeB) => {
      const normalizedB = normalizeCellRange(rangeB, columnOrder);
      if (!normalizedB) return false;
      return sameRange(normalizedA, normalizedB);
    });
  });
}

function sameRange(a: CellRange, b: CellRange): boolean {
  return coordEquals(a.start, b.start) && coordEquals(a.end, b.end);
}

/**
 * Move the focus corner by a row/column delta and clamp it into the grid. Returns `null`
 * when the clamped result equals the input — i.e. there is nowhere left to go — so callers
 * can leave the selection untouched instead of writing a no-op state.
 */
export function moveCellFocus(
  focus: CellCoord,
  deltaRow: number,
  deltaColumn: number,
  rowCount: number,
  columnOrder: readonly string[],
): CellCoord | null {
  if (rowCount <= 0) return null;
  const fromColumn = columnIndexFromCoord(focus, columnOrder);
  if (fromColumn < 0) return null;
  const nextRow = Math.min(Math.max(focus.rowIndex + deltaRow, 0), rowCount - 1);
  const nextColumn = Math.min(Math.max(fromColumn + deltaColumn, 0), columnOrder.length - 1);
  if (nextRow === focus.rowIndex && nextColumn === fromColumn) return null;
  const columnName = columnOrder[nextColumn];
  return columnName === undefined ? null : { rowIndex: nextRow, columnName };
}

/**
 * Build a selection from its two corners.
 *
 * `row` mode carries no geometry at all (00-contracts §4 invariant I2): the row highlight
 * comes from `selectedRows`, so anchor/focus/extras are dropped here instead of being kept
 * as a hidden second source of truth.
 */
export function selectionFromAnchorFocus(
  anchor: CellCoord,
  focus: CellCoord,
  mode: 'cell' | 'row',
  extraRanges: CellRange[] = [],
): GridSelection {
  if (mode === 'row') {
    return { anchor: null, focus: null, extraRanges: [], mode: 'row' };
  }
  return { anchor, focus, extraRanges: [...extraRanges], mode: 'cell' };
}

/**
 * Cmd/Ctrl+click on one cell: append it as an extra single-cell region, or drop it when it
 * already is one. Clicking inside the main region is a no-op (it is already selected), and
 * an unknown column leaves the selection untouched.
 */
export function toggleExtraRange(
  selection: GridSelection,
  coord: CellCoord,
  columnOrder: readonly string[],
): GridSelection {
  const extra = normalizeCellRange({ start: coord, end: coord }, columnOrder);
  if (!extra) return selection;
  if (selection.mode !== 'cell' || selection.anchor === null || selection.focus === null) {
    return { anchor: coord, focus: coord, extraRanges: [], mode: 'cell' };
  }
  const existing = selection.extraRanges.findIndex((range) => {
    const normalized = normalizeCellRange(range, columnOrder);
    return normalized !== null && sameRange(normalized, extra);
  });
  if (existing >= 0) {
    return {
      ...selection,
      extraRanges: selection.extraRanges.filter((_, index) => index !== existing),
    };
  }
  const main = normalizeSelectionRange(selection, columnOrder);
  if (main && cellRangeContains(main, coord, columnOrder)) return selection;
  return { ...selection, extraRanges: [...selection.extraRanges, extra] };
}

function coordIsDisplayable(
  coord: CellCoord | null,
  rowCount: number,
  columnOrder: readonly string[],
): boolean {
  if (coord === null) return false;
  if (!Number.isInteger(coord.rowIndex) || coord.rowIndex < 0 || coord.rowIndex >= rowCount) {
    return false;
  }
  return columnOrder.includes(coord.columnName);
}

/**
 * Converge a selection after rows or displayed columns changed (page reload, column
 * visibility). An invalid main corner clears the whole selection; an invalid extra range is
 * dropped; `row` mode keeps no geometry by construction.
 */
export function reconcileGridSelection(
  selection: GridSelection,
  rowCount: number,
  columnOrder: readonly string[],
): GridSelection {
  if (selection.mode === 'row') {
    if (
      selection.anchor === null &&
      selection.focus === null &&
      selection.extraRanges.length === 0
    ) {
      return selection;
    }
    return { anchor: null, focus: null, extraRanges: [], mode: 'row' };
  }
  if (selection.mode === 'none') return EMPTY_GRID_SELECTION;
  if (
    !coordIsDisplayable(selection.anchor, rowCount, columnOrder) ||
    !coordIsDisplayable(selection.focus, rowCount, columnOrder)
  ) {
    return EMPTY_GRID_SELECTION;
  }
  const extraRanges: CellRange[] = [];
  let droppedAny = false;
  for (const range of selection.extraRanges) {
    const normalized = normalizeCellRange(range, columnOrder);
    if (
      normalized === null ||
      normalized.start.rowIndex >= rowCount ||
      normalized.end.rowIndex >= rowCount
    ) {
      droppedAny = true;
      continue;
    }
    extraRanges.push(normalized);
  }
  if (!droppedAny) {
    const unchanged = extraRanges.every((range, index) =>
      sameRange(range, selection.extraRanges[index]),
    );
    if (unchanged) return selection;
  }
  return { ...selection, extraRanges };
}
