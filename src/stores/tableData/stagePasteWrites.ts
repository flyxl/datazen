/**
 * Batch staging for a paste (DB-03 §3.4) — pure function, no Zustand, no `t()`.
 *
 * The reason this is a module of its own is the merge semantics, not the loop: a paste has
 * to land in exactly the structures a single-cell edit lands in, so that "the same cell, in
 * the same state, written by either path" produces **the same** `rows` / `pendingChanges` /
 * `rowIdentityAnchors`. The single-cell path (`pendingEditActions.stageCellValue`) and this
 * batch path therefore share the same primitives — `findPendingForRow`,
 * `rowIdentityIsUnique`, `hasPendingIdentityCollision`, `computeStageCellValueResult` — and
 * the equivalence is pinned by a test rather than by discipline.
 *
 * What the caller gets back is a proposal: the store applies it in **one** `set()`. Errors
 * are returned as structured `problems` instead of being written into `ts.error`, because a
 * batch of 5 000 cells would otherwise have every failure overwritten by the next one.
 */
import type { ColumnSchema } from '../../types';
import type { GridErrorCode } from '../../lib/gridErrors';
import {
  buildRowIdentity,
  rowIdentityKey,
  type CellPasteWrite,
  type PendingRowChange,
} from '../../lib/tableChanges';
import { toCellValue } from './connectionState';
import { findPendingForRow, hasPendingIdentityCollision, rowIdentityIsUnique } from './pendingChanges';
import { computeStageCellValueResult } from './stageCellValue';
import type { TableState } from './types';

export interface PasteCellProblem {
  rowIndex: number;
  columnName: string;
  code: GridErrorCode;
  detail?: string;
}

export interface StagePasteWritesOutcome {
  rows: Record<string, unknown>[];
  pendingChanges: Map<string, PendingRowChange>;
  rowIdentityAnchors: Map<number, string>;
  /** Cells that really entered `pendingChanges` (a cell reverting to its original does not count). */
  staged: number;
  /** Cells that were skipped, with the reason. **The caller decides whether to surface them.** */
  problems: PasteCellProblem[];
}

/**
 * Write N cells in one pass.
 *
 * Same checks, same order, same branches as one `stageCellValue` call, repeated over the
 * block: a cell whose row identity is ambiguous is skipped **and the rest still lands** —
 * one bad cell must not throw away 4 999 good ones.
 */
export function stagePasteWritesInto(
  ts: TableState,
  writes: readonly CellPasteWrite[],
  primaryKeyColumns: readonly ColumnSchema[],
): StagePasteWritesOutcome {
  if (writes.length === 0) {
    // Reference-equal: an empty paste must not produce a new state object, otherwise it
    // would re-render a grid for nothing.
    return {
      rows: ts.rows,
      pendingChanges: ts.pendingChanges,
      rowIdentityAnchors: ts.rowIdentityAnchors,
      staged: 0,
      problems: [],
    };
  }

  const pkCols = [...primaryKeyColumns];
  if (pkCols.length === 0) {
    return {
      rows: ts.rows,
      pendingChanges: ts.pendingChanges,
      rowIdentityAnchors: ts.rowIdentityAnchors,
      staged: 0,
      problems: writes.map((write) => ({
        rowIndex: write.rowIndex,
        columnName: write.columnName,
        code: 'grid.cell.noRowIdentity' as GridErrorCode,
      })),
    };
  }

  const rows = [...ts.rows];
  const pendingChanges = new Map(ts.pendingChanges);
  const rowIdentityAnchors = new Map(ts.rowIdentityAnchors);
  // The working state: every iteration sees the writes of the previous ones, exactly as the
  // single-cell path would after its own `set()`.
  const working: TableState = {
    ...ts,
    rows,
    pendingChanges,
    rowIdentityAnchors,
  };

  let staged = 0;
  const problems: PasteCellProblem[] = [];

  for (const write of writes) {
    const { rowIndex, columnName, value } = write;
    if (!Number.isInteger(rowIndex) || rowIndex < 0 || rowIndex >= rows.length) {
      problems.push({
        rowIndex,
        columnName,
        code: 'grid.paste.outOfBounds',
      });
      continue;
    }
    const rowObject = rows[rowIndex];
    if (!rowObject) {
      problems.push({ rowIndex, columnName, code: 'grid.paste.outOfBounds' });
      continue;
    }

    // The row identity is resolved from the row's own primary-key values, never from the
    // row number (00-contracts §4.3).
    const rowIdentity = buildRowIdentity(rowObject, pkCols);
    if (!rowIdentity || !rowIdentityIsUnique(working, rowIndex, rowIdentity, pkCols)) {
      problems.push({
        rowIndex,
        columnName,
        code: 'grid.cell.ambiguousIdentity',
      });
      continue;
    }
    const match = findPendingForRow(working, rowIndex, rowObject, pkCols);
    const identity = match?.change.rowIdentity ?? rowIdentity;
    if (!identity) {
      problems.push({ rowIndex, columnName, code: 'grid.cell.ambiguousIdentity' });
      continue;
    }
    const existing = match?.change;
    const hasOriginalValue =
      existing !== undefined &&
      Object.prototype.hasOwnProperty.call(existing.originalValues, columnName);
    const originalValue = hasOriginalValue
      ? existing.originalValues[columnName]
      : toCellValue(rowObject[columnName]);
    const currentValue = value;
    const prospectiveRow = { ...rowObject, [columnName]: currentValue };
    const prospectiveIdentity = buildRowIdentity(prospectiveRow, pkCols);
    const key = match?.key ?? rowIdentityKey(identity);
    if (
      !prospectiveIdentity ||
      !rowIdentityIsUnique(
        {
          ...working,
          rows: rows.map((candidate, index) => (index === rowIndex ? prospectiveRow : candidate)),
        },
        rowIndex,
        prospectiveIdentity,
        pkCols,
      ) ||
      hasPendingIdentityCollision(working, key, prospectiveIdentity, pkCols)
    ) {
      // Paste-specific code: the user did not aim at the primary key, so "the row would no
      // longer be locatable" is told with the code the copy chapter owns (03 §3.3).
      problems.push({ rowIndex, columnName, code: 'grid.paste.identityChanged' });
      continue;
    }

    const next = computeStageCellValueResult({
      existing,
      column: columnName,
      value: currentValue,
      originalValue,
      rowIdentity: identity,
      rowIndex,
    });
    if (next.change === null) pendingChanges.delete(key);
    else pendingChanges.set(key, next.change);
    rows[rowIndex] = { ...rowObject, [columnName]: currentValue };
    rowIdentityAnchors.set(rowIndex, key);
    if (next.change !== null && next.change.changedColumns.includes(columnName)) staged += 1;
  }

  return { rows, pendingChanges, rowIdentityAnchors, staged, problems };
}
