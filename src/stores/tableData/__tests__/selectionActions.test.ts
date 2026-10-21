import { describe, expect, it, vi } from 'vitest';
import type { TableState } from '../types';
import { emptyTableState } from '../connectionState';
import {
  EMPTY_GRID_SELECTION,
  selectionFromAnchorFocus,
  type CellCoord,
  type GridSelection,
} from '../gridSelection';
import {
  ROW_MODE_GRID_SELECTION,
  applyBeginCellSelection,
  applyClearGridSelection,
  applyGridSelection,
  applyReconcileSelection,
  applyRowModeTransition,
  applyRowSelect,
  applyToggleSelectAll,
  createSelectionSlice,
  displayedColumnNames,
  panelColumnOrder,
  rowIndexSelectionReset,
} from '../selectionActions';

const coord = (rowIndex: number, columnName: string): CellCoord => ({ rowIndex, columnName });

function tableState(overrides: Partial<TableState> = {}): TableState {
  return {
    ...emptyTableState(),
    columns: [
      { name: 'id', dataType: 'integer', nullable: false, isPrimaryKey: true },
      { name: 'name', dataType: 'text', nullable: true, isPrimaryKey: false },
      { name: 'note', dataType: 'text', nullable: true, isPrimaryKey: false },
    ],
    rows: [{ id: 1, name: 'Alice', note: null }, { id: 2, name: 'Bob', note: 'x' }, { id: 3, name: 'Cara', note: null }],
    ...overrides,
  };
}

describe('applyRowSelect (moved verbatim from the store)', () => {
  it('selects a single row on a plain click', () => {
    const next = applyRowSelect(tableState(), 1, undefined);
    expect(next.selectedRows).toEqual(new Set([1]));
    expect(next.lastSelectedIndex).toBe(1);
  });

  it('clears the selection when the only selected row is clicked again', () => {
    const next = applyRowSelect(tableState({ selectedRows: new Set([1]), lastSelectedIndex: 1 }), 1, undefined);
    expect(next.selectedRows).toEqual(new Set());
    expect(next.lastSelectedIndex).toBeNull();
  });

  it('extends a contiguous range from the anchor row', () => {
    const next = applyRowSelect(
      tableState({ selectedRows: new Set([2]), lastSelectedIndex: 2 }),
      0,
      { range: true },
    );
    expect([...(next.selectedRows ?? [])].sort((left, right) => left - right)).toEqual([0, 1, 2]);
    expect(next.lastSelectedIndex).toBe(0);
  });

  it('falls back to a single row when a range click has no anchor yet', () => {
    const next = applyRowSelect(tableState(), 2, { range: true });
    expect(next.selectedRows).toEqual(new Set([2]));
    expect(next.lastSelectedIndex).toBe(2);
  });

  it('toggles one row in multi mode', () => {
    const ts = tableState({ selectedRows: new Set([0, 1]), lastSelectedIndex: 1 });
    expect(applyRowSelect(ts, 0, { multi: true }).selectedRows).toEqual(new Set([1]));
    expect(applyRowSelect(ts, 2, { multi: true }).selectedRows).toEqual(new Set([0, 1, 2]));
  });
});

describe('applyToggleSelectAll', () => {
  it('selects every row of the page and clears lastSelectedIndex', () => {
    const next = applyToggleSelectAll(tableState());
    expect(next.selectedRows).toEqual(new Set([0, 1, 2]));
    expect(next.lastSelectedIndex).toBeNull();
  });

  it('clears a full selection', () => {
    const next = applyToggleSelectAll(
      tableState({ selectedRows: new Set([0, 1, 2]), lastSelectedIndex: 0 }),
    );
    expect(next.selectedRows).toEqual(new Set());
    expect(next.lastSelectedIndex).toBeNull();
  });

  it('keeps an empty page empty', () => {
    const next = applyToggleSelectAll(tableState({ rows: [] }));
    expect(next.selectedRows).toEqual(new Set());
  });
});

