import { describe, expect, it } from 'vitest';
import { buildRowIdentity, clonePendingRowChange } from '../../../lib/tableChanges';
import {
  AMBIGUOUS_ROW_IDENTITY_ERROR,
  effectivePendingIdentity,
  findPendingForRow,
  hasPendingIdentityCollision,
  overlayPendingRows,
  pendingChangesForWire,
  pendingChangesSignature,
  rebuildEditBuffer,
  rowIdentityIsUnique,
} from '../pendingChanges';
import { emptyTableState } from '../connectionState';
import type { PendingRowChange } from '../../../lib/tableChanges';

const pkColumns = [
  { name: 'id', dataType: 'int', isPrimaryKey: true, nullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, nullable: true },
];

describe('[tester] tableData/pendingChanges', () => {
  it('rowIdentityIsUnique returns false for null identity and true for single-row tables', () => {
    const ts = emptyTableState();
    ts.columns = pkColumns;
    ts.rows = [{ id: 1, name: 'a' }];
    expect(rowIdentityIsUnique(ts, 0, null, pkColumns)).toBe(false);
    const identity = buildRowIdentity(ts.rows[0]!, pkColumns);
    expect(rowIdentityIsUnique(ts, 0, identity, pkColumns)).toBe(true);
  });

  it('findPendingForRow resolves anchored and direct pending keys', () => {
    const ts = emptyTableState();
    ts.columns = pkColumns;
    ts.rows = [{ id: 1, name: 'Alice' }];
    const change: PendingRowChange = {
      rowIndex: 0,
      rowIdentity: { id: 1 },
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Bob' },
      changedColumns: ['name'],
      deleteMarked: false,
    };
    ts.pendingChanges.set('1', change);
    ts.rowIdentityAnchors.set(0, '1');

    expect(findPendingForRow(ts, 0, ts.rows[0], pkColumns)).toEqual({
      key: '1',
      change,
    });
  });

  it('hasPendingIdentityCollision detects cross-row identity conflicts', () => {
    const ts = emptyTableState();
    ts.columns = pkColumns;
    ts.pendingChanges.set('2', {
      rowIndex: 1,
      rowIdentity: { id: 2 },
      originalValues: {},
      currentValues: { id: 3 },
      changedColumns: ['id'],
      deleteMarked: false,
    });
    expect(hasPendingIdentityCollision(ts, '1', { id: 3 }, pkColumns)).toBe(true);
    expect(hasPendingIdentityCollision(ts, '2', { id: 2 }, pkColumns)).toBe(false);
  });

  it('overlayPendingRows applies staged current values', () => {
    const ts = emptyTableState();
    ts.columns = pkColumns;
    ts.pendingChanges.set('1', {
      rowIndex: 0,
      rowIdentity: { id: 1 },
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Bob' },
      changedColumns: ['name'],
      deleteMarked: false,
    });
    ts.rowIdentityAnchors.set(0, '1');
    const rows = [{ id: 1, name: 'Alice' }];
    expect(overlayPendingRows(ts, rows)).toEqual([{ id: 1, name: 'Bob' }]);
  });

  it('rebuildEditBuffer skips delete-marked rows', () => {
    const ts = emptyTableState();
    ts.columns = pkColumns;
    ts.rows = [{ id: 1, name: 'Alice' }];
    ts.pendingChanges.set('1', {
      rowIndex: 0,
      rowIdentity: { id: 1 },
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Bob' },
      changedColumns: ['name'],
      deleteMarked: true,
    });
    ts.rowIdentityAnchors.set(0, '1');
    expect(rebuildEditBuffer(ts).size).toBe(0);
  });

  it('pendingChangesSignature and pendingChangesForWire strip rowIndex', () => {
    const change: PendingRowChange = {
      rowIndex: 0,
      rowIdentity: { id: 1 },
      originalValues: { name: 'Alice' },
      currentValues: { name: 'Bob' },
      changedColumns: ['name'],
      deleteMarked: false,
    };
    const map = new Map([['1', change]]);
    expect(pendingChangesSignature(map)).toContain('"1"');
    const wire = pendingChangesForWire(map);
    expect(wire[0]).not.toHaveProperty('rowIndex');
    expect(wire[0].rowIdentity).toEqual({ id: 1 });
  });

  it('effectivePendingIdentity applies PK overrides from currentValues', () => {
    const change: PendingRowChange = {
      rowIndex: 0,
      rowIdentity: { id: 1 },
      originalValues: { id: 1 },
      currentValues: { id: 1 },
      changedColumns: ['id'],
      deleteMarked: false,
    };
    expect(effectivePendingIdentity(change, pkColumns)).toEqual({ id: 1 });
  });

  it('exports ambiguous row identity error constant', () => {
    expect(AMBIGUOUS_ROW_IDENTITY_ERROR).toMatch(/ambiguous/i);
  });
});

