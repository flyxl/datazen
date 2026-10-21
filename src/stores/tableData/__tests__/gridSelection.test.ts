import { describe, expect, it } from 'vitest';
import {
  EMPTY_GRID_SELECTION,
  cellCoordEquals,
  cellRangeContains,
  clampCellCoord,
  columnIndexFromCoord,
  columnsCoveredBySelection,
  coordAt,
  gridSelectionCellCount,
  gridSelectionContains,
  gridSelectionEquals,
  moveCellFocus,
  normalizeCellRange,
  normalizeSelectionRange,
  reconcileGridSelection,
  rowsCoveredBySelection,
  selectionFromAnchorFocus,
  selectionRanges,
  toggleExtraRange,
  type CellCoord,
  type CellRange,
  type GridSelection,
} from '../gridSelection';

/** Render order deliberately disagrees with lexicographic order (`id` < `name`). */
const COLUMNS = ['name', 'id', 'note'];

const coord = (rowIndex: number, columnName: string): CellCoord => ({ rowIndex, columnName });

const cellSelection = (
  anchor: CellCoord,
  focus: CellCoord,
  extraRanges: CellRange[] = [],
): GridSelection => selectionFromAnchorFocus(anchor, focus, 'cell', extraRanges);

describe('EMPTY_GRID_SELECTION', () => {
  it('is the frozen `none` baseline shared by every empty selection site', () => {
    expect(Object.isFrozen(EMPTY_GRID_SELECTION)).toBe(true);
    expect(Object.isFrozen(EMPTY_GRID_SELECTION.extraRanges)).toBe(true);
    expect(EMPTY_GRID_SELECTION.mode).toBe('none');
    expect(EMPTY_GRID_SELECTION.anchor).toBeNull();
    expect(EMPTY_GRID_SELECTION.focus).toBeNull();
    expect(EMPTY_GRID_SELECTION.extraRanges).toHaveLength(0);
  });
});

describe('normalizeCellRange', () => {
  it('normalizes a bottom-up drag so rows grow downwards', () => {
    const normalized = normalizeCellRange(
      { start: coord(2, 'name'), end: coord(0, 'id') },
      COLUMNS,
    );
    expect(normalized).toEqual({ start: coord(0, 'name'), end: coord(2, 'id') });
  });

  it('normalizes columns by render order, not by column name', () => {
    // `id` sorts before `name` lexicographically but is displayed second.
    const normalized = normalizeCellRange(
      { start: coord(1, 'id'), end: coord(1, 'name') },
      COLUMNS,
    );
    expect(normalized).toEqual({ start: coord(1, 'name'), end: coord(1, 'id') });
    expect(columnIndexFromCoord(coord(1, 'name'), COLUMNS)).toBe(0);
  });

  it('keeps a single-cell range unchanged', () => {
    const range: CellRange = { start: coord(0, 'note'), end: coord(0, 'note') };
    expect(normalizeCellRange(range, COLUMNS)).toEqual(range);
  });

  it('returns null for an unknown column instead of guessing a position', () => {
    expect(
      normalizeCellRange({ start: coord(0, 'ghost'), end: coord(1, 'id') }, COLUMNS),
    ).toBeNull();
  });

  it('returns null for a negative row', () => {
    expect(
      normalizeCellRange({ start: coord(-1, 'name'), end: coord(1, 'name') }, COLUMNS),
    ).toBeNull();
  });
});

describe('coordinates', () => {
  it('clampCellCoord clamps rows into the grid', () => {
    expect(clampCellCoord(coord(-4, 'id'), 3, COLUMNS)).toEqual(coord(0, 'id'));
    expect(clampCellCoord(coord(99, 'id'), 3, COLUMNS)).toEqual(coord(2, 'id'));
  });

  it('clampCellCoord rejects hidden columns and an empty grid', () => {
    expect(clampCellCoord(coord(0, 'ghost'), 3, COLUMNS)).toBeNull();
    expect(clampCellCoord(coord(0, 'id'), 0, COLUMNS)).toBeNull();
  });

  it('coordAt maps indices to a coordinate and rejects out-of-range input', () => {
    expect(coordAt(2, 1, COLUMNS)).toEqual(coord(2, 'id'));
    expect(coordAt(-1, 0, COLUMNS)).toBeNull();
    expect(coordAt(0, COLUMNS.length, COLUMNS)).toBeNull();
  });

  it('cellCoordEquals treats two nulls as equal and null/coord as different', () => {
    expect(cellCoordEquals(null, null)).toBe(true);
    expect(cellCoordEquals(coord(0, 'id'), null)).toBe(false);
    expect(cellCoordEquals(coord(0, 'id'), coord(0, 'id'))).toBe(true);
    expect(cellCoordEquals(coord(0, 'id'), coord(0, 'name'))).toBe(false);
  });
});

