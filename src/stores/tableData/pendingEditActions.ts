/**
 * The staged-edit actions of one table panel, moved verbatim out of
 * `tableDataStore.ts` (DB-06 track, GC budget).
 *
 * These five actions — `stageCellChange`, `applyColumnToRows`, `stageRowDelete`,
 * `deleteRows` and `rollbackPendingChanges` — are the ones the dirty-cell
 * chapter (DB-06) rewrites: it drops the second dirty-state cache from them,
 * adds the "undo the delete mark" action next to them, and moves the
 * "next version of a `PendingRowChange`" computation into a pure module.
 *
 * With the store file at 758 lines and 800 as the hard ceiling, those changes
 * did not fit. The bodies below are **moved, not rewritten**: same order, same
 * reads, same writes, same error branches.
 *
 * The store keeps only the wiring (the same pattern as the selection slice and
 * the draft slice): `patch` writes one panel's slice, `read` reads the current
 * slice (fresh on every call, so the loop in `applyColumnToRows` still sees the
 * previous write), and `reload` re-reads the page.
 */
import { t } from '../../locales/t';
import { buildRowIdentity, rowIdentityKey } from '../../lib/tableChanges';
import type { Value } from '../../types';
import { toCellValue } from './connectionState';
import {
  AMBIGUOUS_ROW_IDENTITY_ERROR,
  findPendingForRow,
  hasPendingIdentityCollision,
  rowIdentityIsUnique,
} from './pendingChanges';
import { computeUnstageDeleteResult } from './stageEdit';
import { computeStageCellValueResult, restoredCellValue } from './stageCellValue';
import { stagePasteWritesInto, type PasteCellProblem } from './stagePasteWrites';
import type { CellPasteWrite } from '../../lib/tableChanges';
import type { TableState } from './types';

/** What one paste did, for the caller's summary line. */
export interface StagePasteWritesResult {
  /** Cells that really entered `pendingChanges`. */
  staged: number;
  /** Cells that were skipped (the reason is in `problems`). */
  skipped: number;
  problems: PasteCellProblem[];
}

export interface PendingEditStoreSlice {
  /** Stage one cell edit; the only entry point that produces a staged change. */
  stageCellChange: (panelId: string, row: number, col: string, value: unknown) => void;
  /**
   * Stage one cell edit with an explicit write intent (DB-05).
   *
   * `undefined` = do not write this column, `null` = write SQL `NULL`,
   * anything else = write that value. `stageCellChange` is the thin shell
   * over it, so the old entry point keeps its behaviour byte for byte.
   */
  stageCellValue: (
    panelId: string,
    row: number,
    col: string,
    value: Value | null | undefined,
  ) => void;
  /** Stage the same value in one column of many rows (one call per cell). */
  applyColumnToRows: (panelId: string, col: string, value: unknown, rows: number[]) => void;
  /**
   * Stage many cells in **one** state write (DB-03).
   *
   * Each write carries its own three-state intent through the membership of the sequence
   * (`CellPasteWrite`); the merge with existing staged changes happens in
   * `stagePasteWritesInto` and is the same merge a single-cell edit performs.
   *
   * Returns what happened instead of writing `ts.error`: a batch of thousands of cells would
   * otherwise have every failure promptly overwritten by the next one.
   */
  stagePasteWrites: (panelId: string, writes: readonly CellPasteWrite[]) => StagePasteWritesResult;
  /** Mark rows for deletion. */
  stageRowDelete: (panelId: string, rowIndices: number | number[]) => void;
  /** Delete rows: the selection semantics first, then the delete mark. */
  deleteRows: (panelId: string, rowIndices: number[]) => void;
  /** Undo the delete mark of one row. */
  unstageRowDelete: (panelId: string, rowIndex: number) => void;
  /** Drop every staged change and re-read the page. */
  rollbackPendingChanges: (panelId: string) => void;
}

