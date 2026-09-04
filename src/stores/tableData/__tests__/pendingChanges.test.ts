import { describe, expect, it } from 'vitest';
import { buildRowIdentity } from '../../../lib/tableChanges';
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
  { name: 'id', dataType: 'int', isPrimaryKey: true, isNullable: false },
  { name: 'name', dataType: 'text', isPrimaryKey: false, isNullable: true },
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
