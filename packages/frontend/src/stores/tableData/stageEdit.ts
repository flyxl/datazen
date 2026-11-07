/**
 * The pure computations behind the staged-edit actions (DB-06 §5 Step 3).
 *
 * They live here for one reason: "the user reverted the cell to its original
 * value, so the row is clean again and its key must leave `pendingChanges`" is
 * the rule the dirty marks depend on, and until now it was buried inside a
 * store action where no test could reach it.
 *
 * Both functions are total and side-effect free: they take the staged change
 * of a row and return its **next version**, or `null` when the row has nothing
 * staged left. The store owns the write.
 */
import { valuesEqual, type PendingRowChange, type RowIdentity } from '../../lib/tableChanges';
import type { Value } from '../../types';

export interface StageEditInput {
  /** The change staged for that row today, if any. */
  readonly existing: PendingRowChange | undefined;
  /** The column the user edited. */
  readonly column: string;
  /** The value before the edit (the snapshot taken when the change was staged). */
  readonly originalValue: Value | null;
  /** The value the user entered. */
  readonly currentValue: Value | null;
  /** The identity the change is filed under. */
  readonly rowIdentity: RowIdentity;
  /** The row index — a UI hint only, never part of the identity. */
  readonly rowIndex: number;
}

export interface StageEditResult {
  /**
   * The next version of the change, or `null` when the row is clean again
   * (the user reverted the only changed column and the row is not marked for
   * deletion) — the caller then removes the key from `pendingChanges`.
   */
  readonly change: PendingRowChange | null;
  /** The columns that really differ from the original value after the edit. */
  readonly changedColumns: readonly string[];
}

/**
 * The next version of a row's staged change after one cell was edited.
 *
 * The rule it encodes:
 *
 * - editing a cell to a value that differs from the original stages it;
 * - editing it **back** to the original value unstages it — and if that was the
 *   only changed column and the row is not marked for deletion, the row's key
 *   leaves `pendingChanges` entirely (`change === null`);
 * - a row that is marked for deletion keeps its entry even when every column
 *   was reverted: the delete mark is a fact of its own.
 */
export function computeStageResult(input: StageEditInput): StageEditResult {
  const originalValues = { ...(input.existing?.originalValues ?? {}) };
  const currentValues = { ...(input.existing?.currentValues ?? {}) };
  let changedColumns = [...(input.existing?.changedColumns ?? [])];

  if (valuesEqual(input.originalValue, input.currentValue)) {
    delete originalValues[input.column];
    delete currentValues[input.column];
    changedColumns = changedColumns.filter((column) => column !== input.column);
  } else {
    originalValues[input.column] = input.originalValue;
    currentValues[input.column] = input.currentValue;
    if (!changedColumns.includes(input.column)) changedColumns.push(input.column);
  }

  if (changedColumns.length === 0 && !input.existing?.deleteMarked) {
    return { change: null, changedColumns };
  }

  return {
    change: {
      rowIndex: input.rowIndex,
      rowIdentity: { ...input.rowIdentity },
      originalValues,
      currentValues,
      changedColumns,
      deleteMarked: input.existing?.deleteMarked ?? false,
    },
    changedColumns,
  };
}

/**
 * The next version of a row's staged change after its delete mark was undone.
 *
 * A row that was only marked for deletion has nothing left staged → its key
 * leaves `pendingChanges` (`null`). A row that also had column changes keeps
 * them and only loses the delete mark.
 */
export function computeUnstageDeleteResult(change: PendingRowChange): PendingRowChange | null {
  if (change.changedColumns.length === 0) return null;
  return { ...change, deleteMarked: false };
}
