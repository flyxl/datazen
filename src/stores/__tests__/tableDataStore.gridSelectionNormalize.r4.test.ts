/**
 * DB-01 round-4 edge coverage for I2 geometry normalisation in `applyGridSelection`
 * (00-contracts §4: only `cell` may carry geometry; `row`/`none` must be null) plus the
 * shared-constant aliasing risk of `ROW_MODE_GRID_SELECTION`.
 *
 * `setGridSelection` is the only public entry that accepts a caller-built `GridSelection`,
 * so it must not trust the caller. These cases pin the behaviour instead of the diff:
 * - dirty `row`/`none` selections are normalised (geometry dropped, never persisted);
 * - a legitimate `cell` selection keeps its geometry exactly (the normalisation must not
 *   over-reach and clear cell, which would silently break cell selection);
 * - `row` + empty rows stays legal (no fallback to `none`);
 * - the canonical row-mode constant is shared by reference across panels, so every action
 *   is checked to leave it untouched — a single stray `.push`/assignment would corrupt all
 *   row-mode selections everywhere.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { GridSelection } from '../tableData/gridSelection';
import type { TableState } from '../tableData/types';

// Resolved inside `beforeEach` from the freshly reset module graph: `vi.resetModules()` gives
// the store its own copy of the frozen constants, so identity assertions must use those.
let EMPTY_GRID_SELECTION: GridSelection;
let ROW_MODE_GRID_SELECTION: GridSelection;

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));

const PANEL_A = 'panel-normalize-r4-a';
const PANEL_B = 'panel-normalize-r4-b';

const response = {
  columns: [
    { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
    { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
  ],
  rows: [
    [1, 'Alice'],
    [2, 'Bob'],
  ] as (string | number | null)[][],
  totalRows: 2,
  page: 0,
  pageSize: 50,
};

/** Dirty row-mode selection: I2 forbids every piece of geometry here. */
function dirtyRowSelection(): GridSelection {
  return {
    anchor: { rowIndex: 0, columnName: 'id' },
    focus: { rowIndex: 1, columnName: 'name' },
    extraRanges: [
      {
        start: { rowIndex: 1, columnName: 'id' },
        end: { rowIndex: 1, columnName: 'name' },
      },
    ],
    mode: 'row',
  };
}

function dirtyEmptySelection(): GridSelection {
  return {
    anchor: { rowIndex: 1, columnName: 'name' },
    focus: { rowIndex: 1, columnName: 'name' },
    extraRanges: [
      {
        start: { rowIndex: 0, columnName: 'id' },
        end: { rowIndex: 0, columnName: 'id' },
      },
    ],
    mode: 'none',
  };
}

function cellSelection(): GridSelection {
  return {
    anchor: { rowIndex: 0, columnName: 'id' },
    focus: { rowIndex: 1, columnName: 'name' },
    extraRanges: [
      {
        start: { rowIndex: 1, columnName: 'id' },
        end: { rowIndex: 1, columnName: 'id' },
      },
    ],
    mode: 'cell',
  };
}

