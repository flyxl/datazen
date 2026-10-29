/**
 * Permanent guard for the extraction of the staged-edit actions out of
 * `tableDataStore.ts` (DB-06 track, GC budget).
 *
 * The temporary probe drove the pre-split store (a verbatim copy of commit
 * 78197e830) and the post-split one through the same 12-step action script and
 * compared the store's action surface and the resulting panel slice field by
 * field — both came out `identical: true`. The probe is deleted; this file is
 * what keeps that conclusion alive.
 *
 * The branches pinned here are the ones the split could have broken silently:
 * the error branches of the staged-edit actions, the index normalisation, and
 * the exact shape of the staged state.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { TableState } from '../types';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

const sampleColumns = [
  { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
  { name: 'score', dataType: 'integer', isPrimaryKey: false, isNullable: true },
];

const sampleResponse = {
  columns: sampleColumns,
  rows: [
    [1, 'Alice', 10],
    [2, 'Bob', 20],
    [3, 'Cara', 30],
  ] as (string | number | null)[][],
  totalRows: 3,
  page: 0,
  pageSize: 50,
};

const PANEL = 'panel-users';
const NO_PK_PANEL = 'panel-no-pk';

/** Every action the store exposes. The split must not add or drop one. */
const STORE_ACTIONS = [
  'addDraftRow',
  'addFilter',
  // DB-11: add (or re-aim) one column of the multi-column sort.
  'addSort',
  'applyColumnToRows',
  'applyFilters',
  'beginCellSelection',
  'cancelEdit',
  'clearEnumCandidates',
  'clearFilters',
  'clearGridSelection',
  'clearHighlightedNewRow',
  // DB-11: drop the whole multi-column sort.
  'clearSorts',
  'commitPendingChanges',
  'deleteRows',
  'duplicateDraftRow',
  'invalidateCachedData',
  'loadTableData',
  'previewPendingChanges',
  'reconcileGridSelectionForColumns',
  'reloadPanel',
  'removeDraftRow',
  'removeFilter',
  'removePanel',
  // DB-11: drop one column from the multi-column sort.
  'removeSort',
  'reset',
  'rollbackPendingChanges',
  // DB-07: the first-frame seed of a tab that was opened by a foreign-key jump.
  'seedPanel',
  'selectRow',
  // DB-12: the column tools — render-only, no re-read.
  'setColumnOrder',
  'setDefaultOrderOverride',
  'setDetailRow',
  'setDraftValue',
  'setEnumCandidates',
  'setFilterLogic',
  'setFilterPanelOpen',
  'setFilters',
  // DB-12: the freeze half of the column tools.
  'setFrozenColumnCount',
  'setGridSelection',
  'setPage',
  'setPageSize',
  'setRowModeSelection',
  'setSort',
  'setVisibleColumns',
  'stageCellChange',
  // DB-05: the three-state write (undefined = do not write the column).
  'stageCellValue',
  // DB-03: the batch write of a paste (one `set()` for the whole block).
  'stagePasteWrites',
  'stageRowDelete',
  'startEdit',
  'toggleSelectAll',
  'unstageRowDelete',
  'updateFilter',
];

