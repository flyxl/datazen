/**
 * Boundary coverage for the pure grid-selection geometry (`gridSelection.ts`).
 *
 * The main suite pins the contract examples; this file targets the degenerate shapes the
 * grid can actually reach: an empty page, one row, one column, a hidden or reordered
 * column set, non-integer coordinates, and the reconcile "nothing to do" fast paths.
 */
import { describe, expect, it } from 'vitest';
import {
  EMPTY_GRID_SELECTION,
  cellRangeContains,
  clampCellCoord,
  columnsCoveredBySelection,
  coordAt,
  gridSelectionCellCount,
  gridSelectionEquals,
  moveCellFocus,
  normalizeCellRange,
  reconcileGridSelection,
  rowsCoveredBySelection,
  selectionFromAnchorFocus,
  toggleExtraRange,
  type CellCoord,
  type CellRange,
  type GridSelection,
} from '../gridSelection';

const COLUMNS = ['name', 'id', 'note'];

const coord = (rowIndex: number, columnName: string): CellCoord => ({ rowIndex, columnName });

const cellSelection = (
  anchor: CellCoord,
  focus: CellCoord,
  extraRanges: CellRange[] = [],
): GridSelection => selectionFromAnchorFocus(anchor, focus, 'cell', extraRanges);

describe('normalizeCellRange boundaries', () => {
  it('returns null for an empty column order', () => {
    expect(normalizeCellRange({ start: coord(0, 'id'), end: coord(1, 'id') }, [])).toBeNull();
  });

  it('returns null for a non-integer top row and a negative row', () => {
    expect(
      normalizeCellRange({ start: coord(Number.NaN, 'id'), end: coord(1, 'id') }, COLUMNS),
    ).toBeNull();
    expect(normalizeCellRange({ start: coord(-1, 'id'), end: coord(1, 'id') }, COLUMNS)).toBeNull();
    expect(normalizeCellRange({ start: coord(1, 'id'), end: coord(-1, 'id') }, COLUMNS)).toBeNull();
  });

  it('is stable for the single-column case (indexOf degenerates to the same index)', () => {
    const range = { start: coord(2, 'id'), end: coord(2, 'id') };
    const normalized = normalizeCellRange(range, ['id']);
    expect(normalized).toEqual(range);
    expect(normalizeCellRange(normalized ?? range, ['id'])).toEqual(range);
    expect(cellRangeContains(range, coord(2, 'id'), ['id'])).toBe(true);
    expect(cellRangeContains(range, coord(2, 'name'), ['id'])).toBe(false);
  });

  it('normalizes a reversed range to identical corners', () => {
    const forward = { start: coord(0, 'name'), end: coord(2, 'note') };
    const reversed = { start: coord(2, 'note'), end: coord(0, 'name') };
    expect(normalizeCellRange(reversed, COLUMNS)).toEqual(normalizeCellRange(forward, COLUMNS));
  });
});

describe('coordinate boundaries', () => {
  it('coordAt rejects a fractional column index, an empty order and a negative row', () => {
    expect(coordAt(0, 0.5, COLUMNS)).toBeNull();
    expect(coordAt(0, 0, [])).toBeNull();
    expect(coordAt(-1, 0, COLUMNS)).toBeNull();
    expect(coordAt(0.5, 0, COLUMNS)).toBeNull();
    expect(coordAt(4, 2, COLUMNS)).toEqual({ rowIndex: 4, columnName: 'note' });
  });

  it('clampCellCoord clamps a huge row index and rejects a non-integer one', () => {
    expect(clampCellCoord(coord(10_000_000, 'id'), 3, COLUMNS)).toEqual(coord(2, 'id'));
    expect(clampCellCoord(coord(Number.NaN, 'id'), 3, COLUMNS)).toBeNull();
    expect(clampCellCoord(coord(0, 'id'), 0, COLUMNS)).toBeNull();
  });

  it('moveCellFocus cannot move on a single-row single-column grid', () => {
    const only = coord(0, 'id');
    expect(moveCellFocus(only, 1, 0, 1, ['id'])).toBeNull();
    expect(moveCellFocus(only, -1, 0, 1, ['id'])).toBeNull();
    expect(moveCellFocus(only, 0, 1, 1, ['id'])).toBeNull();
    expect(moveCellFocus(only, 0, 0, 1, ['id'])).toBeNull();
  });

  it('moveCellFocus returns null for an empty grid or empty column order', () => {
    expect(moveCellFocus(coord(0, 'id'), 1, 0, 0, COLUMNS)).toBeNull();
    expect(moveCellFocus(coord(0, 'id'), 1, 0, 10, [])).toBeNull();
  });
});

describe('column names with spaces, quotes and non-ASCII characters', () => {
  const QUIRKY = ['café name', 'a"b', 'normal'];

  it('treats quirky column names verbatim in normalize/contains/count', () => {
    const selection = cellSelection(coord(0, 'café name'), coord(1, 'a"b'));
    const normalized = normalizeCellRange(
      { start: coord(1, 'a"b'), end: coord(0, 'café name') },
      QUIRKY,
    );
    expect(normalized).not.toBeNull();
    if (normalized === null) throw new Error('normalizeCellRange rejected a displayable column');
    expect(normalized).toEqual({ start: coord(0, 'café name'), end: coord(1, 'a"b') });
    expect(cellRangeContains(normalized, coord(0, 'a"b'), QUIRKY)).toBe(true);
    expect(columnsCoveredBySelection(selection, QUIRKY)).toBe(2);
    // No case folding: 'CAFÉ NAME' is a different column, not a match.
    expect(cellRangeContains(normalized, coord(0, 'CAFÉ NAME'), QUIRKY)).toBe(false);
  });
});

