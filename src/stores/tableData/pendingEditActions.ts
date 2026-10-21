/**
 * The staged-edit actions of one table panel, moved verbatim out of
 * `tableDataStore.ts` (DB-06 track, GC budget).
 *
 * These five actions — `stageCellChange`, `applyColumnToRows`, `stageRowDelete`,
 * `deleteRows` and `rollbackPendingChanges` — are the ones the dirty-cell
 * chapter (DB-06) rewrites: it removes the `editBuffer` write points from them,
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
import { buildRowIdentity, rowIdentityKey, valuesEqual } from '../../lib/tableChanges';
import { toCellValue } from './connectionState';
import {
  AMBIGUOUS_ROW_IDENTITY_ERROR,
  findPendingForRow,
  hasPendingIdentityCollision,
  rebuildEditBuffer,
  rowIdentityIsUnique,
} from './pendingChanges';
import type { TableState } from './types';

export interface PendingEditStoreSlice {
  /** Stage one cell edit; the only entry point that produces a staged change. */
  stageCellChange: (panelId: string, row: number, col: string, value: unknown) => void;
  /** Stage the same value in one column of many rows (one call per cell). */
  applyColumnToRows: (panelId: string, col: string, value: unknown, rows: number[]) => void;
  /** Mark rows for deletion. */
  stageRowDelete: (panelId: string, rowIndices: number | number[]) => void;
  /** Delete rows: the selection semantics first, then the delete mark. */
  deleteRows: (panelId: string, rowIndices: number[]) => void;
  /** Drop every staged change and re-read the page. */
  rollbackPendingChanges: (panelId: string) => void;
}

export function createPendingEditSlice(
  patch: (panelId: string, updater: (ts: TableState) => Partial<TableState>) => void,
  read: (panelId: string) => TableState | undefined,
  reload: (panelId: string) => void,
): PendingEditStoreSlice {
  const stageCellChange = (
    panelId: string,
    row: number,
    col: string,
    value: unknown,
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
    const currentValue = toCellValue(value);
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
    const originalValues = { ...(existing?.originalValues ?? {}) };
    const currentValues = { ...(existing?.currentValues ?? {}) };
    let changedColumns = [...(existing?.changedColumns ?? [])];

    if (valuesEqual(originalValue, currentValue)) {
      delete originalValues[col];
      delete currentValues[col];
      changedColumns = changedColumns.filter((column) => column !== col);
    } else {
      originalValues[col] = originalValue;
      currentValues[col] = currentValue;
      if (!changedColumns.includes(col)) changedColumns.push(col);
    }

    const pendingChanges = new Map(ts.pendingChanges);
    if (changedColumns.length === 0 && !existing?.deleteMarked) {
      pendingChanges.delete(key);
    } else {
      pendingChanges.set(key, {
        rowIndex: row,
        rowIdentity: { ...identity },
        originalValues,
        currentValues,
        changedColumns,
        deleteMarked: existing?.deleteMarked ?? false,
      });
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
      editBuffer: rebuildEditBuffer({ ...ts, rows: nextRows, pendingChanges }),
      editingCell: null,
      error: null,
    }));
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
      editBuffer: rebuildEditBuffer({ ...ts, pendingChanges }),
      editingCell: null,
      error: null,
    }));
  };

  return {
    stageCellChange,

    applyColumnToRows: (panelId, col, value, rows) => {
      for (const row of rows) stageCellChange(panelId, row, col, value);
    },

    stageRowDelete,

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
        editBuffer: new Map(),
        editingCell: null,
        highlightedNewRowKey: null,
        error: null,
      }));
      reload(panelId);
    },
  };
}
