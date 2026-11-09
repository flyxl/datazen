import { describe, expect, it } from 'vitest';
import { emptyTableState } from '../connectionState';
import {
  draftInsertCount,
  orderedDrafts,
  withDraftRowsFromCopy,
  withNewDraft,
} from '../draftInserts';
import {
  draftsForWire,
  hasPendingWork,
  pendingChangesForWire,
  pendingWorkSignature,
} from '../pendingChanges';
import type { TableState } from '../types';
import type { TableChangeContext } from '../../../lib/tableChanges';

const CONTEXT: TableChangeContext = {
  connectionId: 'connection-1',
  dbSessionId: 'session-1',
  driverType: 'postgresql',
  database: 'app',
  schema: 'public',
  table: 'users',
};

function stateWith(
  ...drafts: { draftId: string; values: Record<string, string | number | null> }[]
) {
  const base: TableState = emptyTableState(CONTEXT);
  const draftInserts = new Map(base.draftInserts);
  for (const draft of drafts) {
    draftInserts.set(draft.draftId, { draftId: draft.draftId, values: draft.values });
  }
  return {
    ...base,
    draftInserts,
    draftInsertOrder: drafts.map((draft) => draft.draftId),
  };
}

describe('draft rows (store layer)', () => {
  it('round-trips the wire shape of a draft', () => {
    const ts = stateWith({ draftId: 'draft-1', values: { name: 'ab', note: null } });

    const [wire] = draftsForWire(ts);

    expect(wire.insertMarked).toBe(true);
    expect(wire.rowIdentity).toEqual({});
    expect(wire.originalValues).toEqual({});
    expect(wire.deleteMarked).toBe(false);
    // The written columns are sorted; the cleared ones are simply absent.
    expect(wire.changedColumns).toEqual(['name', 'note']);
    // An explicit NULL stays NULL on the wire — it is not the same as "not
    // written", which is expressed by the missing key.
    expect(wire.currentValues).toEqual({ name: 'ab', note: null });
    expect(JSON.parse(JSON.stringify(wire))).toMatchObject({
      insertMarked: true,
      changedColumns: ['name', 'note'],
      currentValues: { name: 'ab', note: null },
    });
  });

  it('keeps the insert marker out of the payload of an ordinary row change', () => {
    const ts = stateWith();
    ts.pendingChanges.set('["id":1]', {
      rowIdentity: { id: 1 },
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Bob' },
      changedColumns: ['name'],
      deleteMarked: false,
    });

    // The **production** wire function, not a hand-rolled copy of it: the
    // property is that the payload of a change without inserts carries no
    // `insertMarked` key at all, so a production change that always wrote
    // `insertMarked: false` would turn this red.
    const wire = pendingChangesForWire(ts.pendingChanges);

    expect(Object.keys(wire[0])).not.toContain('insertMarked');
    expect(wire[0]).not.toHaveProperty('insertMarked');
    expect(JSON.parse(JSON.stringify(wire))[0]).not.toHaveProperty('insertMarked');
  });

  it('sends the drafts in creation order, not in map order', () => {
    const ts = stateWith(
      { draftId: 'draft-1', values: { name: 'first' } },
      { draftId: 'draft-2', values: { name: 'second' } },
    );
    // The map order and the creation order disagree on purpose.
    const reordered = new Map(ts.draftInserts);
    ts.draftInserts = new Map([...reordered.entries()].reverse());
    ts.draftInsertOrder = ['draft-1', 'draft-2'];

    const wire = draftsForWire(ts);

    expect(wire.map((change) => change.currentValues.name)).toEqual(['first', 'second']);
    expect(orderedDrafts(ts).map((draft) => draft.draftId)).toEqual(['draft-1', 'draft-2']);
  });

  it('counts pending work across both collections', () => {
    const empty = stateWith();
    expect(hasPendingWork(empty)).toBe(false);
    expect(draftInsertCount(empty)).toBe(0);

    const withDraft = stateWith({ draftId: 'draft-1', values: { name: 'ab' } });
    // A panel with only drafts still has work to commit.
    expect(hasPendingWork(withDraft)).toBe(true);
    expect(draftInsertCount(withDraft)).toBe(1);

    withDraft.pendingChanges.set('["id":1]', {
      rowIdentity: { id: 1 },
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Bob' },
      changedColumns: ['name'],
      deleteMarked: false,
    });
    expect(hasPendingWork(withDraft)).toBe(true);
  });

  it('changes the work signature when a draft changes', () => {
    const ts = stateWith({ draftId: 'draft-1', values: { name: 'ab' } });
    const before = pendingWorkSignature(ts);

    const changed = stateWith({ draftId: 'draft-1', values: { name: 'abc' } });
    expect(pendingWorkSignature(changed)).not.toBe(before);

    // Removing the draft changes it as well — the preview must not survive it.
    expect(pendingWorkSignature(stateWith())).not.toBe(before);
  });

  it('appends a draft to the end of the creation order', () => {
    const ts = stateWith({ draftId: 'draft-1', values: { name: 'ab' } });
    const next = { ...ts, ...withNewDraft(ts, 'draft-2') };

    expect(next.draftInsertOrder).toEqual(['draft-1', 'draft-2']);
    expect(next.draftInserts.get('draft-2')?.values).toEqual({});
  });
});

describe('draft rows from a row copy (DBR-02)', () => {
  it('appends prefilled drafts in copy order, preserving NULL vs absent keys', () => {
    const ts = stateWith({ draftId: 'existing', values: { name: 'already' } });
    const next = withDraftRowsFromCopy(ts, [
      { draftId: 'copy-1', values: { name: 'first', note: null } },
      { draftId: 'copy-2', values: {} },
    ]);

    expect(next.draftInsertOrder).toEqual(['existing', 'copy-1', 'copy-2']);
    expect(next.draftInserts?.get('copy-1')).toEqual({
      draftId: 'copy-1',
      values: { name: 'first', note: null },
    });
    // An empty record is a valid draft: every column keeps its default.
    expect(next.draftInserts?.get('copy-2')).toEqual({ draftId: 'copy-2', values: {} });
    // The copied drafts are pending work and invalidate any previewed plan.
    expect(next.previewPlan).toBeNull();
    expect(hasPendingWork({ ...ts, ...next } as TableState)).toBe(true);
  });

  it('an empty copy is a no-op patch', () => {
    const ts = stateWith();
    expect(withDraftRowsFromCopy(ts, [])).toEqual({});
  });
});