describe('moveCellFocus', () => {
  it('moves by row and column deltas', () => {
    expect(moveCellFocus(coord(1, 'name'), 1, 0, 5, COLUMNS)).toEqual(coord(2, 'name'));
    expect(moveCellFocus(coord(1, 'name'), 0, 1, 5, COLUMNS)).toEqual(coord(1, 'id'));
  });

  it('returns null when the clamped result would not move', () => {
    expect(moveCellFocus(coord(0, 'name'), -1, 0, 5, COLUMNS)).toBeNull();
    expect(moveCellFocus(coord(4, 'id'), 1, 0, 5, COLUMNS)).toBeNull();
    expect(moveCellFocus(coord(1, 'note'), 0, 1, 5, COLUMNS)).toBeNull();
  });

  it('clamps the column while still moving to another row', () => {
    expect(moveCellFocus(coord(1, 'note'), 1, 1, 5, COLUMNS)).toEqual(coord(2, 'note'));
  });

  it('returns null for an empty grid or a hidden focus column', () => {
    expect(moveCellFocus(coord(0, 'id'), 1, 0, 0, COLUMNS)).toBeNull();
    expect(moveCellFocus(coord(0, 'ghost'), 1, 0, 5, COLUMNS)).toBeNull();
  });
});

describe('selectionFromAnchorFocus', () => {
  it('builds a cell selection and copies the extra ranges', () => {
    const extra: CellRange[] = [{ start: coord(4, 'note'), end: coord(4, 'note') }];
    const selection = cellSelection(coord(0, 'name'), coord(1, 'id'), extra);
    expect(selection).toEqual({
      anchor: coord(0, 'name'),
      focus: coord(1, 'id'),
      extraRanges: extra,
      mode: 'cell',
    });
    expect(selection.extraRanges).not.toBe(extra);
  });

  it('drops all geometry in row mode so the row set stays the only truth (invariant I2)', () => {
    const selection = selectionFromAnchorFocus(coord(0, 'name'), coord(2, 'id'), 'row', [
      { start: coord(5, 'note'), end: coord(5, 'note') },
    ]);
    expect(selection).toEqual({ anchor: null, focus: null, extraRanges: [], mode: 'row' });
  });
});

describe('containment and counts', () => {
  const main = cellSelection(coord(0, 'name'), coord(1, 'id'));
  const withExtra = cellSelection(coord(0, 'name'), coord(1, 'id'), [
    { start: coord(5, 'note'), end: coord(5, 'note') },
  ]);

  it('finds cells inside the main region and the extra regions', () => {
    expect(gridSelectionContains(main, coord(1, 'id'), COLUMNS)).toBe(true);
    expect(gridSelectionContains(main, coord(1, 'note'), COLUMNS)).toBe(false);
    expect(gridSelectionContains(withExtra, coord(5, 'note'), COLUMNS)).toBe(true);
    expect(gridSelectionContains(main, coord(5, 'note'), COLUMNS)).toBe(false);
  });

  it('never reports a cell for none/row mode or an unknown column', () => {
    expect(gridSelectionContains(EMPTY_GRID_SELECTION, coord(0, 'id'), COLUMNS)).toBe(false);
    expect(
      gridSelectionContains(
        selectionFromAnchorFocus(coord(0, 'id'), coord(2, 'id'), 'row'),
        coord(0, 'id'),
        COLUMNS,
      ),
    ).toBe(false);
    expect(gridSelectionContains(main, coord(0, 'ghost'), COLUMNS)).toBe(false);
  });

  it('counts the main region plus every extra region', () => {
    expect(gridSelectionCellCount(main, COLUMNS)).toBe(4);
    expect(gridSelectionCellCount(withExtra, COLUMNS)).toBe(5);
  });

  it('counts distinct covered columns across regions', () => {
    expect(columnsCoveredBySelection(withExtra, COLUMNS)).toBe(3);
    expect(columnsCoveredBySelection(EMPTY_GRID_SELECTION, COLUMNS)).toBe(0);
  });

  it('lists the main region first and skips extras for hidden columns', () => {
    const selection = cellSelection(coord(0, 'name'), coord(0, 'id'), [
      { start: coord(3, 'note'), end: coord(3, 'note') },
      { start: coord(4, 'ghost'), end: coord(4, 'ghost') },
    ]);
    expect(selectionRanges(selection, COLUMNS)).toEqual([
      { start: coord(0, 'name'), end: coord(0, 'id') },
      { start: coord(3, 'note'), end: coord(3, 'note') },
    ]);
  });

  it('cellRangeContains resolves the corners through render order', () => {
    expect(
      cellRangeContains({ start: coord(0, 'id'), end: coord(2, 'name') }, coord(1, 'name'), COLUMNS),
    ).toBe(true);
    expect(
      cellRangeContains({ start: coord(0, 'name'), end: coord(0, 'id') }, coord(1, 'name'), COLUMNS),
    ).toBe(false);
  });
});

