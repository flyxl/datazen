/**
 * Review audit of the insert-frontend track (04 §3 / §5 Step 7 / §7).
 *
 * These cases are **independent** of the track's own suites: they re-derive the
 * tri-state write contract, the wire shape of a draft and the "where did the
 * new row end up" rules from the contract text, so that a test which asserts
 * on its own fixture cannot hide a regression in the production path.
 *
 * The tri-state contract (00-contracts §3):
 *   - key present with a value → the column is written with that value;
 *   - key present with `null`  → the column is written as SQL `NULL`;
 *   - key absent               → the column is **not written** and the database
 *                                 keeps its own default.
 */
import { describe, expect, it, vi, beforeEach } from 'vitest';
import { emptyTableState } from '../connectionState';
import { withDraftValue } from '../draftInserts';
import { draftsForWire, hasPendingWork, pendingChangesForWire } from '../pendingChanges';
import { locateInsertedRow } from '../insertLocation';
import { draftWriteColumns, type PendingRowChange } from '../../../lib/tableChanges';
import type { TableState } from '../types';
import type { ColumnSchema, Value } from '../../../types';

const mockDatabaseCommands = {
  getTableData: vi.fn(),
  previewPendingChanges: vi.fn(),
  commitPendingChanges: vi.fn(),
};

vi.mock('../../../lib/session/tableSession', () => ({ tableManualHandle: vi.fn(() => null) }));
vi.mock('../../../commands/database', () => ({ databaseCommands: mockDatabaseCommands }));

const CONTEXT = {
  connectionId: 'connection-1',
  dbSessionId: 'session-1',
  driverType: 'postgresql',
  database: 'app',
  schema: 'public',
  table: 'users',
};

const PANEL = 'panel-users';

function stateWithDrafts(
  drafts: { draftId: string; values: Record<string, Value | null> }[],
  order?: string[],
): TableState {
  const base = emptyTableState(CONTEXT);
  const draftInserts = new Map(base.draftInserts);
  for (const draft of drafts) draftInserts.set(draft.draftId, { ...draft });
  return {
    ...base,
    draftInserts,
    draftInsertOrder: order ?? drafts.map((draft) => draft.draftId),
  };
}

describe('A · the tri-state write contract of a draft', () => {
  it('writes a filled column and takes it out of the "not written" set', () => {
    const ts = stateWithDrafts([{ draftId: 'draft-1', values: { name: 'Alice' } }]);

    const [wire] = draftsForWire(ts);

    expect(wire.changedColumns).toEqual(['name']);
    expect(wire.currentValues).toEqual({ name: 'Alice' });
  });

  it('treats a cleared column as "not written" — the key is gone, not null', () => {
    const ts = stateWithDrafts([{ draftId: 'draft-1', values: { name: 'Alice' } }]);

    // The store action is the one the cell editor calls when the user empties
    // the cell: `undefined` removes the key.
    const next = { ...ts, ...withDraftValue(ts, 'draft-1', 'name', undefined) };

    expect(next.draftInserts.get('draft-1')?.values).toEqual({});
    expect('name' in (next.draftInserts.get('draft-1')?.values ?? {})).toBe(false);

    const [wire] = draftsForWire(next);
    // The column disappeared from the statement: the database fills it.
    expect(wire.changedColumns).toEqual([]);
    expect(wire.currentValues).toEqual({});
    // And it is **not** written as NULL.
    expect(wire.currentValues).not.toHaveProperty('name');
  });

  it('does not mistake 0, false or an empty string for "not filled"', () => {
    const ts = stateWithDrafts([
      {
        draftId: 'draft-1',
        values: { id: 0, name: '', note: false },
      },
    ]);

    const [wire] = draftsForWire(ts);

    expect(wire.changedColumns).toEqual(['id', 'name', 'note']);
    expect(wire.currentValues).toEqual({ id: 0, name: '', note: false });
    // The falsy values survive the trip to the wire.
    expect(JSON.parse(JSON.stringify(wire)).currentValues).toEqual({
      id: 0,
      name: '',
      note: false,
    });
  });

  it('keeps an explicit NULL distinguishable from "not written" on the wire', () => {
    const explicitNull = stateWithDrafts([
      { draftId: 'draft-1', values: { name: null } },
    ]);
    const notWritten = stateWithDrafts([{ draftId: 'draft-1', values: {} }]);

    const [nullWire] = draftsForWire(explicitNull);
    const [absentWire] = draftsForWire(notWritten);

    // The column list differs …
    expect(nullWire.changedColumns).toEqual(['name']);
    expect(absentWire.changedColumns).toEqual([]);
    // … and so does the serialized payload, byte for byte.
    expect(JSON.stringify(nullWire)).not.toBe(JSON.stringify(absentWire));
    expect(JSON.parse(JSON.stringify(nullWire)).currentValues).toEqual({ name: null });
    expect(JSON.parse(JSON.stringify(absentWire)).currentValues).toEqual({});
  });

  it('sorts the written columns, whatever order the user typed them in', () => {
    expect(draftWriteColumns({ zeta: 'z', alpha: 'a', mid: 'm' })).toEqual([
      'alpha',
      'mid',
      'zeta',
    ]);
    // A falsy value is a written column too.
    expect(draftWriteColumns({ n: 0, b: false, s: '' })).toEqual(['b', 'n', 's']);
  });
});