describe('cell/row mode transitions', () => {
  it('beginCellSelection enters cell mode and empties the row set', () => {
    const next = applyBeginCellSelection(
      tableState({ selectedRows: new Set([1]), lastSelectedIndex: 1 }),
      coord(0, 'name'),
    );
    expect(next.gridSelection).toEqual({
      anchor: coord(0, 'name'),
      focus: coord(0, 'name'),
      extraRanges: [],
      mode: 'cell',
    });
    expect(next.selectedRows).toEqual(new Set());
    expect(next.lastSelectedIndex).toBeNull();
  });

  it('applyRowModeTransition switches to row mode while keeping the row semantics', () => {
    const next = applyRowModeTransition(tableState(), 2, { multi: true });
    expect(next.gridSelection).toBe(ROW_MODE_GRID_SELECTION);
    expect(next.selectedRows).toEqual(new Set([2]));
  });

  it('row mode stays valid when the clicked row is deselected (empty row set)', () => {
    const next = applyRowModeTransition(
      tableState({ selectedRows: new Set([1]), lastSelectedIndex: 1 }),
      1,
      undefined,
    );
    expect(next.gridSelection).toBe(ROW_MODE_GRID_SELECTION);
    expect(next.selectedRows).toEqual(new Set());
  });

  it('setGridSelection derives the row set from the mode, so no stale highlight survives', () => {
    const cell: GridSelection = selectionFromAnchorFocus(coord(0, 'id'), coord(1, 'id'), 'cell');
    const withCell = applyGridSelection(tableState({ selectedRows: new Set([0]) }), cell);
    expect(withCell.gridSelection).toBe(cell);
    expect(withCell.selectedRows).toEqual(new Set());

    // A bare row-mode value carries no row list: it means "row mode, nothing selected yet",
    // and only `applyRowModeTransition` may pair a non-empty row set with the mode (D4).
    const withRow = applyGridSelection(
      tableState({ selectedRows: new Set([0]) }),
      ROW_MODE_GRID_SELECTION,
    );
    expect(withRow.gridSelection).toBe(ROW_MODE_GRID_SELECTION);
    expect(withRow.selectedRows).toEqual(new Set());
    expect(withRow.lastSelectedIndex).toBeNull();
  });

  it('setting the empty selection is the same transition as clearing (hook Escape path)', () => {
    const next = applyGridSelection(
      tableState({ selectedRows: new Set([2]), lastSelectedIndex: 2 }),
      EMPTY_GRID_SELECTION,
    );
    expect(next).toEqual({
      gridSelection: EMPTY_GRID_SELECTION,
      selectedRows: new Set(),
      lastSelectedIndex: null,
    });
  });

  it('clearGridSelection drops geometry, rows and the anchor together', () => {
    const next = applyClearGridSelection(tableState({ selectedRows: new Set([1]), lastSelectedIndex: 1 }));
    expect(next.gridSelection).toBe(EMPTY_GRID_SELECTION);
    expect(next.selectedRows).toEqual(new Set());
    expect(next.lastSelectedIndex).toBeNull();
  });
});

describe('reconciliation', () => {
  it('clears the selection when a main corner column is hidden', () => {
    const selection = selectionFromAnchorFocus(coord(0, 'name'), coord(1, 'id'), 'cell');
    const next = applyReconcileSelection(tableState({ gridSelection: selection }), ['id', 'note']);
    expect(next.gridSelection).toBe(EMPTY_GRID_SELECTION);
  });

  it('keeps a selection whose columns and rows all still exist', () => {
    const selection = selectionFromAnchorFocus(coord(0, 'name'), coord(1, 'id'), 'cell');
    const next = applyReconcileSelection(tableState({ gridSelection: selection }), [
      'id',
      'name',
      'note',
    ]);
    expect(next.gridSelection).toBe(selection);
  });
});

describe('rowIndexSelectionReset', () => {
  it('packages all three row-indexed fields and hands out fresh sets', () => {
    const first = rowIndexSelectionReset();
    const second = rowIndexSelectionReset();
    expect(first.gridSelection).toBe(EMPTY_GRID_SELECTION);
    expect(first.lastSelectedIndex).toBeNull();
    expect(first.selectedRows).toEqual(new Set());
    expect(first.selectedRows).not.toBe(second.selectedRows);
  });
});

