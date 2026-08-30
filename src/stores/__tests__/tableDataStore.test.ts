import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { FilterCondition, SortCondition } from '../../types';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  commitRowUpdates: vi.fn(),
  commitRowDeletes: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

const sampleColumns = [
  { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
];

const sampleResponse = {
  columns: sampleColumns,
  rows: [
    [1, 'Alice'],
    [2, 'Bob'],
  ] as (string | number | null)[][],
  totalRows: 2,
  page: 0,
  pageSize: 50,
};

const samplePlan = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  table: { dbSessionId: 'conn-1', table: 'users', database: null },
  updates: [
    {
      rowIdentity: { id: 1 },
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Updated' },
      changedColumns: ['name'],
      sqlTemplate: 'UPDATE "users" SET "name" = \'Updated\' WHERE "id" = 1',
      parameterSummary: ['SET name="Updated"', 'PK id=1'],
    },
  ],
  deletes: [],
  warnings: [],
};

describe('tableDataStore', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(sampleResponse);
    mockDatabaseCommands.commitRowUpdates.mockResolvedValue(undefined);
    mockDatabaseCommands.commitRowDeletes.mockResolvedValue(undefined);
    mockDatabaseCommands.previewPendingChanges.mockResolvedValue(samplePlan);
    mockDatabaseCommands.commitPendingChanges.mockResolvedValue({
      planId: samplePlan.planId,
      fingerprint: samplePlan.fingerprint,
      statements: [{ operation: 'update', rowIdentity: { id: 1 }, affectedRows: 1 }],
      affectedRows: 1,
    });
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    useTableDataStore.getState().reset();
    useTableDataStore.getState().setActiveConnection('conn-1');
  });

  async function loadTable() {
    await useTableDataStore.getState().loadTableData({ dbSessionId: 'conn-1', table: 'users' });
  }

  it('detailRowIndex defaults to null', () => {
    expect(useTableDataStore.getState().detailRowIndex).toBeNull();
  });

  it('setDetailRow sets and clears index', () => {
    useTableDataStore.getState().setDetailRow(2);
    expect(useTableDataStore.getState().detailRowIndex).toBe(2);
    useTableDataStore.getState().setDetailRow(null);
    expect(useTableDataStore.getState().detailRowIndex).toBeNull();
  });

  it('loadTableData populates rows and columns', async () => {
    await loadTable();
    const s = useTableDataStore.getState();
    expect(s.activeTable).toBe('users');
    expect(s.rows).toHaveLength(2);
    expect(s.rows[0]).toEqual({ id: 1, name: 'Alice' });
    expect(s.loading).toBe(false);
  });

  it('loadTableData handles errors', async () => {
    mockDatabaseCommands.getTableData.mockRejectedValueOnce(new Error('db error'));
    await loadTable();
    expect(useTableDataStore.getState().error).toBe('db error');
  });

  it('skips duplicate concurrent loads', async () => {
    let resolveLoad: () => void;
    mockDatabaseCommands.getTableData.mockReturnValueOnce(
      new Promise((r) => {
        resolveLoad = () => r(sampleResponse);
      }),
    );
    const p1 = useTableDataStore
      .getState()
      .loadTableData({ dbSessionId: 'conn-1', table: 'users' });
    await useTableDataStore.getState().loadTableData({ dbSessionId: 'conn-1', table: 'users' });
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(1);
    resolveLoad!();
    await p1;
  });

  it('switchToTable syncs flat state', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockResolvedValueOnce({
      ...sampleResponse,
      rows: [[3, 'Carol']],
    });
    await useTableDataStore.getState().loadTableData({ dbSessionId: 'conn-1', table: 'orders' });
    useTableDataStore.getState().switchToTable('users');
    expect(useTableDataStore.getState().rows[0].name).toBe('Alice');
  });

  it('setPage triggers reload with skipCount', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().setPage(1);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ page: 1, skipCount: true }),
    );
  });

  it('forwards the explicit database and remembers it for refreshes (F1 BUG-002)', async () => {
    await useTableDataStore
      .getState()
      .loadTableData({ dbSessionId: 'conn-1', table: 'users', database: 'db_b' });
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ table: 'users', database: 'db_b' }),
    );

    // Store-driven refreshes (paging) keep targeting the same database.
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().setPage(1);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ page: 1, database: 'db_b' }),
    );
  });

  it('sends a null database when no explicit target is given', async () => {
    await loadTable();
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ table: 'users', database: null }),
    );
  });

  it('addFilter edits draft only; applyFilters reloads', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    const filter: FilterCondition = { column: 'name', operator: 'eq', value: 'Alice' };
    useTableDataStore.getState().addFilter(filter);
    expect(useTableDataStore.getState().draftFilters).toContainEqual(filter);
    expect(useTableDataStore.getState().filters).toEqual([]);
    expect(useTableDataStore.getState().filterPanelOpen).toBe(true);
    await new Promise((r) => setTimeout(r, 20));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();

    useTableDataStore.getState().applyFilters();
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(useTableDataStore.getState().filters).toContainEqual(filter);

    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().removeFilter(0);
    expect(useTableDataStore.getState().draftFilters).toEqual([]);
    expect(useTableDataStore.getState().filters).toContainEqual(filter);
    await new Promise((r) => setTimeout(r, 20));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();

    useTableDataStore.getState().setFilters([filter]);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    useTableDataStore.getState().clearFilters();
    expect(useTableDataStore.getState().filters).toEqual([]);
    expect(useTableDataStore.getState().draftFilters).toEqual([]);
  });

  it('addFilter with empty value does not reload', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().addFilter({ column: 'id', operator: 'eq', value: '' });
    expect(useTableDataStore.getState().draftFilters).toHaveLength(1);
    expect(useTableDataStore.getState().filters).toHaveLength(0);
    await new Promise((r) => setTimeout(r, 20));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();
  });

  it('loadTableData omits incomplete applied filters from the request', async () => {
    await loadTable();
    useTableDataStore.getState().addFilter({ column: 'id', operator: 'eq', value: '' });
    useTableDataStore.getState().addFilter({ column: 'name', operator: 'eq', value: 'Bob' });
    useTableDataStore.getState().applyFilters();
    await vi.waitFor(() =>
      expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
        expect.objectContaining({
          filters: [{ column: 'name', operator: 'eq', value: 'Bob' }],
        }),
      ),
    );
  });

  it('setSort triggers reload', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    const sort: SortCondition = { column: 'name', direction: 'asc' };
    useTableDataStore.getState().setSort(sort);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(useTableDataStore.getState().sorts).toEqual([sort]);
  });

  it('startEdit and cancelEdit', async () => {
    await loadTable();
    useTableDataStore.getState().startEdit(0, 'name');
    expect(useTableDataStore.getState().editingCell).toEqual({ row: 0, col: 'name' });
    useTableDataStore.getState().cancelEdit();
    expect(useTableDataStore.getState().editingCell).toBeNull();
  });

  it('updateCell stages changes without committing', async () => {
    await loadTable();
    useTableDataStore.getState().updateCell(0, 'name', 'Updated');
    expect(mockDatabaseCommands.commitRowUpdates).not.toHaveBeenCalled();
    expect(mockDatabaseCommands.previewPendingChanges).not.toHaveBeenCalled();
    expect(useTableDataStore.getState().rows[0].name).toBe('Updated');
    expect(useTableDataStore.getState().pendingChanges.size).toBe(1);
    expect([...useTableDataStore.getState().pendingChanges.values()][0]).toMatchObject({
      rowIdentity: { id: 1 },
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Updated' },
      changedColumns: ['name'],
      deleteMarked: false,
    });
  });

  it('retains null originals and removes a change when reverted', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValueOnce({
      ...sampleResponse,
      rows: [[1, null]],
    });
    await loadTable();
    useTableDataStore.getState().stageCellChange(0, 'name', 'Updated');
    expect([...useTableDataStore.getState().pendingChanges.values()][0].originalValues).toEqual({
      name: null,
    });

    useTableDataStore.getState().stageCellChange(0, 'name', null);
    expect(useTableDataStore.getState().pendingChanges.size).toBe(0);
  });

  it('staging errors without primary key and never creates a write', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValueOnce({
      columns: [{ name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true }],
      rows: [['x']],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    });
    await useTableDataStore.getState().loadTableData({ dbSessionId: 'conn-1', table: 'nopk' });
    useTableDataStore.getState().updateCell(0, 'name', 'y');
    expect(useTableDataStore.getState().error).toBeTruthy();
    expect(useTableDataStore.getState().pendingChanges.size).toBe(0);
    expect(mockDatabaseCommands.previewPendingChanges).not.toHaveBeenCalled();
  });

  it('commit failure preserves pending changes', async () => {
    await loadTable();
    useTableDataStore.getState().updateCell(0, 'name', 'Fail');
    mockDatabaseCommands.commitPendingChanges.mockRejectedValueOnce(new Error('commit fail'));
    const result = await useTableDataStore.getState().commitPendingChanges();
    expect(result.status).toBe('failed');
    expect(useTableDataStore.getState().error).toBe('commit fail');
    expect(useTableDataStore.getState().pendingChanges.size).toBe(1);
    expect(useTableDataStore.getState().previewPlan).toEqual(samplePlan);
  });

  it('preview and successful commit clear pending changes and request refresh', async () => {
    await loadTable();
    useTableDataStore.getState().stageCellChange(0, 'name', 'Updated');
    const plan = await useTableDataStore.getState().previewPendingChanges();
    expect(plan).toEqual(samplePlan);
    expect(mockDatabaseCommands.previewPendingChanges).toHaveBeenCalledWith({
      dbSessionId: 'conn-1',
      table: 'users',
      database: null,
      changes: [
        {
          rowIdentity: { id: 1 },
          originalValues: { name: 'Alice' },
          currentValues: { name: 'Updated' },
          changedColumns: ['name'],
          deleteMarked: false,
        },
      ],
    });
    const result = await useTableDataStore.getState().commitPendingChanges();
    expect(result.status).toBe('committed');
    expect(result.refreshRequired).toBe(true);
    expect(result.refreshed).toBe(true);
    expect(useTableDataStore.getState().pendingChanges.size).toBe(0);
    expect(mockDatabaseCommands.commitPendingChanges).toHaveBeenCalledWith({
      dbSessionId: 'conn-1',
      plan: samplePlan,
      fingerprint: samplePlan.fingerprint,
    });
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2);
  });

  it('discardChanges reloads table', async () => {
    await loadTable();
    useTableDataStore.getState().startEdit(0, 'name');
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().discardChanges();
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
  });

  it('selectRow single, multi, and range', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(0);
    expect(useTableDataStore.getState().selectedRows).toEqual(new Set([0]));

    useTableDataStore.getState().selectRow(1, { multi: true });
    expect(useTableDataStore.getState().selectedRows).toEqual(new Set([0, 1]));

    useTableDataStore.getState().selectRow(0, { multi: true });
    expect(useTableDataStore.getState().selectedRows).toEqual(new Set([1]));

    useTableDataStore.getState().selectRow(0);
    useTableDataStore.getState().selectRow(1, { range: true });
    expect(useTableDataStore.getState().selectedRows).toEqual(new Set([0, 1]));
  });

  it('toggleSelectAll selects and deselects all rows', async () => {
    await loadTable();
    useTableDataStore.getState().toggleSelectAll();
    expect(useTableDataStore.getState().selectedRows.size).toBe(2);
    useTableDataStore.getState().toggleSelectAll();
    expect(useTableDataStore.getState().selectedRows.size).toBe(0);
  });

  it('deleteSelectedRows stages PK deletes without executing', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(0);
    useTableDataStore.getState().selectRow(1, { multi: true });
    await useTableDataStore.getState().deleteSelectedRows();
    expect(mockDatabaseCommands.commitRowDeletes).not.toHaveBeenCalled();
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(1);
    expect([...useTableDataStore.getState().pendingChanges.values()]).toEqual([
      expect.objectContaining({ rowIdentity: { id: 1 }, deleteMarked: true }),
      expect.objectContaining({ rowIdentity: { id: 2 }, deleteMarked: true }),
    ]);
  });

  it('deleteRows selects indices then stages deletes', async () => {
    await loadTable();
    await useTableDataStore.getState().deleteRows([1]);
    expect(mockDatabaseCommands.commitRowDeletes).not.toHaveBeenCalled();
    expect([...useTableDataStore.getState().pendingChanges.values()]).toEqual([
      expect.objectContaining({ rowIdentity: { id: 2 }, deleteMarked: true }),
    ]);
  });

  it('closeTable removes table state', async () => {
    await loadTable();
    useTableDataStore.getState().closeTable('users');
    expect(useTableDataStore.getState().activeTable).toBeNull();
    expect(useTableDataStore.getState().tableStates.has('users')).toBe(false);
  });

  it('reset clears all state', async () => {
    await loadTable();
    useTableDataStore.getState().reset();
    expect(useTableDataStore.getState().activeDbSessionId).toBeNull();
    expect(useTableDataStore.getState().rows).toEqual([]);
  });

  it('setDatabaseType affects page size from registry', async () => {
    useTableDataStore.getState().setDatabaseType('postgresql');
    await loadTable();
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ pageSize: expect.any(Number) }),
    );
  });

  it('setFilterLogic only updates draft until apply', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().setFilterLogic('or');
    expect(useTableDataStore.getState().draftFilterLogic).toBe('or');
    expect(useTableDataStore.getState().filterLogic).toBe('and');
    await new Promise((r) => setTimeout(r, 20));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();

    useTableDataStore.getState().applyFilters();
    await vi.waitFor(() => {
      expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
        expect.objectContaining({ filterLogic: 'or' }),
      );
    });
    expect(useTableDataStore.getState().filterLogic).toBe('or');
  });
});