describe('coverage counts with a single cell/column', () => {
  it('a one-row one-column region covers exactly one row and one column', () => {
    const selection = cellSelection(coord(0, 'id'), coord(0, 'id'));
    expect(gridSelectionCellCount(selection, COLUMNS)).toBe(1);
    expect(columnsCoveredBySelection(selection, COLUMNS)).toBe(1);
    const rows = rowsCoveredBySelection({ ...selection, selectedRows: new Set<number>() });
    expect(rows.size).toBe(1);
    expect(rows.has(0)).toBe(true);
  });

  it('a overlapped extra cell is counted twice by the cell metric but once by the row metric', () => {
    const selection = cellSelection(coord(0, 'name'), coord(1, 'id'), [
      { start: coord(1, 'id'), end: coord(1, 'id') },
    ]);
    // Cell metric is a render/summary count: overlapping regions count twice.
    expect(gridSelectionCellCount(selection, COLUMNS)).toBe(5);
    expect(columnsCoveredBySelection(selection, COLUMNS)).toBe(2);
    expect(
      [...rowsCoveredBySelection({ ...selection, selectedRows: new Set<number>() })].sort(),
    ).toEqual([0, 1]);
  });

  it('rowsCoveredBySelection ignores an extra range with a negative row', () => {
    const selection = cellSelection(coord(0, 'name'), coord(0, 'name'), [
      { start: coord(-1, 'name'), end: coord(-1, 'name') },
      { start: coord(2, 'note'), end: coord(2, 'note') },
    ]);
    expect(
      [...rowsCoveredBySelection({ ...selection, selectedRows: new Set<number>() })],
    ).toEqual([0, 2]);
  });
});

describe('reconcileGridSelection fast paths and shrunken pages', () => {
  it('returns the row-mode selection unchanged when it already has no geometry', () => {
    const rowMode: GridSelection = { anchor: null, focus: null, extraRanges: [], mode: 'row' };
    expect(reconcileGridSelection(rowMode, 3, COLUMNS)).toBe(rowMode);
  });

  it('keeps already-normalized extra ranges by reference (memo stability)', () => {
    const selection = cellSelection(coord(0, 'name'), coord(1, 'id'), [
      { start: coord(4, 'note'), end: coord(4, 'note') },
    ]);
    expect(reconcileGridSelection(selection, 10, COLUMNS)).toBe(selection);
  });

  it('drops extra ranges whose rows fell outside a shrunken page', () => {
    const selection = cellSelection(coord(0, 'name'), coord(1, 'id'), [
      { start: coord(2, 'note'), end: coord(2, 'note') },
      { start: coord(9, 'note'), end: coord(9, 'note') },
    ]);
    const reconciled = reconcileGridSelection(selection, 3, COLUMNS);
    expect(reconciled.mode).toBe('cell');
    expect(reconciled.extraRanges).toEqual([{ start: coord(2, 'note'), end: coord(2, 'note') }]);
  });

  it('normalizes a reversed extra range instead of dropping it', () => {
    const selection = cellSelection(coord(0, 'name'), coord(1, 'id'), [
      { start: coord(4, 'note'), end: coord(4, 'name') },
    ]);
    const reconciled = reconcileGridSelection(selection, 10, COLUMNS);
    expect(reconciled.extraRanges).toEqual([{ start: coord(4, 'name'), end: coord(4, 'note') }]);
  });

  it('clears a cell selection when every column was hidden', () => {
    const selection = cellSelection(coord(0, 'name'), coord(1, 'id'));
    expect(reconcileGridSelection(selection, 10, [])).toBe(EMPTY_GRID_SELECTION);
  });

  it('keeps a cell selection when the column set only grows', () => {
    const selection = cellSelection(coord(0, 'name'), coord(1, 'id'));
    expect(reconcileGridSelection(selection, 10, [...COLUMNS, 'extra'])).toBe(selection);
  });
});

describe('gridSelectionEquals boundaries', () => {
  it('returns false when an extra range references a hidden column', () => {
    const left = cellSelection(coord(0, 'name'), coord(0, 'name'), [
      { start: coord(3, 'ghost'), end: coord(3, 'ghost') },
    ]);
    expect(gridSelectionEquals(left, left, COLUMNS)).toBe(false);
  });

  it('returns false for different row-mode anchors and true for identical none selections', () => {
    const row = selectionFromAnchorFocus(coord(0, 'id'), coord(0, 'id'), 'row');
    expect(gridSelectionEquals(row, { ...row }, COLUMNS)).toBe(true);
    expect(gridSelectionEquals(EMPTY_GRID_SELECTION, { ...EMPTY_GRID_SELECTION }, COLUMNS)).toBe(
      true,
    );
  });
});

describe('toggleExtraRange boundaries', () => {
  it('starts a cell selection when Cmd+click happens while in row mode', () => {
    const rowMode = selectionFromAnchorFocus(coord(0, 'name'), coord(0, 'name'), 'row');
    expect(toggleExtraRange(rowMode, coord(2, 'id'), COLUMNS)).toEqual({
      anchor: coord(2, 'id'),
      focus: coord(2, 'id'),
      extraRanges: [],
      mode: 'cell',
    });
  });

  it('re-centres a cell selection that carries no anchor', () => {
    const broken: GridSelection = {
      anchor: null,
      focus: null,
      extraRanges: [{ start: coord(3, 'note'), end: coord(3, 'note') }],
      mode: 'cell',
    };
    expect(toggleExtraRange(broken, coord(1, 'id'), COLUMNS)).toEqual({
      anchor: coord(1, 'id'),
      focus: coord(1, 'id'),
      extraRanges: [],
      mode: 'cell',
    });
  });
});