describe('displayed column order', () => {
  const columns = [{ name: 'id' }, { name: 'name' }, { name: 'note' }];

  it('returns every column when no visibility filter is set', () => {
    expect(displayedColumnNames(columns, null)).toEqual(['id', 'name', 'note']);
  });

  it('keeps the original order while filtering hidden columns', () => {
    expect(displayedColumnNames(columns, ['note', 'id'])).toEqual(['id', 'note']);
  });

  it('panelColumnOrder reads the slice it is given', () => {
    expect(panelColumnOrder(tableState({ visibleColumns: ['name'] }))).toEqual(['name']);
  });
});

describe('createSelectionSlice', () => {
  it('routes every panel action through the panel-scoped patch', () => {
    const patched: Array<{ panelId: string; partial: Partial<TableState> }> = [];
    const slice = createSelectionSlice((panelId, updater) => {
      patched.push({ panelId, partial: updater(tableState()) });
    });

    slice.selectRow('panel-a', 1);
    slice.toggleSelectAll('panel-a');
    slice.beginCellSelection('panel-a', coord(0, 'id'));
    slice.setGridSelection('panel-a', EMPTY_GRID_SELECTION);
    slice.clearGridSelection('panel-a');
    slice.reconcileGridSelectionForColumns('panel-a', ['id']);

    expect(patched).toHaveLength(6);
    expect(patched.every((call) => call.panelId === 'panel-a')).toBe(true);
    expect(patched[0]?.partial.selectedRows).toEqual(new Set([1]));
    expect(patched[2]?.partial.gridSelection).toEqual({
      anchor: coord(0, 'id'),
      focus: coord(0, 'id'),
      extraRanges: [],
      mode: 'cell',
    });
    expect(patched[4]?.partial.gridSelection).toBe(EMPTY_GRID_SELECTION);
  });

  it('uses the patch callback once per action', () => {
    const patch = vi.fn();
    const slice = createSelectionSlice(patch);
    slice.toggleSelectAll('panel');
    expect(patch).toHaveBeenCalledTimes(1);
    expect(patch.mock.calls[0]?.[0]).toBe('panel');
  });

  it('selectRow enters row mode, so the gutter path reaches mode "row" (D4)', () => {
    const patched: Array<Partial<TableState>> = [];
    const slice = createSelectionSlice((_panelId, updater) => {
      patched.push(updater(tableState()));
    });

    slice.selectRow('panel', 0);

    expect(patched[0]?.selectedRows).toEqual(new Set([0]));
    expect(patched[0]?.gridSelection?.mode).toBe('row');
    expect(patched[0]?.gridSelection?.anchor).toBeNull();
    expect(patched[0]?.gridSelection?.focus).toBeNull();
  });

  it('toggleSelectAll also switches to row mode', () => {
    const patched: Array<Partial<TableState>> = [];
    const slice = createSelectionSlice((_panelId, updater) => {
      patched.push(updater(tableState()));
    });

    slice.toggleSelectAll('panel');

    expect(patched[0]?.selectedRows).toEqual(new Set([0, 1, 2]));
    expect(patched[0]?.gridSelection?.mode).toBe('row');
  });

  it('setRowModeSelection enters row mode and leaving it clears the rows too', () => {
    const patched: Array<Partial<TableState>> = [];
    const state = tableState({ selectedRows: new Set([1]), lastSelectedIndex: 1 });
    const slice = createSelectionSlice((_panelId, updater) => {
      patched.push(updater(state));
    });

    slice.setRowModeSelection('panel', 2, { range: true });
    expect(patched[0]?.selectedRows).toEqual(new Set([1, 2]));
    expect(patched[0]?.gridSelection?.mode).toBe('row');

    slice.setRowModeSelection('panel', null);
    expect(patched[1]).toEqual({
      gridSelection: EMPTY_GRID_SELECTION,
      selectedRows: new Set(),
      lastSelectedIndex: null,
    });
  });
});
