/**
 * DB-01 round-2 regression pins for the D3/D4 fixes and for the new §4 invariant:
 * "`mode` is the row set's single authority — writes must be paired".
 *
 * Covered here (the pure/slice suites do not):
 * - a *stale* failed fetch (already superseded by a newer request) must not clear the
 *   selection the newer request is currently serving — the revision guard is the fix;
 * - row mode is reachable from `selectRow`, and every row-set writer keeps
 *   `mode`/`selectedRows`/`lastSelectedIndex` in agreement;
 * - `deleteRows` writes a row set without pairing the mode, which the new invariant
 *   forbids while a cell selection is active (03's batch-delete path will hit this).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { TableState } from '../tableData/types';
import {
  EMPTY_GRID_SELECTION,
  rowsCoveredBySelection,
  selectionFromAnchorFocus,
} from '../tableData/gridSelection';
import { ROW_MODE_GRID_SELECTION } from '../tableData/selectionActions';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));

const PANEL = 'panel-row-selection-r2';

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

interface Deferred<T> {
  promise: Promise<T>;
  resolve: (value: T) => void;
  reject: (reason: unknown) => void;
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

async function flush(): Promise<void> {
  await Promise.resolve();
  await Promise.resolve();
}

describe('tableDataStore row-mode + stale-failure semantics (round 2)', () => {
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

  function selectCell() {
    useTableDataStore
      .getState()
      .setGridSelection(
        PANEL,
        selectionFromAnchorFocus(
          { rowIndex: 0, columnName: 'id' },
          { rowIndex: 1, columnName: 'name' },
          'cell',
        ),
      );
  }

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(response);
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    useTableDataStore.getState().reset();
  });

  it('selectRow enters row mode and rowsCoveredBySelection reports the same set', async () => {
    await loadTable();

    useTableDataStore.getState().selectRow(PANEL, 1);

    // `vi.resetModules()` gives the store its own copy of the frozen constant, so compare
    // structurally; the row-set identity check below uses the store's own Set reference.
    expect(loaded().gridSelection).toEqual(ROW_MODE_GRID_SELECTION);
    expect(loaded().gridSelection).toMatchObject({ mode: 'row', anchor: null, focus: null });
    expect(loaded().selectedRows).toEqual(new Set([1]));
    expect(loaded().lastSelectedIndex).toBe(1);

    const covered = rowsCoveredBySelection({
      ...loaded().gridSelection,
      selectedRows: loaded().selectedRows,
    });
    // §4.2.1: row mode returns `selectedRows` itself, no copy.
    expect(covered).toBe(loaded().selectedRows);
  });

  it('setRowModeSelection(null) clears geometry, rows and the last index', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 1);

    useTableDataStore.getState().setRowModeSelection(PANEL, null);

    expect(loaded().gridSelection).toEqual(EMPTY_GRID_SELECTION);
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();
  });

  it('setRowModeSelection(index, {range}) extends the row set and keeps mode "row"', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 0);

    useTableDataStore.getState().setRowModeSelection(PANEL, 1, { range: true });

    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows).toEqual(new Set([0, 1]));
    expect(loaded().lastSelectedIndex).toBe(1);
  });

  it('toggleSelectAll enters row mode; toggling it off leaves row mode with no rows', async () => {
    await loadTable();

    useTableDataStore.getState().toggleSelectAll(PANEL);
    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows).toEqual(new Set([0, 1]));

    useTableDataStore.getState().toggleSelectAll(PANEL);
    // Documented current behaviour: "row mode, nothing selected" (no highlight either way).
    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();
  });

  it('a stale failed fetch must not clear the selection of the newer request', async () => {
    await loadTable();
    const stale = deferred<typeof response>();
    const fresh = deferred<typeof response>();
    mockDatabaseCommands.getTableData
      .mockReturnValueOnce(stale.promise)
      .mockReturnValueOnce(fresh.promise);

    useTableDataStore.getState().reloadPanel(PANEL); // revision 0, request A
    await flush();
    useTableDataStore.getState().setPage(PANEL, 1); // revision 1, request B
    await flush();

    useTableDataStore.getState().selectRow(PANEL, 1); // selection made while B is in flight
    expect(loaded().gridSelection.mode).toBe('row');

    stale.reject(new Error('old request failed'));
    await flush();

    // The superseded failure must be ignored: no error banner, no selection reset.
    expect(loaded().error).toBeNull();
    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows).toEqual(new Set([1]));
    expect(loaded().lastSelectedIndex).toBe(1);
    expect(loaded().loading).toBe(true);

    fresh.resolve({ ...response, page: 1 });
    await vi.waitFor(() => expect(loaded().loading).toBe(false));

    // The request that actually owns the page still performs the reset convergence.
    expect(loaded().gridSelection.mode).toBe('none');
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();
  });

  it('setGridSelection(row) clears the row set (bypassing setRowModeSelection loses rows)', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 1);
    expect(loaded().selectedRows).toEqual(new Set([1]));

    useTableDataStore.getState().setGridSelection(PANEL, ROW_MODE_GRID_SELECTION);

    expect(loaded().gridSelection.mode).toBe('row');
    expect(loaded().selectedRows.size).toBe(0);
  });

  it('deleteRows must not leave a non-empty row set under a cell selection (§4 invariant)', async () => {
    await loadTable();
    selectCell();
    expect(loaded().gridSelection.mode).toBe('cell');

    useTableDataStore.getState().deleteRows(PANEL, [1]);

    // New §4 invariant: `mode: 'cell'` implies `selectedRows` is empty; row-set writes are
    // only legal together with the mode through setRowModeSelection/applyRowModeTransition.
    expect(loaded().gridSelection.mode).toBe('cell');
    expect(loaded().selectedRows.size).toBe(0);
    expect(loaded().lastSelectedIndex).toBeNull();
  });
});
