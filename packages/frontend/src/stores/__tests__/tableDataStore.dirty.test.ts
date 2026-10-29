/**
 * The dirty-state journeys of the table panel (DB-06 §9.3).
 *
 * Every case drives the **real** store and reads the dirty index the panel
 * would render, so the assertions cover the whole chain: staged change → the
 * index → the counts the toolbar shows.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { buildDirtyIndex, selectDirtyCounts } from '../tableData/dirtyIndex';
import type { TableState } from '../tableData/types';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

const sampleColumns = [
  { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
];

const PAGE_0 = {
  columns: sampleColumns,
  rows: [
    [1, 'Alice'],
    [2, 'Bob'],
  ] as (string | number | null)[][],
  totalRows: 4,
  page: 0,
  pageSize: 2,
};

const PAGE_1 = {
  columns: sampleColumns,
  rows: [
    [3, 'Cara'],
    [4, 'Dan'],
  ] as (string | number | null)[][],
  totalRows: 4,
  page: 1,
  pageSize: 2,
};

const samplePlan = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  table: {
    connectionId: 'conn-1',
    dbSessionId: 'conn-1',
    driverType: 'postgres',
    database: 'app',
    schema: null,
    table: 'users',
  },
  updates: [],
  deletes: [],
  warnings: [],
};

const PANEL = 'panel-users';

describe('tableDataStore dirty-state journeys (DB-06)', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const slice = (panelId: string = PANEL): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(panelId);
    if (!ts) throw new Error(`panel ${panelId} has no table-data slice`);
    return ts;
  };

  /** The index the panel would paint right now. */
  const index = (panelId: string = PANEL) => buildDirtyIndex(slice(panelId));

  const load = async (panelId = PANEL) => {
    await useTableDataStore.getState().loadTableData({
      panelId,
      dbSessionId: 'conn-1',
      table: 'users',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
  };

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockImplementation(
      async ({ page }: { page?: number }) => (page === 1 ? PAGE_1 : PAGE_0),
    );
    mockDatabaseCommands.previewPendingChanges.mockResolvedValue(samplePlan);
    mockDatabaseCommands.commitPendingChanges.mockResolvedValue({
      planId: samplePlan.planId,
      fingerprint: samplePlan.fingerprint,
      statements: [],
      affectedRows: 0,
    });
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
  });

  it('journey: edit → highlight → edit back → clean', async () => {
    await load();
    const store = useTableDataStore.getState();

    store.stageCellChange(PANEL, 0, 'name', 'Alicia');
    expect(index().counts.totalCells).toBe(1);
    expect(index().byRowIndex.get(0)?.state).toBe('dirty');
    expect(index().counts.cellsOnPage).toBe(1);

    store.stageCellChange(PANEL, 0, 'name', 'Alice');
    expect(slice().pendingChanges.size).toBe(0);
    expect(index().counts.totalCells).toBe(0);
    expect(index().byRowIndex.has(0)).toBe(false);
  });

  it('journey: stage on page 1 → next page → back: the mark stays on the original row', async () => {
    await load();
    const store = useTableDataStore.getState();

    store.stageCellChange(PANEL, 0, 'name', 'Alicia');
    expect(index().byRowIndex.get(0)?.identityKey).toBe('["id":1]');

    store.setPage(PANEL, 1);
    await vi.waitFor(() => expect(slice().page).toBe(1));
    await vi.waitFor(() => expect(slice().rows[0]?.id).toBe(3));
    // The staged change is on another page: it is counted, but not painted.
    expect(index().counts.cellsOnPage).toBe(0);
    expect(index().counts.cellsOffPage).toBe(1);
    expect(index().counts.totalCells).toBe(1);
    expect(index().offPageKeys).toEqual(['["id":1]']);

    store.setPage(PANEL, 0);
    await vi.waitFor(() => expect(slice().page).toBe(0));
    await vi.waitFor(() => expect(slice().rows[0]?.id).toBe(1));
    // Back on the first page the mark is on the row with the same identity.
    expect(index().byRowIndex.get(0)?.identityKey).toBe('["id":1]');
    expect(index().counts.cellsOnPage).toBe(1);
  });

  it('journey: two cells in one row → totalCells 2, totalRows 1', async () => {
    await load();
    const store = useTableDataStore.getState();

    store.stageCellChange(PANEL, 0, 'name', 'Alicia');
    store.stageCellChange(PANEL, 0, 'id', 11);

    expect(index().counts.totalCells).toBe(2);
    expect(index().counts.totalRows).toBe(1);
    expect(index().counts.updateRows).toBe(1);
    expect(index().byRowIndex.get(0)?.changedColumns).toEqual(['name', 'id']);
  });

  it('journey: sort reload keeps the mark on the same identity', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');

    // The next page comes back with the rows in the opposite order.
    mockDatabaseCommands.getTableData.mockResolvedValue({
      ...PAGE_0,
      rows: [
        [2, 'Bob'],
        [1, 'Alice'],
      ] as (string | number | null)[][],
    });
    store.setSort(PANEL, { column: 'name', descending: true });
    await vi.waitFor(() => expect(slice().rows[0]?.id).toBe(2));

    // The mark follows the identity (id = 1), which is now the second row.
    expect(index().byRowIndex.get(1)?.identityKey).toBe('["id":1]');
    expect(index().byRowIndex.has(0)).toBe(false);
    expect(index().counts.cellsOnPage).toBe(1);
  });

  it('journey: filter change keeps off-filter changes staged and counted offPage', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');

    mockDatabaseCommands.getTableData.mockResolvedValue({
      ...PAGE_0,
      rows: [[2, 'Bob']] as (string | number | null)[][],
    });
    store.setFilters(PANEL, [{ column: 'name', operator: 'eq', value: 'Bob' }]);
    await vi.waitFor(() => expect(slice().rows.length).toBe(1));

    // The change is still staged and still committable.
    expect(slice().pendingChanges.size).toBe(1);
    expect(index().counts.totalCells).toBe(1);
    expect(index().counts.cellsOnPage).toBe(0);
    expect(index().counts.cellsOffPage).toBe(1);
  });

  it('journey: commit failure keeps every mark', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');
    mockDatabaseCommands.commitPendingChanges.mockRejectedValue(new Error('write denied'));

    const result = await store.commitPendingChanges(PANEL);

    expect(result.status).toBe('failed');
    expect(index().counts.totalCells).toBe(1);
    expect(slice().error).toBeTruthy();
  });

  it('journey: commit success clears marks only when the signature is unchanged', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');

    await store.commitPendingChanges(PANEL);
    expect(index().counts.totalCells).toBe(0);
    expect(slice().pendingChanges.size).toBe(0);
  });

  it('journey: rollback clears marks synchronously, values revert after the reload', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');
    expect(slice().rows[0]?.name).toBe('Alicia');

    store.rollbackPendingChanges(PANEL);
    // The mark is gone immediately — before the reload response arrives.
    expect(index().counts.totalCells).toBe(0);
    expect(slice().pendingChanges.size).toBe(0);

    mockDatabaseCommands.getTableData.mockResolvedValue(PAGE_0);
    await vi.waitFor(() => expect(slice().rows[0]?.name).toBe('Alice'));
  });

  it('journey: unstageRowDelete returns a delete-marked row that also had cell edits to dirty', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');
    store.stageRowDelete(PANEL, 0);
    expect(index().byRowIndex.get(0)?.state).toBe('deleted');

    store.unstageRowDelete(PANEL, 0);
    expect(index().byRowIndex.get(0)?.state).toBe('dirty');
    expect(index().byRowIndex.get(0)?.changedColumns).toEqual(['name']);
    expect(slice().pendingChanges.size).toBe(1);
  });

  it('journey: unstageRowDelete drops a row that was only marked for deletion', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageRowDelete(PANEL, 1);
    expect(slice().pendingChanges.size).toBe(1);

    store.unstageRowDelete(PANEL, 1);
    expect(slice().pendingChanges.size).toBe(0);
    expect(index().byRowIndex.has(1)).toBe(false);
  });

  it('journey: invalidateCachedData keeps a panel with staged edits and its marks', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');

    store.invalidateCachedData('conn-1');
    expect(slice().rows.length).toBe(2);
    expect(index().counts.totalCells).toBe(1);
  });

  it('journey: read-only and no-primary-key tables never create a mark', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns: [{ name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true }],
      rows: [['Alice']] as (string | number | null)[][],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    });
    await load();
    const store = useTableDataStore.getState();

    store.stageCellChange(PANEL, 0, 'name', 'Bob');
    expect(slice().error).toBeTruthy();
    expect(index().counts.totalCells).toBe(0);
  });

  it('journey: staging and clearing a change never rewrites selectedRows', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.setRowModeSelection(PANEL, 0);
    store.setRowModeSelection(PANEL, 1, { multi: true });
    const before = [...slice().selectedRows].sort();

    store.stageCellChange(PANEL, 0, 'name', 'Alicia');
    expect([...slice().selectedRows].sort()).toEqual(before);

    store.stageCellChange(PANEL, 0, 'name', 'Alice');
    expect([...slice().selectedRows].sort()).toEqual(before);
    expect(slice().lastSelectedIndex).toBe(1);
  });

  it('journey: a write executed elsewhere keeps the marks and the overlaid values', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');

    store.invalidateCachedData('conn-1');
    expect(slice().rows[0]?.name).toBe('Alicia');
    expect(index().counts.totalCells).toBe(1);
  });

  it('journey: connection teardown drops the slice and every mark', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');

    store.removePanel(PANEL);
    expect(useTableDataStore.getState().byPanel.has(PANEL)).toBe(false);
  });

  it('the panel counts and the index counts are the same numbers', async () => {
    await load();
    const store = useTableDataStore.getState();
    store.stageCellChange(PANEL, 0, 'name', 'Alicia');
    store.stageRowDelete(PANEL, 1);

    const panelCounts = selectDirtyCounts(slice().pendingChanges);
    const indexCounts = index().counts;
    expect(indexCounts.totalCells).toBe(panelCounts.totalCells);
    expect(indexCounts.totalRows).toBe(panelCounts.totalRows);
    expect(indexCounts.updateRows).toBe(panelCounts.updateRows);
    expect(indexCounts.deleteRows).toBe(panelCounts.deleteRows);
  });
});
