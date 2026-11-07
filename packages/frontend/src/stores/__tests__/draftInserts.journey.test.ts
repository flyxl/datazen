import { describe, expect, it, vi, beforeEach } from 'vitest';
import { draftWriteColumns } from '../../lib/tableChanges';
import { classifyGridError } from '../../lib/gridErrors';
import type { TableState } from '../tableData/types';

/**
 * The continuous journey of a new row (04 §9.5).
 *
 * Every step is a **transition**, not a static state: the draft appears, takes
 * a value, survives an interrupted composition, survives Escape, is duplicated,
 * is discarded, is cleared, is previewed and is committed — including the
 * residual intermediate states (a row with only one filled column, a row that
 * was filled and cleared again, and a commit that fails).
 *
 * The UI entry points are exercised in
 * `src/windows/connection/__tests__/TableView.insert.test.tsx`; each step
 * below names the action the interface calls.
 */

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));

vi.mock('../../commands/database', () => ({
  databaseCommands: mockDatabaseCommands,
}));

const samplePlan = {
  planId: 'plan-1',
  fingerprint: 'fingerprint-1',
  table: {
    connectionId: 'conn-1',
    dbSessionId: 'conn-1',
    driverType: 'postgres',
    database: 'app',
    schema: null,
    table: 'events',
  },
  updates: [],
  deletes: [],
  inserts: [
    {
      rowIdentity: {},
      originalValues: {},
      currentValues: { name: 'ab' },
      changedColumns: ['name'],
      sqlTemplate: 'INSERT INTO "events" ("name") VALUES ($1)',
      parameterSummary: ['INSERT name=ab'],
    },
  ],
  warnings: [],
};

const PANEL = 'panel-events';

