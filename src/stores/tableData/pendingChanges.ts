import type { ColumnSchema } from '../../types';
import {
  buildRowIdentity,
  clonePendingRowChange,
  rowIdentityKey,
  type PendingRowChange,
} from '../../lib/tableChanges';
import { editKey } from './connectionState';
import type { CellEdit, TableState } from './types';

export const AMBIGUOUS_ROW_IDENTITY_ERROR =
  'Row identity is NULL, unstable, or ambiguous; UPDATE/DELETE was not staged.';

export function effectivePendingIdentity(
  change: PendingRowChange,
  pkColumns: ColumnSchema[],
): ReturnType<typeof buildRowIdentity> {
  const values: Record<string, unknown> = { ...change.rowIdentity };
  for (const column of pkColumns) {
    if (Object.prototype.hasOwnProperty.call(change.currentValues, column.name)) {
      values[column.name] = change.currentValues[column.name];
    }
  }
  return buildRowIdentity(values, pkColumns);
}

export function rowIdentityIsUnique(
  ts: TableState,
  rowIndex: number,
  identity: ReturnType<typeof buildRowIdentity>,
  pkColumns: ColumnSchema[],
): boolean {
  if (!identity) return false;
  const key = rowIdentityKey(identity);
  return (
    ts.rows.filter((row, index) => {
      if (index === rowIndex) return false;
      const other = buildRowIdentity(row, pkColumns);
      return other ? rowIdentityKey(other) === key : false;
    }).length === 0
  );
}

export function hasPendingIdentityCollision(
  ts: TableState,
  ownKey: string | null,
  identity: ReturnType<typeof buildRowIdentity>,
  pkColumns: ColumnSchema[],
): boolean {
  if (!identity) return true;
  const key = rowIdentityKey(identity);
  return [...ts.pendingChanges.entries()].some(([pendingKey, change]) => {
    if (pendingKey === ownKey) return false;
    const currentIdentity = effectivePendingIdentity(change, pkColumns);
    return currentIdentity ? rowIdentityKey(currentIdentity) === key : true;
  });
}

export function findPendingForRow(
  ts: TableState,
  rowIndex: number,
  row: Record<string, unknown>,
  pkColumns: ColumnSchema[],
): { key: string; change: PendingRowChange } | null {
  const identity = buildRowIdentity(row, pkColumns);
  if (!identity) return null;
  if (!rowIdentityIsUnique(ts, rowIndex, identity, pkColumns)) return null;

  const anchoredKey = ts.rowIdentityAnchors.get(rowIndex);
  const anchored = anchoredKey ? ts.pendingChanges.get(anchoredKey) : undefined;
  if (anchored) return { key: anchoredKey!, change: anchored };

  const directKey = rowIdentityKey(identity);
  const direct = ts.pendingChanges.get(directKey);
  if (direct) return { key: directKey, change: direct };
  return null;
}

export function rebuildEditBuffer(ts: TableState): Map<string, CellEdit> {
  const next = new Map<string, CellEdit>();
  const pkColumns = ts.columns.filter((column) => column.isPrimaryKey);
  if (pkColumns.length === 0) return next;

  ts.rows.forEach((row, rowIndex) => {
    const match = findPendingForRow(ts, rowIndex, row, pkColumns);
    if (!match || match.change.deleteMarked) return;
    for (const column of match.change.changedColumns) {
      next.set(editKey(rowIndex, column), {
        rowIndex,
        columnName: column,
        originalValue: match.change.originalValues[column],
        newValue: match.change.currentValues[column],
        pkSnapshot: { ...match.change.rowIdentity },
      });
    }
  });
  return next;
}

export function overlayPendingRows(
  ts: TableState,
  rows: Record<string, unknown>[],
): Record<string, unknown>[] {
  const pkColumns = ts.columns.filter((column) => column.isPrimaryKey);
  if (pkColumns.length === 0 || ts.pendingChanges.size === 0) return rows;
  return rows.map((row, rowIndex) => {
    const match = findPendingForRow(ts, rowIndex, row, pkColumns);
    return match ? { ...row, ...match.change.currentValues } : row;
  });
}

export function pendingChangesSignature(pendingChanges: Map<string, PendingRowChange>): string {
  return JSON.stringify(
    Array.from(pendingChanges.entries())
      .sort(([left], [right]) => left.localeCompare(right))
      .map(([key, change]) => [key, clonePendingRowChange(change)]),
  );
}

export function pendingChangesForWire(pendingChanges: Map<string, PendingRowChange>): PendingRowChange[] {
  return Array.from(pendingChanges.values()).map((change) => {
    const { rowIndex: _rowIndex, ...wireChange } = clonePendingRowChange(change);
    return wireChange;
  });
}
