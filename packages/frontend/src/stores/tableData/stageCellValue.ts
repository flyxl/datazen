/**
 * The three states of a write, as a pure computation (DB-05).
 *
 * The front end does not need a new type for "write NULL / write a value /
 * do not write": the membership of `changedColumns` already carries it
 * (00-contracts §3). This module is where that membership is decided, so
 * the rule is testable without a store:
 *
 * - a **value** is written when it differs from the original — the same
 *   rule the staged edits have always used, including "reverting the only
 *   changed column unstages the row";
 * - **`null`** is a value like any other: it is written as SQL `NULL` and
 *   stays distinguishable from an empty string;
 * - **`undefined`** means *do not write this column*. The column leaves the
 *   changed list and the cell goes back to the value it had before the
 *   change — which is what "revert this cell" does.
 */
import { valuesEqual, type PendingRowChange, type RowIdentity } from '../../lib/tableChanges';
import type { Value } from '../../types';
import { computeStageResult } from './stageEdit';

export interface StageCellValueInput {
  /** The change staged for that row today, if any. */
  readonly existing: PendingRowChange | undefined;
  /** The column the user edited. */
  readonly column: string;
  /**
   * The write intent.
   *
   * `undefined` = do not write this column; `null` = write SQL `NULL`;
   * anything else = write that value.
   */
  readonly value: Value | null | undefined;
  /** The value before the edit. */
  readonly originalValue: Value | null;
  /** The identity the change is filed under. */
  readonly rowIdentity: RowIdentity;
  /** The row index — a UI hint only, never part of the identity. */
  readonly rowIndex: number;
}

export interface StageCellValueResult {
  /**
   * The next version of the row's staged change, or `null` when the row has
   * nothing staged left — the caller then removes the key from
   * `pendingChanges`.
   */
  readonly change: PendingRowChange | null;
  /** The columns that really differ from the original value. */
  readonly changedColumns: readonly string[];
}

/**
 * The next version of a row's staged change after one cell was written.
 *
 * The value case is the rule the staged edits already had; the `undefined`
 * case is new and is the one that makes "revert this cell" possible.
 */
export function computeStageCellValueResult(input: StageCellValueInput): StageCellValueResult {
  if (input.value !== undefined) {
    return computeStageResult({
      existing: input.existing,
      column: input.column,
      originalValue: input.originalValue,
      currentValue: input.value,
      rowIdentity: input.rowIdentity,
      rowIndex: input.rowIndex,
    });
  }

  const existing = input.existing;
  if (!existing) return { change: null, changedColumns: [] };

  const originalValues = { ...existing.originalValues };
  const currentValues = { ...existing.currentValues };
  delete originalValues[input.column];
  delete currentValues[input.column];
  const changedColumns = existing.changedColumns.filter((column) => column !== input.column);

  // A row that is marked for deletion keeps its entry: the delete mark is a
  // fact of its own.
  if (changedColumns.length === 0 && !existing.deleteMarked) {
    return { change: null, changedColumns };
  }

  return {
    change: {
      ...existing,
      originalValues,
      currentValues,
      changedColumns,
    },
    changedColumns,
  };
}

/**
 * The value a cell goes back to when its change is undone.
 *
 * `null` when the column was never staged — the cell keeps whatever it
 * holds.
 */
export function restoredCellValue(
  existing: PendingRowChange | undefined,
  column: string,
): Value | null {
  if (!existing) return null;
  if (!existing.changedColumns.includes(column)) return null;
  const original = existing.originalValues[column];
  return original === undefined ? null : original;
}

/**
 * Whether the staged change of a column still differs from the original.
 *
 * Used by the "revert this cell" entry, which may only be offered when
 * there is something to undo.
 */
export function hasPendingColumnChange(
  existing: PendingRowChange | undefined,
  column: string,
): boolean {
  if (!existing) return false;
  if (!existing.changedColumns.includes(column)) return false;
  return !valuesEqual(existing.originalValues[column], existing.currentValues[column]);
}
