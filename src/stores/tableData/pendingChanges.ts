import type { ColumnSchema } from '../../types';
import {
  buildRowIdentity,
  clonePendingRowChange,
  draftWriteColumns,
  rowIdentityKey,
  type PendingInsert,
  type PendingRowChange,
} from '../../lib/tableChanges';
import type { TableState } from './types';

export const AMBIGUOUS_ROW_IDENTITY_ERROR =
  'Row identity is NULL, unstable, or ambiguous; UPDATE/DELETE was not staged.';

export function effectivePendingIdentity(
  change: PendingRowChange,
  pkColumns: ColumnSchema[],
): ReturnType<typeof buildRowIdentity> {
  const actualPkColumns = pkColumns.filter((c) => c.isPrimaryKey);
  const targetColumns = actualPkColumns.length > 0 ? actualPkColumns : pkColumns;
  const values: Record<string, unknown> = { ...change.rowIdentity };
  for (const column of targetColumns) {
    if (Object.prototype.hasOwnProperty.call(change.currentValues, column.name)) {
      values[column.name] = change.currentValues[column.name];
    }
  }
  return buildRowIdentity(values, targetColumns);
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

/**
 * "Is there work that has to be committed?" — staged edits **or** new-row
 * drafts (04 §3.7 R-2).
 *
 * The two collections are independent: a panel can hold drafts without a
 * single staged edit (that is the normal state right after "+ row"), and a
 * panel can hold edits without drafts. Every gate that used to ask
 * `pendingChanges.size === 0` has to ask this instead, otherwise a panel with
 * only drafts would keep its commit button greyed out forever.
 */
export function hasPendingWork(ts: TableState): boolean {
  return ts.pendingChanges.size > 0 || ts.draftInserts.size > 0;
}

/**
 * The drafts as the wire sees them: one `PendingRowChange` per draft, marked
 * with `insertMarked: true`.
 *
 * The order is `draftInsertOrder`, never the `Map` iteration order: the
 * insertion order decides which row gets which auto-increment value, and the
 * back end keeps the request order (it must not sort the insert statements).
 *
 * The draft id never goes on the wire — it is a UI association key only. The
 * shape is exactly the one an existing row change has, plus the marker:
 * `rowIdentity` / `originalValues` are empty (a new row has neither), and
 * `changedColumns` is the exact list of columns to write.
 */
export function draftsForWire(ts: TableState): PendingRowChange[] {
  const changes: PendingRowChange[] = [];
  for (const draftId of ts.draftInsertOrder) {
    const draft = ts.draftInserts.get(draftId);
    if (!draft) continue;
    changes.push(
      clonePendingRowChange({
        rowIdentity: {},
        originalValues: {},
        currentValues: { ...draft.values },
        changedColumns: draftWriteColumns(draft.values),
        deleteMarked: false,
        insertMarked: true,
      }),
    );
  }
  return changes;
}

/**
 * Signature of everything that has to be committed (staged edits **and**
 * drafts), used to detect "the user changed something while the preview was in
 * flight" (04 §3.7 R-3).
 *
 * `pendingChangesSignature` is left untouched — it keeps its single-argument
 * signature — and is reused here for the staged part. The draft part keeps the
 * creation order, because for drafts the order *is* part of the intent.
 */
export function pendingWorkSignature(input: {
  pendingChanges: Map<string, PendingRowChange>;
  draftInserts: Map<string, PendingInsert>;
  draftInsertOrder: string[];
}): string {
  const drafts = input.draftInsertOrder.map((draftId) => {
    const draft = input.draftInserts.get(draftId);
    return [draftId, draft ? draft.values : null];
  });
  return JSON.stringify([pendingChangesSignature(input.pendingChanges), drafts]);
}
