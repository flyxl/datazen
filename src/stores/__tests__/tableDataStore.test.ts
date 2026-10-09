import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { FilterCondition, SortCondition } from '../../types';
import type { TableState } from '../tableData/types';

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

function deferred<T>() {
  let resolve!: (value: T | PromiseLike<T>) => void;
  const promise = new Promise<T>((res) => {
    resolve = res;
  });
  return { promise, resolve };
}

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

const PANEL = 'panel-users';
const OTHER_PANEL = 'panel-orders';

describe('tableDataStore (panel-scoped)', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const slice = (panelId: string = PANEL) => useTableDataStore.getState().byPanel.get(panelId);
  const loaded = (panelId: string = PANEL): TableState => {
    const ts = slice(panelId);
    if (!ts) throw new Error(`panel ${panelId} has no table-data slice`);
    return ts;
  };

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
  });

  async function loadTable(panelId: string = PANEL) {
    await useTableDataStore.getState().loadTableData({
      panelId,
      dbSessionId: 'conn-1',
      table: panelId === OTHER_PANEL ? 'orders' : 'users',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
  }

  it('has no slice for a panel that never loaded', () => {
    expect(slice()).toBeUndefined();
  });

  it('detailRowIndex defaults to null and is scoped to the panel', async () => {
    await loadTable();
    expect(loaded().detailRowIndex).toBeNull();
    useTableDataStore.getState().setDetailRow(PANEL, 2);
    expect(loaded().detailRowIndex).toBe(2);
    useTableDataStore.getState().setDetailRow(PANEL, null);
    expect(loaded().detailRowIndex).toBeNull();
  });

  it('loadTableData populates rows and columns on the panel slice', async () => {
    await loadTable();
    const ts = loaded();
    expect(ts.context?.table).toBe('users');
    expect(ts.rows).toHaveLength(2);
    expect(ts.rows[0]).toEqual({ id: 1, name: 'Alice' });
    expect(ts.loading).toBe(false);
  });

  it('loadTableData handles errors', async () => {
    mockDatabaseCommands.getTableData.mockRejectedValueOnce(new Error('db error'));
    await loadTable();
    expect(loaded().error).toBe('db error');
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
      .loadTableData({ panelId: PANEL, dbSessionId: 'conn-1', table: 'users' });
    await useTableDataStore
      .getState()
      .loadTableData({ panelId: PANEL, dbSessionId: 'conn-1', table: 'users' });
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(1);
    resolveLoad!();
    await p1;
  });

  it('keeps one slice per panel so two tabs on different tables never overwrite each other', async () => {
    await loadTable(PANEL);
    mockDatabaseCommands.getTableData.mockResolvedValueOnce({
      ...sampleResponse,
      rows: [[3, 'Carol']],
    });
    await loadTable(OTHER_PANEL);
    expect(loaded(PANEL).rows[0].name).toBe('Alice');
    expect(loaded(OTHER_PANEL).rows[0].name).toBe('Carol');
  });

  it('ignores actions on a panel without a slice', () => {
    useTableDataStore.getState().setDetailRow('missing-panel', 1);
    useTableDataStore.getState().startEdit('missing-panel', 0, 'name');
    expect(slice('missing-panel')).toBeUndefined();
  });

  it('setPage triggers reload with skipCount', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().setPage(PANEL, 1);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ page: 1, skipCount: true }),
    );
  });

  it('ignores an old page response after a newer filter request starts', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();

    const oldPage = deferred<typeof sampleResponse>();
    const newFilterPage = deferred<typeof sampleResponse>();
    const filter: FilterCondition = { column: 'id', operator: 'eq', value: 99 };
    mockDatabaseCommands.getTableData.mockImplementation(
      (params: { page?: number; filters?: FilterCondition[] }) =>
        params.page === 2 ? oldPage.promise : newFilterPage.promise,
    );

    useTableDataStore.getState().setPage(PANEL, 2);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(1));
    useTableDataStore.getState().addFilter(PANEL, filter);
    useTableDataStore.getState().applyFilters(PANEL);

    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2));
    expect(mockDatabaseCommands.getTableData).toHaveBeenNthCalledWith(
      2,
      expect.objectContaining({ page: 0, filters: [filter] }),
    );

    oldPage.resolve({ ...sampleResponse, page: 2, rows: [[200, 'Old page']] });
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(loaded().rows[0]).toEqual({ id: 1, name: 'Alice' });
    expect(loaded().page).toBe(0);

    newFilterPage.resolve({ ...sampleResponse, page: 0, rows: [[99, 'Filtered']] });
    await vi.waitFor(() => expect(loaded().loading).toBe(false));
    expect(loaded().rows[0]).toEqual({ id: 99, name: 'Filtered' });
    expect(loaded().filters).toEqual([filter]);
    expect(loaded().page).toBe(0);
  });

  it('forwards the explicit database and remembers it for refreshes', async () => {
    await useTableDataStore
      .getState()
      .loadTableData({ panelId: PANEL, dbSessionId: 'conn-1', table: 'users', database: 'db_b' });
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ table: 'users', database: 'db_b' }),
    );

    // Store-driven refreshes (paging) keep targeting the same database.
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().setPage(PANEL, 1);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ page: 1, database: 'db_b' }),
    );
  });

  it('forwards the table schema so a non-default schema still resolves', async () => {
    await useTableDataStore.getState().loadTableData({
      panelId: PANEL,
      dbSessionId: 'conn-1',
      table: 'users',
      database: 'db_b',
      schema: 'sales',
    });
    // Omitting this made the host fall back to the connection default, which
    // reads a table outside that default as missing.
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ table: 'users', database: 'db_b', schema: 'sales' }),
    );

    // Store-driven refreshes (paging) keep the same schema.
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().setPage(PANEL, 1);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ page: 1, schema: 'sales' }),
    );
  });
  // NOTE: upstream's "isolates pending changes when the database context
  // changes" test was dropped here: it relied on the removed connection-scoped
  // API (switchToTable / root pendingChanges). Pending-change isolation is now
  // per-panel and covered by the slice-isolation tests above.

  it('uses the complete remembered context when no explicit target is given', async () => {
    await loadTable();
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ table: 'users', database: 'app' }),
    );
  });

  it('reloadPanel re-runs the panel load with its own context', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().reloadPanel(PANEL);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
      expect.objectContaining({ dbSessionId: 'conn-1', table: 'users', database: 'app' }),
    );
  });

  it('addFilter edits draft only; applyFilters reloads', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    const filter: FilterCondition = { column: 'name', operator: 'eq', value: 'Alice' };
    useTableDataStore.getState().addFilter(PANEL, filter);
    expect(loaded().draftFilters).toContainEqual(filter);
    expect(loaded().filters).toEqual([]);
    expect(loaded().filterPanelOpen).toBe(true);
    await new Promise((r) => setTimeout(r, 20));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();

    useTableDataStore.getState().applyFilters(PANEL);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(loaded().filters).toContainEqual(filter);

    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().removeFilter(PANEL, 0);
    expect(loaded().draftFilters).toEqual([]);
    expect(loaded().filters).toContainEqual(filter);
    await new Promise((r) => setTimeout(r, 20));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();

    useTableDataStore.getState().setFilters(PANEL, [filter]);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    useTableDataStore.getState().clearFilters(PANEL);
    expect(loaded().filters).toEqual([]);
    expect(loaded().draftFilters).toEqual([]);
  });

  it('addFilter with empty value does not reload', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().addFilter(PANEL, { column: 'id', operator: 'eq', value: '' });
    expect(loaded().draftFilters).toHaveLength(1);
    expect(loaded().filters).toHaveLength(0);
    await new Promise((r) => setTimeout(r, 20));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();
  });

  it('loadTableData omits incomplete applied filters from the request', async () => {
    await loadTable();
    useTableDataStore.getState().addFilter(PANEL, { column: 'id', operator: 'eq', value: '' });
    useTableDataStore.getState().addFilter(PANEL, { column: 'name', operator: 'eq', value: 'Bob' });
    useTableDataStore.getState().applyFilters(PANEL);
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
    const sort: SortCondition = { column: 'name', descending: false };
    useTableDataStore.getState().setSort(PANEL, sort);
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
    expect(loaded().sorts).toEqual([sort]);
  });

  it('startEdit and cancelEdit', async () => {
    await loadTable();
    useTableDataStore.getState().startEdit(PANEL, 0, 'name');
    expect(loaded().editingCell).toEqual({ row: 0, col: 'name' });
    useTableDataStore.getState().cancelEdit(PANEL);
    expect(loaded().editingCell).toBeNull();
  });

  it('stageCellChange stages without committing', async () => {
    await loadTable();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Updated');
    expect(mockDatabaseCommands.commitRowUpdates).not.toHaveBeenCalled();
    expect(mockDatabaseCommands.previewPendingChanges).not.toHaveBeenCalled();
    expect(loaded().rows[0].name).toBe('Updated');
    expect(loaded().pendingChanges.size).toBe(1);
    expect([...loaded().pendingChanges.values()][0]).toMatchObject({
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
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Updated');
    expect([...loaded().pendingChanges.values()][0].originalValues).toEqual({ name: null });

    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', null);
    expect(loaded().pendingChanges.size).toBe(0);
  });

  it('rejects a single-column primary-key edit that collides with another row', async () => {
    await loadTable();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'id', 2);

    const ts = loaded();
    expect(ts.pendingChanges.size).toBe(0);
    expect(ts.rows[0].id).toBe(1);
    expect(ts.error).toContain('ambiguous');
  });

  it('rejects a composite primary-key edit that collides with another row', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValueOnce({
      ...sampleResponse,
      columns: [
        { name: 'tenantId', dataType: 'integer', isPrimaryKey: true, isNullable: false },
        { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
        { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
      ],
      rows: [
        [1, 9, 'Alice'],
        [1, 10, 'Bob'],
      ],
    });
    await loadTable();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'id', 10);

    const ts = loaded();
    expect(ts.pendingChanges.size).toBe(0);
    expect(ts.rows[0].id).toBe(9);
    expect(ts.error).toContain('ambiguous');
  });

  it('keeps the original identity after a non-colliding primary-key edit', async () => {
    await loadTable();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'id', 3);
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Updated');

    const changes = [...loaded().pendingChanges.values()];
    expect(changes).toHaveLength(1);
    expect(changes[0]).toMatchObject({
      rowIdentity: { id: 1 },
      currentValues: { id: 3, name: 'Updated' },
    });
  });

  it('staging errors without primary key and never creates a write', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValueOnce({
      columns: [{ name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true }],
      rows: [['x']],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    });
    await useTableDataStore
      .getState()
      .loadTableData({ panelId: PANEL, dbSessionId: 'conn-1', table: 'nopk' });
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'y');
    expect(loaded().error).toBeTruthy();
    expect(loaded().pendingChanges.size).toBe(0);
    expect(mockDatabaseCommands.previewPendingChanges).not.toHaveBeenCalled();
  });

  it('commit failure preserves pending changes', async () => {
    await loadTable();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Fail');
    mockDatabaseCommands.commitPendingChanges.mockRejectedValueOnce(new Error('commit fail'));
    const result = await useTableDataStore.getState().commitPendingChanges(PANEL);
    expect(result.status).toBe('failed');
    expect(loaded().error).toBe('commit fail');
    expect(loaded().pendingChanges.size).toBe(1);
    expect(loaded().previewPlan).toEqual(samplePlan);
  });

  it('preview and successful commit clear pending changes and request refresh', async () => {
    await loadTable();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Updated');
    const plan = await useTableDataStore.getState().previewPendingChanges(PANEL);
    expect(plan).toEqual(samplePlan);
    expect(mockDatabaseCommands.previewPendingChanges).toHaveBeenCalledWith({
      context: samplePlan.table,
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
    const result = await useTableDataStore.getState().commitPendingChanges(PANEL);
    expect(result.status).toBe('committed');
    expect(result.refreshRequired).toBe(true);
    expect(result.refreshed).toBe(true);
    expect(loaded().pendingChanges.size).toBe(0);
    expect(mockDatabaseCommands.commitPendingChanges).toHaveBeenCalledWith({
      dbSessionId: 'conn-1',
      plan: samplePlan,
      fingerprint: samplePlan.fingerprint,
    });
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(2);
  });

  it('rollbackPendingChanges discards staged edits and reloads', async () => {
    await loadTable();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Updated');
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().rollbackPendingChanges(PANEL);
    await vi.waitFor(() => expect(loaded().pendingChanges.size).toBe(0));
    await vi.waitFor(() => expect(mockDatabaseCommands.getTableData).toHaveBeenCalled());
  });

  it('selectRow single, multi, and range', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 0);
    expect(loaded().selectedRows).toEqual(new Set([0]));

    useTableDataStore.getState().selectRow(PANEL, 1, { multi: true });
    expect(loaded().selectedRows).toEqual(new Set([0, 1]));

    useTableDataStore.getState().selectRow(PANEL, 0, { multi: true });
    expect(loaded().selectedRows).toEqual(new Set([1]));

    useTableDataStore.getState().selectRow(PANEL, 0);
    useTableDataStore.getState().selectRow(PANEL, 1, { range: true });
    expect(loaded().selectedRows).toEqual(new Set([0, 1]));
  });

  it('selectRow toggles the sole selected row off on a second plain click', async () => {
    await loadTable();
    useTableDataStore.getState().selectRow(PANEL, 0);
    expect(loaded().selectedRows).toEqual(new Set([0]));

    useTableDataStore.getState().selectRow(PANEL, 0);
    expect(loaded().selectedRows).toEqual(new Set());
    expect(loaded().lastSelectedIndex).toBeNull();

    // A plain click on another row still replaces (rather than clears) it.
    useTableDataStore.getState().selectRow(PANEL, 0);
    useTableDataStore.getState().selectRow(PANEL, 1);
    expect(loaded().selectedRows).toEqual(new Set([1]));
  });

  it('toggleSelectAll selects and deselects all rows', async () => {
    await loadTable();
    useTableDataStore.getState().toggleSelectAll(PANEL);
    expect(loaded().selectedRows.size).toBe(2);
    useTableDataStore.getState().toggleSelectAll(PANEL);
    expect(loaded().selectedRows.size).toBe(0);
  });

  it('applyColumnToRows stages the same value for the selected rows', async () => {
    await loadTable();
    useTableDataStore.getState().applyColumnToRows(PANEL, 'name', 'Same', [0, 1]);
    expect(loaded().pendingChanges.size).toBe(2);
    expect(loaded().rows.map((row) => row.name)).toEqual(['Same', 'Same']);
  });

  it('deleteRows selects indices then stages deletes', async () => {
    await loadTable();
    useTableDataStore.getState().deleteRows(PANEL, [1]);
    expect(mockDatabaseCommands.commitRowDeletes).not.toHaveBeenCalled();
    expect([...loaded().pendingChanges.values()]).toEqual([
      expect.objectContaining({ rowIdentity: { id: 2 }, deleteMarked: true }),
    ]);
  });

  it('removePanel drops the slice entirely', async () => {
    await loadTable();
    useTableDataStore.getState().removePanel(PANEL);
    expect(slice(PANEL)).toBeUndefined();
  });

  it('invalidateCachedData drops cached rows so the panel re-fetches', async () => {
    await loadTable();
    useTableDataStore.getState().invalidateCachedData('conn-1');
    expect(loaded().columns).toHaveLength(0);
    expect(loaded().rows).toHaveLength(0);
    mockDatabaseCommands.getTableData.mockClear();
    await loadTable();
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalledTimes(1);
  });

  it('invalidateCachedData can target a single table', async () => {
    await loadTable(PANEL);
    await loadTable(OTHER_PANEL);
    useTableDataStore.getState().invalidateCachedData('conn-1', 'users');
    expect(loaded(PANEL).rows).toHaveLength(0);
    expect(loaded(OTHER_PANEL).rows).toHaveLength(2);
  });

  it('invalidateCachedData keeps panels with staged edits untouched', async () => {
    await loadTable();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Updated');
    useTableDataStore.getState().invalidateCachedData('conn-1');
    expect(loaded().rows[0]).toEqual({ id: 1, name: 'Updated' });
    expect(loaded().pendingChanges.size).toBe(1);
  });

  it('invalidateCachedData ignores other sessions', async () => {
    await loadTable();
    useTableDataStore.getState().invalidateCachedData('conn-other');
    expect(loaded().rows).toHaveLength(2);
  });

  it('reset clears every panel slice', async () => {
    await loadTable();
    useTableDataStore.getState().reset();
    expect(useTableDataStore.getState().byPanel.size).toBe(0);
  });

  it('setFilterLogic only updates draft until apply', async () => {
    await loadTable();
    mockDatabaseCommands.getTableData.mockClear();
    useTableDataStore.getState().setFilterLogic(PANEL, 'or');
    expect(loaded().draftFilterLogic).toBe('or');
    expect(loaded().filterLogic).toBe('and');
    await new Promise((r) => setTimeout(r, 20));
    expect(mockDatabaseCommands.getTableData).not.toHaveBeenCalled();

    useTableDataStore.getState().applyFilters(PANEL);
    await vi.waitFor(() => {
      expect(mockDatabaseCommands.getTableData).toHaveBeenCalledWith(
        expect.objectContaining({ filterLogic: 'or' }),
      );
    });
    expect(loaded().filterLogic).toBe('or');
  });

  it('manages visibleColumns and drops stale names on the next page', async () => {
    await loadTable();
    expect(loaded().visibleColumns).toBeNull();

    useTableDataStore.getState().setVisibleColumns(PANEL, ['id']);
    expect(loaded().visibleColumns).toEqual(['id']);

    useTableDataStore.getState().setVisibleColumns(PANEL, ['id', 'ghost']);
    await loadTable();
    expect(loaded().visibleColumns).toEqual(['id']);

    useTableDataStore.getState().setVisibleColumns(PANEL, null);
    expect(loaded().visibleColumns).toBeNull();
  });
});
