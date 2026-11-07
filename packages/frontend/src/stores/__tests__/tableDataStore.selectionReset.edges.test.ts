/**
 * Supplemental regression coverage for "re-fetch owns selection reset" (DB-01 §4.3/§7).
 *
 * Adds the cases the main `selectionReset` file does not assert:
 * - the `selectedRows` half is proven with a **row-only** selection, so it cannot be
 *   satisfied by the pre-existing `commitFetchedPage` cleanup (the main file's helper enters
 *   cell mode last, which clears the row set eagerly and makes that half vacuous);
 * - `setPageSize` / `applyFilters` (both listed in 01-selection §9.7) are exercised;
 * - the "actions do not clear on their own" contract is pinned through the `applyFilters`
 *   early return;
 * - a direct `reloadPanel` failure, which 01-selection §7 #19 says must also clear.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TableState } from '../tableData/types';
import {
  EMPTY_GRID_SELECTION,
  rowsCoveredBySelection,
  selectionFromAnchorFocus,
} from '../tableData/gridSelection';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));

const PANEL = 'panel-selection-edges';

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

function never<T>(): Promise<T> {
  return new Promise<T>(() => {});
}

describe('tableDataStore selection reset — supplemental edges', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const loaded = (): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(PANEL);
    if (!ts) throw new Error(`panel ${PANEL} has no table-data slice`);
    return ts;
  };

  async function loadTable() {
    await useTableDataStore.getState().loadTableData({
      panelId: PANEL,
      dbSessionId: 'conn-1',
      table: 'users',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
  }

  /** Row-only selection: no cell mode, so `selectedRows` keeps the row the user clicked. */
  function selectRowOnly() {
    useTableDataStore.getState().selectRow(PANEL, 1);
    expect(loaded().selectedRows).toEqual(new Set([1]));
    expect(loaded().lastSelectedIndex).toBe(1);
  }

  function selectCell() {
    useTableDataStore
      .getState()
      .setGridSelection(
        PANEL,
        selectionFromAnchorFocus({ rowIndex: 0, columnName: 'id' }, { rowIndex: 1, columnName: 'name' }, 'cell'),
      );
    expect(loaded().gridSelection.mode).toBe('cell');
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(response);
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    useTableDataStore.getState().reset();
  });

  it('setPageSize clears a row-only selection before the response lands', async () => {
    await loadTable();
    selectRowOnly();
    mockDatabaseCommands.getTableData.mockReturnValueOnce(never());

    useTableDataStore.getState().setPageSize(PANEL, 100);

    expect(loaded().pageSize).toBe(100);
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();
  });

  it('applyFilters clears a cell selection before the response lands', async () => {
    await loadTable();
    selectCell();
    useTableDataStore
      .getState()
      .addFilter(PANEL, { column: 'name', operator: 'eq', value: 'Alice' });
    mockDatabaseCommands.getTableData.mockReturnValueOnce(never());

    useTableDataStore.getState().applyFilters(PANEL);

    expect(loaded().filters).toHaveLength(1);
    expect(loaded().gridSelection).toEqual(EMPTY_GRID_SELECTION);
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();
  });

  it('rowsCoveredBySelection cannot reference stale rows after paging', async () => {
    await loadTable();
    useTableDataStore
      .getState()
      .setGridSelection(
        PANEL,
        selectionFromAnchorFocus({ rowIndex: 0, columnName: 'id' }, { rowIndex: 1, columnName: 'id' }, 'cell'),
      );
    expect(
      rowsCoveredBySelection({ ...loaded().gridSelection, selectedRows: loaded().selectedRows })
        .size,
    ).toBe(2);

    mockDatabaseCommands.getTableData.mockReturnValueOnce(never());
    useTableDataStore.getState().setPage(PANEL, 1);

    expect(loaded().page).toBe(1);
    expect(
      rowsCoveredBySelection({ ...loaded().gridSelection, selectedRows: loaded().selectedRows })
        .size,
    ).toBe(0);
    expect(rowsCoveredBySelection({ ...loaded().gridSelection, selectedRows: loaded().selectedRows }).size).toBe(0);
  });

  it('applyFilters with an unchanged draft returns early without clearing the selection', async () => {
    await loadTable();
    selectCell();
    const callsBefore = mockDatabaseCommands.getTableData.mock.calls.length;

    useTableDataStore.getState().applyFilters(PANEL);

    expect(mockDatabaseCommands.getTableData.mock.calls.length).toBe(callsBefore);
    expect(loaded().gridSelection.mode).toBe('cell');
  });

  it('setVisibleColumns(null) keeps a selection that is still displayable', async () => {
    await loadTable();
    selectCell();

    useTableDataStore.getState().setVisibleColumns(PANEL, null);

    expect(loaded().gridSelection.mode).toBe('cell');
  });

  it('selectRow enters row mode so the gutter path can satisfy §4.2', async () => {
    await loadTable();

    useTableDataStore.getState().selectRow(PANEL, 0);

    // 01-selection §4.2 / §9.3 row-mode journey: gutter click => mode 'row', anchor null.
    expect(loaded().selectedRows).toEqual(new Set([0]));
    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().gridSelection.anchor).toBeNull();
    expect(loaded().gridSelection.focus).toBeNull();
  });

  it('starting an edit does not clear the cell selection (01-selection §7 #9)', async () => {
    await loadTable();
    selectCell();

    useTableDataStore.getState().startEdit(PANEL, 0, 'name');

    expect(loaded().editingCell).toEqual({ row: 0, col: 'name' });
    expect(loaded().gridSelection.mode).toBe('cell');
  });

  it('removePanel drops the slice so no selection cache outlives the panel (§7 #13)', async () => {
    await loadTable();
    selectCell();

    useTableDataStore.getState().removePanel(PANEL);

    expect(useTableDataStore.getState().byPanel.has(PANEL)).toBe(false);
  });

  it('a direct reloadPanel failure clears every row-indexed selection (01-selection §7 #19)', async () => {
    await loadTable();
    selectRowOnly();
    mockDatabaseCommands.getTableData.mockRejectedValueOnce(new Error('connection lost'));

    useTableDataStore.getState().reloadPanel(PANEL);
    await vi.waitFor(() => expect(loaded().error).toContain('connection lost'));

    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();
    expect(loaded().gridSelection).toEqual(EMPTY_GRID_SELECTION);
  });
});