describe('rowsCoveredBySelection', () => {
  it('derives the row union for cell mode without duplicates', () => {
    const selection = cellSelection(coord(0, 'name'), coord(2, 'id'), [
      { start: coord(2, 'note'), end: coord(2, 'note') },
      { start: coord(5, 'note'), end: coord(5, 'note') },
    ]);
    const rows = rowsCoveredBySelection(selection);
    expect([...rows].sort((left, right) => left - right)).toEqual([0, 1, 2, 5]);
    expect(rows.has(3)).toBe(false);
  });

  it('returns the passed selectedRows verbatim in row mode', () => {
    const selectedRows = new Set([2, 7]);
    const selection: GridSelection & { selectedRows: ReadonlySet<number> } = {
      ...selectionFromAnchorFocus(coord(0, 'name'), coord(0, 'name'), 'row'),
      selectedRows,
    };
    expect(rowsCoveredBySelection(selection)).toBe(selectedRows);
  });

  it('is empty for none mode and for row mode without a row set', () => {
    expect(rowsCoveredBySelection(EMPTY_GRID_SELECTION).size).toBe(0);
    expect(
      rowsCoveredBySelection(selectionFromAnchorFocus(coord(0, 'id'), coord(0, 'id'), 'row')).size,
    ).toBe(0);
  });
});

describe('gridSelectionEquals', () => {
  it('ignores extra-range order and coordinate normalization', () => {
    const extras: CellRange[] = [
      { start: coord(0, 'name'), end: coord(0, 'name') },
      { start: coord(3, 'note'), end: coord(3, 'note') },
    ];
    const first = cellSelection(coord(0, 'id'), coord(1, 'id'), extras);
    const second = cellSelection(coord(0, 'id'), coord(1, 'id'), [...extras].reverse());
    expect(gridSelectionEquals(first, second, COLUMNS)).toBe(true);
  });

  it('detects mode, anchor and extra-range differences', () => {
    const base = cellSelection(coord(0, 'id'), coord(1, 'id'));
    expect(gridSelectionEquals(base, EMPTY_GRID_SELECTION, COLUMNS)).toBe(false);
    expect(gridSelectionEquals(base, cellSelection(coord(1, 'id'), coord(1, 'id')), COLUMNS)).toBe(
      false,
    );
    expect(
      gridSelectionEquals(
        base,
        cellSelection(coord(0, 'id'), coord(1, 'id'), [
          { start: coord(4, 'note'), end: coord(4, 'note') },
        ]),
        COLUMNS,
      ),
    ).toBe(false);
  });
});