/**
 * `withDraftValue` lives in the pure draft module, so the tri-state rules can
 * be exercised without a store.
 */

describe('A3 · the drafts go on the wire in creation order, not in map order', () => {
  it('follows `draftInsertOrder` even when the map iterates in another order', () => {
    // The map holds the drafts in the opposite order of the creation order —
    // exactly what happens when a draft is removed and re-added.
    const ts = stateWithDrafts(
      [
        { draftId: 'draft-1', values: { name: 'first' } },
        { draftId: 'draft-2', values: { name: 'second' } },
      ],
      ['draft-1', 'draft-2'],
    );
    const reversed = new Map(ts.draftInserts);
    ts.draftInserts = new Map([...reversed.entries()].reverse());

    const wire = draftsForWire(ts);

    expect(wire.map((change) => change.currentValues.name)).toEqual(['first', 'second']);
  });

  it('follows the creation order when it is not the map order', () => {
    const ts = stateWithDrafts(
      [
        { draftId: 'draft-1', values: { name: 'first' } },
        { draftId: 'draft-2', values: { name: 'second' } },
      ],
      ['draft-2', 'draft-1'],
    );

    const wire = draftsForWire(ts);

    expect(wire.map((change) => change.currentValues.name)).toEqual(['second', 'first']);
  });
});

describe('B · the wire shape of a draft and of an ordinary change', () => {
  it('marks a draft as an insert and leaves the identity empty', () => {
    const ts = stateWithDrafts([{ draftId: 'draft-1', values: { name: 'Alice' } }]);

    const [wire] = draftsForWire(ts);

    expect(wire.insertMarked).toBe(true);
    expect(wire.rowIdentity).toEqual({});
    expect(wire.originalValues).toEqual({});
    expect(wire.deleteMarked).toBe(false);
  });

  it('keeps the insert marker out of the wire payload of an ordinary change', () => {
    // The production wire function, not a hand-rolled copy of it: the property
    // is "the payload of a change without inserts carries no `insertMarked`
    // key at all", which is what keeps the fingerprint of an insert-free
    // change set byte-identical to the payload that predates the flag.
    const changes = new Map<string, PendingRowChange>();
    changes.set('["id":1]', {
      rowIndex: 0,
      rowIdentity: { id: 1 },
      originalValues: { id: 1, name: 'Alice' },
      currentValues: { id: 1, name: 'Bob' },
      changedColumns: ['name'],
      deleteMarked: false,
    });

    const wire = pendingChangesForWire(changes);

    expect(wire[0]).not.toHaveProperty('insertMarked');
    expect(JSON.parse(JSON.stringify(wire))[0]).not.toHaveProperty('insertMarked');
    // The key is **absent**, not present-and-false.
    expect(Object.keys(wire[0])).not.toContain('insertMarked');
  });

  it('counts a panel with only drafts as work', () => {
    expect(hasPendingWork(stateWithDrafts([]))).toBe(false);
    expect(hasPendingWork(stateWithDrafts([{ draftId: 'draft-1', values: {} }]))).toBe(true);
  });
});