export function createPendingEditSlice(
  patch: (panelId: string, updater: (ts: TableState) => Partial<TableState>) => void,
  read: (panelId: string) => TableState | undefined,
  reload: (panelId: string) => void,
): PendingEditStoreSlice {
  /**
   * Undo the staged change of one column ("revert this cell").
   *
   * The column leaves the change list and the cell goes back to the value it
   * had before the change — the third state of a write (00-contracts §3).
   */
  const revertCellValue = (panelId: string, row: number, col: string): void => {
    const ts = read(panelId);
    if (!ts) return;
    const rowObj = ts.rows[row];
    if (!rowObj) return;
    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    if (pkCols.length === 0) {
      patch(panelId, () => ({ error: t('tableData.noPrimaryKey') }));
      return;
    }
    const match = findPendingForRow(ts, row, rowObj, pkCols);
    if (!match) return;
    if (!match.change.changedColumns.includes(col)) return;

    const staged = computeStageCellValueResult({
      existing: match.change,
      column: col,
      value: undefined,
      originalValue: match.change.originalValues[col] ?? null,
      rowIdentity: match.change.rowIdentity,
      rowIndex: row,
    });

    const pendingChanges = new Map(ts.pendingChanges);
    if (staged.change === null) {
      pendingChanges.delete(match.key);
    } else {
      pendingChanges.set(match.key, staged.change);
    }

    const nextRows = [...ts.rows];
    nextRows[row] = { ...rowObj, [col]: restoredCellValue(match.change, col) };
    const rowIdentityAnchors = new Map(ts.rowIdentityAnchors);
    rowIdentityAnchors.set(row, match.key);

    patch(panelId, () => ({
      rows: nextRows,
      pendingChanges,
      rowIdentityAnchors,
      previewPlan: null,
      pendingStatus: 'idle',
      editingCell: null,
      error: null,
    }));
  };

  const stageCellValue = (
    panelId: string,
    row: number,
    col: string,
    value: Value | null | undefined,
  ): void => {
    const ts = read(panelId);
    if (!ts) return;
    const rowObj = ts.rows[row];
    if (!rowObj) return;
    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    if (pkCols.length === 0) {
      patch(panelId, () => ({ error: t('tableData.noPrimaryKey') }));
      return;
    }
    // "Do not write this column" is the undo of a staged change, and it has
    // its own path: there is no value to check against the row identity.
    if (value === undefined) {
      revertCellValue(panelId, row, col);
      return;
    }

    const rowIdentity = buildRowIdentity(rowObj, pkCols);
    if (!rowIdentity || !rowIdentityIsUnique(ts, row, rowIdentity, pkCols)) {
      patch(panelId, () => ({ error: AMBIGUOUS_ROW_IDENTITY_ERROR }));
      return;
    }
    const match = findPendingForRow(ts, row, rowObj, pkCols);
    const identity = match?.change.rowIdentity ?? rowIdentity;
    if (!identity) {
      patch(panelId, () => ({ error: AMBIGUOUS_ROW_IDENTITY_ERROR }));
      return;
    }
    const existing = match?.change;
    const hasOriginalValue =
      existing !== undefined && Object.prototype.hasOwnProperty.call(existing.originalValues, col);
    const originalValue = hasOriginalValue
      ? existing.originalValues[col]
      : toCellValue(rowObj[col]);
    const currentValue = value;
    const prospectiveRow = { ...rowObj, [col]: currentValue };
    const prospectiveIdentity = buildRowIdentity(prospectiveRow, pkCols);
    const key = match?.key ?? rowIdentityKey(identity);
    if (
      !prospectiveIdentity ||
      !rowIdentityIsUnique(
        {
          ...ts,
          rows: ts.rows.map((candidate, index) => (index === row ? prospectiveRow : candidate)),
        },
        row,
        prospectiveIdentity,
        pkCols,
      ) ||
      hasPendingIdentityCollision(ts, key, prospectiveIdentity, pkCols)
    ) {
      patch(panelId, () => ({ error: AMBIGUOUS_ROW_IDENTITY_ERROR }));
      return;
    }
    // The "next version of the row's staged change" is a pure computation
    // (DB-06 §5 Step 3): it lives in `tableData/stageEdit` so the rule
    // "reverting the only changed column unstages the row" is testable on its
    // own and shared with the delete-mark undo. The three-state variant
    // lives in `tableData/stageCellValue` (DB-05).
    const staged = computeStageCellValueResult({
      existing,
      column: col,
      value: currentValue,
      originalValue,
      rowIdentity: identity,
      rowIndex: row,
    });

    const pendingChanges = new Map(ts.pendingChanges);
    if (staged.change === null) {
      pendingChanges.delete(key);
    } else {
      pendingChanges.set(key, staged.change);
    }

    const nextRows = [...ts.rows];
    nextRows[row] = { ...rowObj, [col]: currentValue };
    const rowIdentityAnchors = new Map(ts.rowIdentityAnchors);
    rowIdentityAnchors.set(row, key);

    patch(panelId, () => ({
      rows: nextRows,
      pendingChanges,
      rowIdentityAnchors,
      previewPlan: null,
      pendingStatus: 'idle',
      editingCell: null,
      error: null,
    }));
  };

  const stageCellChange = (
    panelId: string,
    row: number,
    col: string,
    value: unknown,
  ): void => {
    // The old entry point keeps its behaviour byte for byte: an absent value
    // was a `NULL`, and it still is.
    stageCellValue(panelId, row, col, toCellValue(value));
  };

  const stageRowDelete = (panelId: string, rowIndices: number | number[]): void => {
    const ts = read(panelId);
    if (!ts) return;
    const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
    if (pkCols.length === 0) {
      patch(panelId, () => ({ error: t('tableData.noPrimaryKey') }));
      return;
    }

    const indices = [
      ...new Set(
        (Array.isArray(rowIndices) ? rowIndices : [rowIndices]).filter(
          (index) => Number.isInteger(index) && index >= 0,
        ),
      ),
    ].sort((left, right) => left - right);
    if (indices.length === 0) return;

    const pendingChanges = new Map(ts.pendingChanges);
    const rowIdentityAnchors = new Map(ts.rowIdentityAnchors);
    for (const rowIndex of indices) {
      const row = ts.rows[rowIndex];
      if (!row) continue;
      const rowIdentity = buildRowIdentity(row, pkCols);
      if (!rowIdentity || !rowIdentityIsUnique(ts, rowIndex, rowIdentity, pkCols)) {
        patch(panelId, () => ({ error: AMBIGUOUS_ROW_IDENTITY_ERROR }));
        return;
      }
      const match = findPendingForRow(ts, rowIndex, row, pkCols);
      const identity = match?.change.rowIdentity ?? rowIdentity;
      const key = match?.key ?? rowIdentityKey(identity);
      if (hasPendingIdentityCollision(ts, key, rowIdentity, pkCols)) {
        patch(panelId, () => ({ error: AMBIGUOUS_ROW_IDENTITY_ERROR }));
        return;
      }
      const existing = match?.change;
      pendingChanges.set(key, {
        rowIndex,
        rowIdentity: { ...identity },
        originalValues: { ...(existing?.originalValues ?? {}) },
        currentValues: { ...(existing?.currentValues ?? {}) },
        changedColumns: [...(existing?.changedColumns ?? [])],
        deleteMarked: true,
      });
      rowIdentityAnchors.set(rowIndex, key);
    }

    patch(panelId, () => ({
      pendingChanges,
      rowIdentityAnchors,
      previewPlan: null,
      pendingStatus: 'idle',
      editingCell: null,
      error: null,
    }));
  };

  return {
    stageCellChange,
    stageCellValue,

    stagePasteWrites: (panelId, writes) => {
      const ts = read(panelId);
      if (!ts || writes.length === 0) return { staged: 0, skipped: 0, problems: [] };
      const pkCols = ts.columns.filter((column) => column.isPrimaryKey);
      const outcome = stagePasteWritesInto(ts, writes, pkCols);
      // A paste where every cell was skipped leaves the state untouched: writing it anyway
      // would re-render the whole grid and clear the preview for nothing.
      const landed = outcome.rows !== ts.rows || outcome.pendingChanges !== ts.pendingChanges;
      if (landed) {
        patch(panelId, () => ({
          rows: outcome.rows,
          pendingChanges: outcome.pendingChanges,
          rowIdentityAnchors: outcome.rowIdentityAnchors,
          // Any staged change invalidates an existing preview (same rule as `stageCellValue`).
          previewPlan: null,
          pendingStatus: 'idle',
          error: null,
        }));
      }
      return {
        staged: outcome.staged,
        skipped: outcome.problems.length,
        problems: outcome.problems,
      };
    },

    applyColumnToRows: (panelId, col, value, rows) => {
      for (const row of rows) stageCellChange(panelId, row, col, value);
    },

    stageRowDelete,

    unstageRowDelete: (panelId, rowIndex) => {
      const ts = read(panelId);
      if (!ts) return;
      const pkCols = ts.columns.filter((c) => c.isPrimaryKey);
      if (pkCols.length === 0) {
        patch(panelId, () => ({ error: t('tableData.noPrimaryKey') }));
        return;
      }
      const row = ts.rows[rowIndex];
      if (!row) return;
      // The row identity is resolved by the existing lookup, never by the row
      // number (00-contracts §4.3): the row index is only a hint.
      const match = findPendingForRow(ts, rowIndex, row, pkCols);
      if (!match) return;

      const next = computeUnstageDeleteResult(match.change);
      const pendingChanges = new Map(ts.pendingChanges);
      if (next === null) pendingChanges.delete(match.key);
      else pendingChanges.set(match.key, next);

      patch(panelId, () => ({
        pendingChanges,
        previewPlan: null,
        pendingStatus: 'idle',
        editingCell: null,
        error: null,
      }));
    },

    deleteRows: (panelId, rowIndices) => {
      const ts = read(panelId);
      if (!ts) return;
      const unique = [...new Set(rowIndices.filter((i) => Number.isInteger(i) && i >= 0))];
      if (unique.length === 0) return;
      // Deleting does not change the selection semantics (00-contracts §4, GS2-D2): the row
      // set is owned by `mode`, so a `cell` selection must stay `cell` + empty rows while the
      // delete scope lives in the derived selector, and row identity is carried by
      // `rowIdentityAnchors` inside `stageRowDelete`. Writing `selectedRows` here would park
      // two sources of truth on top of each other ("region highlighted" + "rows selected").
      stageRowDelete(panelId, unique);
    },

    rollbackPendingChanges: (panelId) => {
      const ts = read(panelId);
      if (!ts) return;
      patch(panelId, () => ({
        pendingChanges: new Map(),
        draftInserts: new Map(),
        draftInsertOrder: [],
        previewPlan: null,
        pendingStatus: 'idle',
        editingCell: null,
        highlightedNewRowKey: null,
        error: null,
      }));
      reload(panelId);
    },
  };
}