describe('toggleExtraRange', () => {
  const base = cellSelection(coord(0, 'name'), coord(1, 'id'));

  it('adds a non-contiguous cell and removes it on the second click', () => {
    const added = toggleExtraRange(base, coord(6, 'note'), COLUMNS);
    expect(added.extraRanges).toHaveLength(1);
    expect(gridSelectionContains(added, coord(6, 'note'), COLUMNS)).toBe(true);
    const removed = toggleExtraRange(added, coord(6, 'note'), COLUMNS);
    expect(removed.extraRanges).toHaveLength(0);
  });

  it('is a no-op for a cell already inside the main region', () => {
    expect(toggleExtraRange(base, coord(1, 'name'), COLUMNS)).toBe(base);
  });

  it('is a no-op for an unknown column', () => {
    expect(toggleExtraRange(base, coord(0, 'ghost'), COLUMNS)).toBe(base);
  });

  it('starts a cell selection when none exists yet', () => {
    const selection = toggleExtraRange(EMPTY_GRID_SELECTION, coord(2, 'id'), COLUMNS);
    expect(selection).toEqual({
      anchor: coord(2, 'id'),
      focus: coord(2, 'id'),
      extraRanges: [],
      mode: 'cell',
    });
  });
});

describe('reconcileGridSelection', () => {
  it('returns the same object when nothing changed (memo stability)', () => {
    const selection = cellSelection(coord(0, 'name'), coord(1, 'id'));
    expect(reconcileGridSelection(selection, 10, COLUMNS)).toBe(selection);
  });

  it('clears the selection when a main corner column is hidden', () => {
    const selection = cellSelection(coord(0, 'name'), coord(1, 'id'));
    expect(reconcileGridSelection(selection, 10, ['name', 'note'])).toBe(EMPTY_GRID_SELECTION);
  });

  it('clears the selection when a main corner row falls outside the page', () => {
    const selection = cellSelection(coord(0, 'name'), coord(8, 'id'));
    expect(reconcileGridSelection(selection, 5, COLUMNS)).toBe(EMPTY_GRID_SELECTION);
  });

  it('drops only the extra ranges that reference hidden columns', () => {
    const selection = cellSelection(coord(0, 'name'), coord(1, 'id'), [
      { start: coord(4, 'note'), end: coord(4, 'note') },
      { start: coord(5, 'ghost'), end: coord(5, 'ghost') },
    ]);
    const reconciled = reconcileGridSelection(selection, 10, COLUMNS);
    expect(reconciled.mode).toBe('cell');
    expect(reconciled.extraRanges).toEqual([{ start: coord(4, 'note'), end: coord(4, 'note') }]);
  });

  it('normalizes row mode geometry away and keeps row mode itself', () => {
    const dirty: GridSelection = {
      anchor: coord(0, 'name'),
      focus: coord(0, 'name'),
      extraRanges: [{ start: coord(0, 'name'), end: coord(0, 'name') }],
      mode: 'row',
    };
    expect(reconcileGridSelection(dirty, 10, COLUMNS)).toEqual({
      anchor: null,
      focus: null,
      extraRanges: [],
      mode: 'row',
    });
  });

  it('collapses none mode onto the shared empty selection', () => {
    const stale: GridSelection = {
      anchor: null,
      focus: null,
      extraRanges: [],
      mode: 'none',
    };
    expect(reconcileGridSelection(stale, 10, COLUMNS)).toBe(EMPTY_GRID_SELECTION);
  });
});

describe('invariants I1-I3', () => {
  it('none and row selections never carry geometry', () => {
    expect(EMPTY_GRID_SELECTION.anchor).toBeNull();
    expect(EMPTY_GRID_SELECTION.focus).toBeNull();
    expect(EMPTY_GRID_SELECTION.extraRanges).toHaveLength(0);
    const row = selectionFromAnchorFocus(coord(2, 'id'), coord(2, 'id'), 'row');
    expect(row.anchor).toBeNull();
    expect(row.focus).toBeNull();
    expect(row.extraRanges).toHaveLength(0);
    expect(row.mode).toBe('row');
  });

  it('cell selections always carry both corners', () => {
    const selection = cellSelection(coord(0, 'id'), coord(3, 'note'));
    expect(selection.mode).toBe('cell');
    expect(selection.anchor).not.toBeNull();
    expect(selection.focus).not.toBeNull();
    expect(normalizeSelectionRange(selection, COLUMNS)).toEqual({
      start: coord(0, 'id'),
      end: coord(3, 'note'),
    });
  });
});