describe('tableDataStore staged-edit actions (extracted slice)', () => {
  let useTableDataStore: typeof import('../../tableDataStore').useTableDataStore;

  const slice = (panelId: string = PANEL): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(panelId);
    if (!ts) throw new Error(`panel ${panelId} has no table-data slice`);
    return ts;
  };

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
    mockDatabaseCommands.getTableData.mockResolvedValue(sampleResponse);
    mockDatabaseCommands.previewPendingChanges.mockResolvedValue(null);
    mockDatabaseCommands.commitPendingChanges.mockResolvedValue({});
    const mod = await import('../../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
  });

  it('keeps the store action surface unchanged by the split', () => {
    const surface = Object.keys(useTableDataStore.getState())
      .filter((key) => key !== 'byPanel')
      .sort();
    expect(surface).toEqual(STORE_ACTIONS);
  });

  it('stages a cell change and anchors the row identity', async () => {
    await load();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Alicia');

    const ts = slice();
    expect(ts.rows[0].name).toBe('Alicia');
    expect(ts.pendingChanges.size).toBe(1);
    const [key, change] = [...ts.pendingChanges.entries()][0];
    expect(key).toBe('["id":1]');
    expect(change.changedColumns).toEqual(['name']);
    expect(change.originalValues).toEqual({ name: 'Alice' });
    expect(change.currentValues).toEqual({ name: 'Alicia' });
    expect(change.deleteMarked).toBe(false);
    expect(ts.rowIdentityAnchors.get(0)).toBe('["id":1]');
    expect(ts.error).toBeNull();
  });

  it('drops the staged change when the value is reverted to the original', async () => {
    await load();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Alicia');
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Alice');

    expect(slice().pendingChanges.size).toBe(0);
  });

  it('keeps two changed columns of one row as one entry', async () => {
    await load();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Alicia');
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'score', 11);

    const ts = slice();
    expect(ts.pendingChanges.size).toBe(1);
    expect([...ts.pendingChanges.values()][0].changedColumns).toEqual(['name', 'score']);
  });

  it('ignores a cell change on a row that is not on the page', async () => {
    await load();
    useTableDataStore.getState().stageCellChange(PANEL, 99, 'name', 'Nobody');

    expect(slice().pendingChanges.size).toBe(0);
    expect(slice().error).toBeNull();
  });

  it('marks a row for deletion and keeps its staged column changes', async () => {
    await load();
    useTableDataStore.getState().stageCellChange(PANEL, 1, 'name', 'Bobby');
    useTableDataStore.getState().stageRowDelete(PANEL, 1);

    const ts = slice();
    const change = [...ts.pendingChanges.values()][0];
    expect(change.deleteMarked).toBe(true);
    expect(change.changedColumns).toEqual(['name']);
    expect(change.originalValues).toEqual({ name: 'Bob' });
    expect(change.currentValues).toEqual({ name: 'Bobby' });
  });

  it('normalises the row indices of a delete mark', async () => {
    await load();
    useTableDataStore.getState().stageRowDelete(PANEL, [2, 0, 2, -1, 1.5]);

    const ts = slice();
    expect(ts.pendingChanges.size).toBe(2);
    expect([...ts.pendingChanges.keys()].sort()).toEqual(['["id":1]', '["id":3]']);
  });

  it('does not touch the row selection when rows are deleted', async () => {
    await load();
    useTableDataStore.getState().setRowModeSelection(PANEL, 0);
    useTableDataStore.getState().deleteRows(PANEL, [1]);

    const ts = slice();
    expect([...ts.selectedRows]).toEqual([0]);
    expect(ts.pendingChanges.size).toBe(1);
    expect([...ts.pendingChanges.values()][0].deleteMarked).toBe(true);
  });

  it('stages the same value in every listed row', async () => {
    await load();
    useTableDataStore.getState().applyColumnToRows(PANEL, 'score', 99, [0, 2]);

    const ts = slice();
    expect(ts.pendingChanges.size).toBe(2);
    expect(ts.rows[0].score).toBe(99);
    expect(ts.rows[1].score).toBe(20);
    expect(ts.rows[2].score).toBe(99);
  });

  it('refuses to stage without a primary key and writes no change', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns: [{ name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true }],
      rows: [['Alice']] as (string | number | null)[][],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    });
    await load(NO_PK_PANEL);

    useTableDataStore.getState().stageCellChange(NO_PK_PANEL, 0, 'name', 'Bob');
    expect(slice(NO_PK_PANEL).pendingChanges.size).toBe(0);
    expect(slice(NO_PK_PANEL).error).toBeTruthy();

    useTableDataStore.getState().stageRowDelete(NO_PK_PANEL, 0);
    expect(slice(NO_PK_PANEL).pendingChanges.size).toBe(0);
    expect(slice(NO_PK_PANEL).error).toBeTruthy();
  });

  it('rolls back every staged change and re-reads the page', async () => {
    await load();
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Alicia');
    useTableDataStore.getState().stageRowDelete(PANEL, 2);
    mockDatabaseCommands.getTableData.mockClear();

    useTableDataStore.getState().rollbackPendingChanges(PANEL);

    const ts = slice();
    expect(ts.pendingChanges.size).toBe(0);
    expect(ts.draftInserts.size).toBe(0);
    expect(ts.previewPlan).toBeNull();
    expect(ts.pendingStatus).toBe('idle');
    expect(ts.error).toBeNull();
    expect(mockDatabaseCommands.getTableData).toHaveBeenCalled();
  });

  it('ignores every staged-edit action on a panel without a slice', () => {
    const state = useTableDataStore.getState();
    expect(() => {
      state.stageCellChange('missing', 0, 'name', 'x');
      state.stageRowDelete('missing', 0);
      state.deleteRows('missing', [0]);
      state.applyColumnToRows('missing', 'name', 'x', [0]);
      state.rollbackPendingChanges('missing');
    }).not.toThrow();
    expect(useTableDataStore.getState().byPanel.has('missing')).toBe(false);
  });
});