describe('D · where the new row ended up', () => {
  const PK: ColumnSchema[] = [{ name: 'id', dataType: 'integer', isPrimaryKey: true } as ColumnSchema];

  it('locates the row exactly when the statement carries an identity', () => {
    const result = locateInsertedRow(
      [{ id: 1 }],
      [{ id: 1 }, { id: 2 }],
      PK,
      [{ operation: 'insert', rowIdentity: { id: 2 }, affectedRows: 1 }],
    );

    expect(result.status).toBe('located');
    expect(result.key).toBe('["id":2]');
  });

  it('falls back to the maximum primary key when it grew', () => {
    const result = locateInsertedRow(
      [{ id: 1 }, { id: 2 }],
      [{ id: 1 }, { id: 2 }, { id: 3 }],
      PK,
      [{ operation: 'insert', rowIdentity: {}, affectedRows: 1 }],
    );

    expect(result.status).toBe('located');
    expect(result.key).toBe('["id":3]');
  });

  it('does not locate anything when the maximum primary key did not grow', () => {
    // The refreshed page still holds the old maximum: the candidate would be a
    // row that was already there, so the interface must not point at it.
    const result = locateInsertedRow(
      [{ id: 1 }, { id: 5 }],
      [{ id: 1 }, { id: 5 }],
      PK,
      [{ operation: 'insert', rowIdentity: {}, affectedRows: 1 }],
    );

    expect(result.status).toBe('unavailable');
    expect(result.key).toBeNull();
  });

  it('does not locate anything when the maximum primary key shrank', () => {
    const result = locateInsertedRow(
      [{ id: 9 }],
      [{ id: 3 }],
      PK,
      [{ operation: 'insert', rowIdentity: {}, affectedRows: 1 }],
    );

    expect(result.status).toBe('unavailable');
  });

  it('does not locate anything on a composite primary key', () => {
    const composite: ColumnSchema[] = [
      { name: 'tenant', dataType: 'integer', isPrimaryKey: true } as ColumnSchema,
      { name: 'id', dataType: 'integer', isPrimaryKey: true } as ColumnSchema,
    ];
    const result = locateInsertedRow(
      [{ tenant: 1, id: 1 }],
      [{ tenant: 1, id: 1 }, { tenant: 1, id: 2 }],
      composite,
      [{ operation: 'insert', rowIdentity: {}, affectedRows: 1 }],
    );

    expect(result.status).toBe('unavailable');
  });

  it('reports "not applicable" when the commit contained no insert', () => {
    const result = locateInsertedRow(
      [{ id: 1 }],
      [{ id: 1 }, { id: 2 }],
      PK,
      [{ operation: 'update', rowIdentity: { id: 1 }, affectedRows: 1 }],
    );

    expect(result.status).toBe('not-applicable');
    expect(result.key).toBeNull();
  });
});

describe('the store wiring of the drafts', () => {
  let useTableDataStore: typeof import('../../tableDataStore').useTableDataStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    mockDatabaseCommands.getTableData.mockResolvedValue({
      columns: [
        { name: 'id', dataType: 'integer', isPrimaryKey: true, isNullable: false },
        { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
      ],
      rows: [
        [1, 'Alice'],
        [2, 'Bob'],
      ],
      totalRows: 2,
      page: 0,
      pageSize: 50,
    });
    mockDatabaseCommands.previewPendingChanges.mockResolvedValue({
      planId: 'plan-1',
      fingerprint: 'fingerprint-1',
      table: { ...CONTEXT, schema: null },
      updates: [],
      deletes: [],
      inserts: [],
      warnings: [],
    });
    mockDatabaseCommands.commitPendingChanges.mockResolvedValue({
      planId: 'plan-1',
      fingerprint: 'fingerprint-1',
      statements: [{ operation: 'insert', rowIdentity: {}, affectedRows: 1 }],
      affectedRows: 1,
    });
    const mod = await import('../../tableDataStore');
    useTableDataStore = mod.useTableDataStore;
    useTableDataStore.getState().reset();
    await useTableDataStore.getState().loadTableData({
      panelId: PANEL,
      dbSessionId: 'session-1',
      table: 'users',
      connectionId: 'connection-1',
      driverType: 'postgresql',
      database: 'app',
      schema: null,
    });
  });

  const slice = (): TableState => {
    const ts = useTableDataStore.getState().byPanel.get(PANEL);
    if (!ts) throw new Error(`panel ${PANEL} has no table-data slice`);
    return ts;
  };

  it('keeps the drafts out of the real row set', () => {
    const rowsBefore = slice().rows;
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'Alice');

    expect(slice().rows).toBe(rowsBefore);
    expect(slice().rows).toHaveLength(2);
    expect(slice().draftInserts.size).toBe(1);
  });

  it('sends the drafts in the changes array and adds no request field', async () => {
    const draftId = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, draftId, 'name', 'Alice');

    await useTableDataStore.getState().previewPendingChanges(PANEL);

    const request = mockDatabaseCommands.previewPendingChanges.mock.calls[0][0];
    expect(Object.keys(request).sort()).toEqual(['changes', 'context']);
    expect(request).not.toHaveProperty('inserts');
    expect(request.changes).toHaveLength(1);
    expect(request.changes[0].insertMarked).toBe(true);
    expect(request.changes[0].changedColumns).toEqual(['name']);
    expect(request.changes[0].currentValues).toEqual({ name: 'Alice' });
  });

  it('distinguishes an explicit NULL from a cleared cell on the wire', async () => {
    const explicit = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, explicit, 'name', null);
    const cleared = useTableDataStore.getState().addDraftRow(PANEL)!;
    useTableDataStore.getState().setDraftValue(PANEL, cleared, 'name', 'x');
    useTableDataStore.getState().setDraftValue(PANEL, cleared, 'name', undefined);

    await useTableDataStore.getState().previewPendingChanges(PANEL);

    const request = mockDatabaseCommands.previewPendingChanges.mock.calls[0][0];
    const byIdentity = request.changes.map((change: PendingRowChange) => change.currentValues);
    expect(byIdentity).toEqual([{ name: null }, {}]);
    expect(JSON.stringify(byIdentity[0])).not.toBe(JSON.stringify(byIdentity[1]));
  });
});