describe('applyGridSelection geometry normalisation (round 4)', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const loaded = (panelId: string): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(panelId);
    if (!ts) throw new Error(`panel ${panelId} has no table-data slice`);
    return ts;
  };

  async function loadTable(panelId: string) {
    await useTableDataStore.getState().loadTableData({
      panelId,
      dbSessionId: 'conn-1',
      table: 'users',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(response);
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    const gridSelection = await import('../tableData/gridSelection');
    const selectionActions = await import('../tableData/selectionActions');
    EMPTY_GRID_SELECTION = gridSelection.EMPTY_GRID_SELECTION;
    ROW_MODE_GRID_SELECTION = selectionActions.ROW_MODE_GRID_SELECTION;
    useTableDataStore.getState().reset();
  });

  it('row + dirty geometry is replaced by the canonical row constant', async () => {
    await loadTable(PANEL_A);
    const dirty = dirtyRowSelection();

    useTableDataStore.getState().setGridSelection(PANEL_A, dirty);

    const ts = loaded(PANEL_A);
    expect(ts.gridSelection).toBe(ROW_MODE_GRID_SELECTION);
    expect(ts.gridSelection.mode).toBe('row');
    expect(ts.gridSelection.anchor).toBeNull();
    expect(ts.gridSelection.focus).toBeNull();
    expect(ts.gridSelection.extraRanges).toEqual([]);
    expect(ts.selectedRows.size).toBe(0);
    expect(ts.lastSelectedIndex).toBeNull();
  });

  it('none + dirty geometry clears geometry, rows and the last index', async () => {
    await loadTable(PANEL_A);
    useTableDataStore.getState().selectRow(PANEL_A, 0);
    expect(loaded(PANEL_A).selectedRows).toEqual(new Set([0]));

    useTableDataStore.getState().setGridSelection(PANEL_A, dirtyEmptySelection());

    const ts = loaded(PANEL_A);
    expect(ts.gridSelection).toBe(EMPTY_GRID_SELECTION);
    expect(ts.selectedRows.size).toBe(0);
    expect(ts.lastSelectedIndex).toBeNull();
  });

  it('legal cell geometry is preserved verbatim and only the row set is cleared', async () => {
    await loadTable(PANEL_A);
    useTableDataStore.getState().selectRow(PANEL_A, 1);
    const cell = cellSelection();

    useTableDataStore.getState().setGridSelection(PANEL_A, cell);

    const ts = loaded(PANEL_A);
    expect(ts.gridSelection.mode).toBe('cell');
    expect(ts.gridSelection).toEqual(cell);
    expect(ts.selectedRows.size).toBe(0);
    expect(ts.lastSelectedIndex).toBeNull();
  });

  it('row normalisation does not fall back to none when the row set is empty', async () => {
    await loadTable(PANEL_A);

    useTableDataStore.getState().setGridSelection(PANEL_A, dirtyRowSelection());
    useTableDataStore.getState().setGridSelection(PANEL_A, dirtyRowSelection());

    expect(loaded(PANEL_A).gridSelection.mode).toBe('row');
    expect(loaded(PANEL_A).selectedRows.size).toBe(0);

    useTableDataStore.getState().selectRow(PANEL_A, 1);
    expect(loaded(PANEL_A).gridSelection.mode).toBe('row');
    expect(loaded(PANEL_A).selectedRows).toEqual(new Set([1]));
  });

  it('panels share the row constant by reference without leaking state across panels', async () => {
    await loadTable(PANEL_A);
    await loadTable(PANEL_B);

    useTableDataStore.getState().setGridSelection(PANEL_A, dirtyRowSelection());
    useTableDataStore.getState().setGridSelection(PANEL_B, dirtyRowSelection());
    expect(loaded(PANEL_A).gridSelection).toBe(ROW_MODE_GRID_SELECTION);
    expect(loaded(PANEL_B).gridSelection).toBe(ROW_MODE_GRID_SELECTION);

    useTableDataStore.getState().selectRow(PANEL_A, 0);
    expect(loaded(PANEL_A).selectedRows).toEqual(new Set([0]));
    expect(loaded(PANEL_B).selectedRows.size).toBe(0);

    useTableDataStore.getState().clearGridSelection(PANEL_B);
    expect(loaded(PANEL_B).gridSelection).toBe(EMPTY_GRID_SELECTION);
    expect(loaded(PANEL_A).gridSelection).toBe(ROW_MODE_GRID_SELECTION);
    expect(loaded(PANEL_A).selectedRows).toEqual(new Set([0]));
  });

  it('production actions never mutate the shared row constant', async () => {
    await loadTable(PANEL_A);
    await loadTable(PANEL_B);
    const snapshot = JSON.stringify(ROW_MODE_GRID_SELECTION);

    useTableDataStore.getState().setGridSelection(PANEL_A, dirtyRowSelection());
    useTableDataStore.getState().selectRow(PANEL_A, 0);
    useTableDataStore.getState().toggleSelectAll(PANEL_A);
    useTableDataStore.getState().setRowModeSelection(PANEL_B, 1, { range: true });
    useTableDataStore.getState().clearGridSelection(PANEL_B);
    useTableDataStore.getState().setGridSelection(PANEL_B, dirtyRowSelection());

    // The constant stored in both panels must still be the pristine canonical object:
    // a stray `extraRanges.push` / field assignment anywhere would show up here.
    expect(JSON.stringify(ROW_MODE_GRID_SELECTION)).toBe(
      JSON.stringify({ anchor: null, focus: null, extraRanges: [], mode: 'row' }),
    );
    expect(JSON.stringify(ROW_MODE_GRID_SELECTION)).toBe(snapshot);
    expect(ROW_MODE_GRID_SELECTION.extraRanges).toHaveLength(0);
    expect(loaded(PANEL_A).gridSelection).toBe(ROW_MODE_GRID_SELECTION);
    expect(loaded(PANEL_B).gridSelection).toBe(ROW_MODE_GRID_SELECTION);
  });

  it('mutating the caller object after the call cannot reach the stored row mode', async () => {
    await loadTable(PANEL_A);
    const dirty = dirtyRowSelection();

    useTableDataStore.getState().setGridSelection(PANEL_A, dirty);
    dirty.anchor = { rowIndex: 1, columnName: 'id' };
    dirty.extraRanges.push({
      start: { rowIndex: 0, columnName: 'id' },
      end: { rowIndex: 0, columnName: 'id' },
    });

    const ts = loaded(PANEL_A);
    expect(ts.gridSelection).toBe(ROW_MODE_GRID_SELECTION);
    expect(ts.gridSelection.anchor).toBeNull();
    expect(ts.gridSelection.extraRanges).toHaveLength(0);
  });
});
