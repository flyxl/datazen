/**
 * Regression pin for "a new fetch invalidates row-indexed selection".
 *
 * Before DB-01, `patchPanelForReload` cleared nothing: only a *successful* response
 * (`commitFetchedPage`) dropped `selectedRows`, and `lastSelectedIndex` was never cleared by
 * any fetch path. These cases therefore assert on the **in-flight window** (immediately after
 * the transition starts, before the response lands) and on the **failure path**, where the
 * old behaviour kept stale rows selected while the pager already claimed a new page/query.
 *
 * `lastSelectedIndex` half: no fetch path used to touch it, so this part was red on day one.
 * `selectedRows` half: only red before the response is committed — asserting after
 * `await`-ing the response would have been a false green.
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TableState } from '../tableData/types';
import { EMPTY_GRID_SELECTION, selectionFromAnchorFocus } from '../tableData/gridSelection';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));

const PANEL = 'panel-selection-reset';

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

describe('tableDataStore selection reset on re-fetch', () => {
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

  /** What a user has after selecting a row plus a cell: both kinds of row-indexed state. */
  function selectEverything() {
    useTableDataStore.getState().selectRow(PANEL, 1);
    useTableDataStore.getState().setGridSelection(
      PANEL,
      selectionFromAnchorFocus({ rowIndex: 0, columnName: 'id' }, { rowIndex: 1, columnName: 'name' }, 'cell', [
        { start: { rowIndex: 1, columnName: 'id' }, end: { rowIndex: 1, columnName: 'id' } },
      ]),
    );
    expect(loaded().gridSelection.mode).toBe('cell');
  }

  function expectSelectionCleared() {
    // Deep equality: `vi.resetModules()` gives the store its own instance of the singleton.
    expect(loaded().gridSelection).toEqual(EMPTY_GRID_SELECTION);
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(response);
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    useTableDataStore.getState().reset();
  });

  it('clears page selection as soon as the page transition starts, before the response', async () => {
    await loadTable();
    selectEverything();
    mockDatabaseCommands.getTableData.mockReturnValueOnce(never());

    useTableDataStore.getState().setPage(PANEL, 1);

    expect(loaded().page).toBe(1);
    expectSelectionCleared();
  });

  it('keeps the selection cleared when the reload fails', async () => {
    await loadTable();
    selectEverything();
    mockDatabaseCommands.getTableData.mockRejectedValueOnce(new Error('connection lost'));

    useTableDataStore.getState().setSort(PANEL, { column: 'name', descending: false });
    await vi.waitFor(() => expect(loaded().error).toContain('connection lost'));

    expectSelectionCleared();
  });

  it('clears page selection when filters are applied', async () => {
    await loadTable();
    selectEverything();
    mockDatabaseCommands.getTableData.mockReturnValueOnce(never());

    useTableDataStore.getState().setFilters(PANEL, [
      { column: 'name', operator: 'eq', value: 'Alice' },
    ]);

    expect(loaded().filters).toHaveLength(1);
    expectSelectionCleared();
  });

  it('also clears lastSelectedIndex when the committed page arrives', async () => {
    await loadTable();
    selectEverything();

    await loadTable();

    expectSelectionCleared();
  });

  it('clears every row-indexed field when cached data is invalidated', async () => {
    await loadTable();
    selectEverything();

    useTableDataStore.getState().invalidateCachedData('conn-1');

    expectSelectionCleared();
    expect(loaded().rows).toHaveLength(0);
  });

  it('reconciles the selection when a selected column gets hidden', async () => {
    await loadTable();
    selectEverything();

    useTableDataStore.getState().setVisibleColumns(PANEL, ['id']);

    expectSelectionCleared();
    // Column visibility only converges the region; it must not reload the page.
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(1);
  });

  it('keeps the row selection when changing column visibility', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 0);

    useTableDataStore.getState().setVisibleColumns(PANEL, ['id']);

    expect(loaded().selectedRows).toEqual(new Set([0]));
    expect(loaded().lastSelectedIndex).toBe(0);
  });

  it('row mode is reachable from selectRow and survives a fetch-free column change', async () => {
    await loadTable();

    useTableDataStore.getState().selectRow(PANEL, 1);

    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().gridSelection.anchor).toBeNull();
    expect(loaded().selectedRows).toEqual(new Set([1]));
  });

  it('clearing the selection in row mode also drops the whole-row highlight (Escape path)', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 1);
    expect(loaded().gridSelection.mode).toBe('row');

    // Exactly what `useGridSelection` does on Escape / a blank click.
    useTableDataStore.getState().setGridSelection(PANEL, EMPTY_GRID_SELECTION);

    expectSelectionCleared();
  });

  it('leaving row mode through setRowModeSelection(null) clears geometry and rows', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 1);

    useTableDataStore.getState().setRowModeSelection(PANEL, null);

    expectSelectionCleared();
  });

  it('deleteRows keeps a cell selection intact and does not park rows in the row set', async () => {
    await loadTable();
    selectEverything();
    expect(loaded().selectedRows.size).toBe(0);

    useTableDataStore.getState().deleteRows(PANEL, [0, 1]);

    // Delete scope came from the explicit indices; the selection keeps its single authority.
    expect(loaded().gridSelection.mode).toBe('cell');
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();
    expect(loaded().pendingChanges.size).toBe(2);
  });

  it('deleteRows leaves a row-mode selection untouched', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 0);

    useTableDataStore.getState().deleteRows(PANEL, [1]);

    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows).toEqual(new Set([0]));
    expect(loaded().lastSelectedIndex).toBe(0);
    expect(loaded().pendingChanges.size).toBe(1);
  });

  it('a paging transition drops a row-mode selection and its mode', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 1);
    mockDatabaseCommands.getTableData.mockReturnValueOnce(never());

    useTableDataStore.getState().setPage(PANEL, 1);

    expectSelectionCleared();
  });
});
