import { describe, expect, it, vi, beforeEach } from 'vitest';
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
  inserts: [],
  warnings: [],
};

const PANEL = 'panel-users';

describe('tableDataStore (new-row drafts)', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const slice = (): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(PANEL);
    if (!ts) throw new Error(`panel ${PANEL} has no table-data slice`);
    return ts;
  };

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue(sampleResponse);
    mockDatabaseCommands.previewPendingChanges.mockResolvedValue(samplePlan);
    mockDatabaseCommands.commitPendingChanges.mockResolvedValue({
      planId: samplePlan.planId,
      fingerprint: samplePlan.fingerprint,
      statements: [{ operation: 'insert', rowIdentity: {}, affectedRows: 1 }],
      affectedRows: 1,
    });
    const mod = await import('../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    useTableDataStore.getState().reset();
  });

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

  it('adds a draft without touching the real rows', async () => {
    await loadTable();
    const rowsBefore = slice().rows;

    const draftId = useTableDataStore.getState().addDraftRow(PANEL);

    expect(draftId).toBeTruthy();
    expect(slice().draftInserts.size).toBe(1);
    expect(slice().draftInsertOrder).toEqual([draftId]);
    // A draft is a derived row, never a row of the table (04 §3.7 R-5).
    expect(slice().rows).toBe(rowsBefore);
  });

  it('previews the drafts in creation order inside the changes array', async () => {
    await loadTable();
    const first = useTableDataStore.getState().addDraftRow(PANEL)!;
    const second = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, first, 'name', 'Alice');
    useTableDataStore.getState().setDraftValue(PANEL, second, 'name', 'Bob');

    const plan = await useTableDataStore.getState().previewPendingChanges(PANEL);

    expect(plan).not.toBeNull();
    const request = mockDatabaseCommands.previewPendingChanges.mock.calls[0][0];
    // The request shape does not change: the drafts ride along in `changes`.
    expect(request).not.toHaveProperty('inserts');
    expect(Object.keys(request).sort()).toEqual(['changes', 'context']);
    expect(request.changes).toHaveLength(2);
    expect(request.changes.map((change: { insertMarked?: boolean }) => change.insertMarked)).toEqual([
      true,
      true,
    ]);
    expect(request.changes.map((change: { currentValues: unknown }) => change.currentValues)).toEqual([
      { name: 'Alice' },
      { name: 'Bob' },
    ]);
  });

  it('drops a cleared column from the written columns', async () => {
    await loadTable();
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'Alice');
    expect(slice().draftInserts.get(draftId)?.values).toEqual({ name: 'Alice' });

    // Clearing the cell removes the key: the column is not written at all.
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', undefined);

    expect(slice().draftInserts.get(draftId)?.values).toEqual({});
    await useTableDataStore.getState().previewPendingChanges(PANEL);
    const request = mockDatabaseCommands.previewPendingChanges.mock.calls[0][0];
    expect(request.changes[0].changedColumns).toEqual([]);
    expect(request.changes[0].currentValues).toEqual({});
  });

  it('does not keep a preview that was built from an older draft', async () => {
    await loadTable();
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'Alice');

    let resolvePreview: (plan: unknown) => void = () => {};
    mockDatabaseCommands.previewPendingChanges.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          resolvePreview = resolve;
        }),
    );
    const pending = useTableDataStore.getState().previewPendingChanges(PANEL);
    // The user keeps typing while the preview is in flight.
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'Alice B');
    resolvePreview(samplePlan);
    await pending;

    expect(slice().previewPlan).toBeNull();
  });

  it('clears the drafts after a successful commit', async () => {
    await loadTable();
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'Alice');

    const result = await useTableDataStore.getState().commitPendingChanges(PANEL);

    expect(result.status).toBe('committed');
    expect(slice().draftInserts.size).toBe(0);
    expect(slice().draftInsertOrder).toEqual([]);
    expect(slice().pendingChanges.size).toBe(0);
  });

  it('reports that the new row could not be located when it is not on the page', async () => {
    await loadTable();
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'Alice');

    const result = await useTableDataStore.getState().commitPendingChanges(PANEL);

    // The driver returned no identity and the refreshed page does not carry a
    // new primary key, so the row cannot be located: the interface has to say
    // so instead of claiming a highlight.
    expect(result.insertLocation).toBe('unavailable');
    expect(slice().highlightedNewRowKey).toBeNull();
  });

  it('locates the new row when the refreshed page carries a grown primary key', async () => {
    await loadTable();
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'Alice');
    mockDatabaseCommands.getTableData.mockResolvedValue({
      ...sampleResponse,
      rows: [
        [1, 'Alice'],
        [2, 'Bob'],
        [3, 'Alice'],
      ],
    });

    const result = await useTableDataStore.getState().commitPendingChanges(PANEL);

    expect(result.insertLocation).toBe('located');
    expect(slice().highlightedNewRowKey).toBe('["id":3]');
  });

  it('keeps the drafts when the commit fails', async () => {
    await loadTable();
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'Alice');
    mockDatabaseCommands.commitPendingChanges.mockRejectedValueOnce(
      new Error('grid.commit.affectedMismatch: INSERT affected 0 rows; refusing ambiguous write'),
    );

    const result = await useTableDataStore.getState().commitPendingChanges(PANEL);

    expect(result.status).toBe('failed');
    // Nothing is thrown away: the drafts are still there to be fixed.
    expect(slice().draftInserts.size).toBe(1);
    expect(slice().draftInserts.get(draftId)?.values).toEqual({ name: 'Alice' });
    expect(slice().error).toContain('grid.commit.affectedMismatch');
  });

  it('allows drafts on a table without a primary key but keeps cell edits disabled', async () => {
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns: [{ name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true }],
      rows: [['Alice']] as (string | number | null)[][],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    });
    await loadTable();

    const draftId = useTableDataStore.getState().addDraftRow(PANEL);

    expect(draftId).toBeTruthy();
    expect(slice().draftInserts.size).toBe(1);
    // The existing row still cannot be edited: a table without a primary key
    // has no row identity for an UPDATE.
    useTableDataStore.getState().stageCellChange(PANEL, 0, 'name', 'Bob');
    expect(slice().pendingChanges.size).toBe(0);
    expect(slice().error).toBeTruthy();
  });

  it('duplicates a draft into a new one without sharing the cells', async () => {
    await loadTable();
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'Alice');

    const copyId = useTableDataStore.getState().duplicateDraftRow(PANEL, draftId)!;

    expect(copyId).not.toBe(draftId);
    expect(slice().draftInsertOrder).toEqual([draftId, copyId]);
    expect(slice().draftInserts.get(copyId)?.values).toEqual({ name: 'Alice' });
    // The copy is independent: changing one does not change the other.
    useTableDataStore.getState().setDraftValue(PANEL, copyId, 'name', 'Bob');
    expect(slice().draftInserts.get(draftId)?.values).toEqual({ name: 'Alice' });
  });

  it('removes a draft row', async () => {
    await loadTable();
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;

    useTableDataStore.getState().removeDraftRow(PANEL, draftId);

    expect(slice().draftInserts.size).toBe(0);
    expect(slice().draftInsertOrder).toEqual([]);
  });
});