describe('journey: a new row from "+ row" to the commit', () => {
  let useTableDataStore: typeof import('../tableDataStore').useTableDataStore;

  const slice = (): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(PANEL);
    if (!ts) throw new Error(`panel ${PANEL} has no table-data slice`);
    return ts;
  };

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    // A table without a primary key: inserting is allowed there (04 §7 #5).
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns: [{ name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true }],
      rows: [['Alice']] as (string | number | null)[][],
      totalRows: 1,
      page: 0,
      pageSize: 50,
    });
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
    await useTableDataStore.getState().loadTableData({
      panelId: PANEL,
      dbSessionId: 'conn-1',
      table: 'events',
      connectionId: 'conn-1',
      driverType: 'postgres',
      database: 'app',
      schema: null,
    });
  });

  it('walks from the empty draft to a committed insert, and back on failure', async () => {
    // ── Step 1: "+ row" ────────────────────────────────────────────────────
    const rowsBefore = slice().rows;
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    expect(slice().draftInserts.size).toBe(1);
    expect(slice().draftInsertOrder).toEqual([draftId]);
    // The draft is a derived row: the real row set is untouched (R-5).
    expect(slice().rows).toBe(rowsBefore);
    expect(slice().rows).toHaveLength(1);

    // ── Step 2: type "ab" into `name` ──────────────────────────────────────
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'ab');
    expect(slice().draftInserts.get(draftId)?.values).toEqual({ name: 'ab' });
    expect(draftWriteColumns(slice().draftInserts.get(draftId)!.values)).toEqual(['name']);

    // ── Step 3: an interrupted composition ────────────────────────────────
    // The composition keeps its text inside the editor: the store only ever
    // sees the committed value, so nothing lands in the middle of the
    // composition and no preview is triggered.
    const signatureBeforeComposition = slice().draftInserts.get(draftId)!.values;
    expect(signatureBeforeComposition).toEqual({ name: 'ab' });
    expect(mockDatabaseCommands.previewPendingChanges).not.toHaveBeenCalled();
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'abc');
    expect(slice().draftInserts.get(draftId)?.values).toEqual({ name: 'abc' });

    // ── Step 4: Escape must not throw away a row that has content ─────────
    // Escape only discards the **empty** drafts; a filled one stays.
    const emptyDraftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    expect(slice().draftInserts.get(emptyDraftId)?.values).toEqual({});
    // The interface removes the empty draft (the Escape shortcut of the table
    // view); the filled one is untouched by that step.
    useTableDataStore.getState().removeDraftRow(PANEL, emptyDraftId);
    expect(slice().draftInserts.has(draftId)).toBe(true);
    expect(slice().draftInserts.has(emptyDraftId)).toBe(false);

    // ── Step 5: duplicate the draft (Cmd/Ctrl+D) ──────────────────────────
    const copyId = useTableDataStore.getState().duplicateDraftRow(PANEL, draftId)!;
    expect(copyId).not.toBe(draftId);
    expect(slice().draftInserts.size).toBe(2);
    expect(slice().draftInsertOrder).toEqual([draftId, copyId]);
    expect(slice().draftInserts.get(copyId)?.values).toEqual({ name: 'abc' });
    // The copy is deep: changing one row does not change the other.
    useTableDataStore.getState().setDraftValue(PANEL, copyId, 'name', 'other');
    expect(slice().draftInserts.get(draftId)?.values).toEqual({ name: 'abc' });

    // ── Step 6: discard one of them (Delete) ──────────────────────────────
    useTableDataStore.getState().removeDraftRow(PANEL, copyId);
    expect(slice().draftInserts.size).toBe(1);

    // ── Step 7: clear the cell again ──────────────────────────────────────
    // A cleared cell loses its key: the column is not written any more.
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', undefined);
    expect(slice().draftInserts.get(draftId)?.values).toEqual({});
    expect(draftWriteColumns(slice().draftInserts.get(draftId)!.values)).toEqual([]);

    // ── Step 8: preview, then commit ──────────────────────────────────────
    const plan = await useTableDataStore.getState().previewPendingChanges(PANEL);
    expect(plan).not.toBeNull();
    const previewRequest = mockDatabaseCommands.previewPendingChanges.mock.calls[0][0];
    // The request shape does not change: one array, no `inserts` field.
    expect(Object.keys(previewRequest).sort()).toEqual(['changes', 'context']);
    expect(previewRequest.changes).toHaveLength(1);
    expect(previewRequest.changes[0].insertMarked).toBe(true);
    expect(previewRequest.changes[0].changedColumns).toEqual([]);
    expect(previewRequest.changes[0].currentValues).toEqual({});

    // Put a value back so the commit has something to write.
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'ab');
    const result = await useTableDataStore.getState().commitPendingChanges(PANEL);
    expect(result.status).toBe('committed');
    const commitRequest = mockDatabaseCommands.commitPendingChanges.mock.calls[0][0];
    expect(commitRequest.plan.inserts).toHaveLength(1);
    expect(commitRequest.plan.inserts[0].changedColumns).toEqual(['name']);
    // The commit went through the same plan the preview produced.
    expect(commitRequest.fingerprint).toBe(samplePlan.fingerprint);
    // The drafts are gone, and the page was refreshed.
    expect(slice().draftInserts.size).toBe(0);
    expect(slice().draftInsertOrder).toEqual([]);
    expect(result.refreshed).toBe(true);
    // The driver returned no primary key and the refreshed page carries no new
    // one, so the new row could not be located: the interface has to say so.
    expect(result.insertLocation).toBe('unavailable');
    expect(slice().highlightedNewRowKey).toBeNull();
  });

  it('keeps every draft when the commit is rejected', async () => {
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'ab');
    mockDatabaseCommands.commitPendingChanges.mockRejectedValueOnce(
      new Error('grid.commit.affectedMismatch: INSERT affected 0 rows; refusing ambiguous write'),
    );

    const result = await useTableDataStore.getState().commitPendingChanges(PANEL);

    expect(result.status).toBe('failed');
    // The staged work survives the failure — the user can fix and retry.
    expect(slice().draftInserts.size).toBe(1);
    expect(slice().draftInserts.get(draftId)?.values).toEqual({ name: 'ab' });
    // The error is classified before it is shown.
    expect(classifyGridError(slice().error ?? '')).toBe('grid.commit.affectedMismatch');
  });
});