/**
 * The insert flag on the wire.
 *
 * `insertMarked` is what tells the backend "this row is a new-row draft, not
 * a change to an existing row". The clone that builds the wire payload used
 * to drop it, so an insert would have arrived as an update of a row that does
 * not exist. These cases pin the flag's survival — and its *absence* when
 * there is no insert, because the backend must keep receiving no key at all
 * for an insert-free plan (that is what keeps its fingerprint byte-identical
 * to the payload that predates the flag).
 */
describe('[tester] tableData/pendingChanges: the insert flag on the wire', () => {
  const insertDraft: PendingRowChange = {
    rowIdentity: {},
    originalValues: {},
    currentValues: { sku: 'A-1', note: null },
    changedColumns: ['sku', 'note'],
    deleteMarked: false,
    insertMarked: true,
  };

  const ordinaryEdit: PendingRowChange = {
    rowIndex: 0,
    rowIdentity: { id: 1 },
    originalValues: { name: 'Alice' },
    currentValues: { name: 'Bob' },
    changedColumns: ['name'],
    deleteMarked: false,
  };

  it('keeps the insert flag on a new-row draft that goes on the wire', () => {
    const wire = pendingChangesForWire(new Map([['new-1', insertDraft]]));

    expect(wire[0]?.insertMarked).toBe(true);
    expect(wire[0]?.changedColumns).toEqual(['sku', 'note']);
  });

  it('sends no insert key at all for a change that is not an insert', () => {
    const wire = pendingChangesForWire(new Map([['1', ordinaryEdit]]));

    // Not `insertMarked: false` and not `insertMarked: undefined`: the key
    // must be absent, because the backend reads a present key as "this is a
    // new-row draft".
    expect(wire[0]).not.toHaveProperty('insertMarked');
    expect(Object.keys(wire[0] ?? {})).not.toContain('insertMarked');
    expect(JSON.parse(JSON.stringify(wire[0]))).not.toHaveProperty('insertMarked');
  });

  it('clones into a different object, so editing the clone cannot reach the original', () => {
    const clone = clonePendingRowChange(insertDraft);

    expect(clone).not.toBe(insertDraft);
    expect(clone.rowIdentity).not.toBe(insertDraft.rowIdentity);
    expect(clone.originalValues).not.toBe(insertDraft.originalValues);
    expect(clone.currentValues).not.toBe(insertDraft.currentValues);
    expect(clone.changedColumns).not.toBe(insertDraft.changedColumns);

    // The flag itself is a primitive, so it is copied by value; the
    // containers around it are copies, not aliases.
    expect(clone.insertMarked).toBe(true);
    expect(clone.currentValues).toEqual(insertDraft.currentValues);
    clone.currentValues['sku'] = 'changed';
    clone.changedColumns.push('extra');
    expect(insertDraft.currentValues['sku']).toBe('A-1');
    expect(insertDraft.changedColumns).toEqual(['sku', 'note']);
  });
});
