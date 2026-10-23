/**
 * The dirty-state index of one table panel — the **single derived view** of
 * "what has been staged but not committed yet" (DB-06).
 *
 * It is a pure function module on purpose:
 *
 * - it adds **no state** and **no source of truth**. `pendingChanges` stays the
 *   only truth; this module only reads it;
 * - it never reads the selection state. `data-dt-selected` (selection) and
 *   `data-dt-dirty` (staged change) are orthogonal dimensions: selecting a cell
 *   must never change a highlight, and clearing a highlight must never change
 *   a selection (00-contracts §4);
 * - it is therefore testable without React and without a store.
 *
 * `buildDirtyIndex` is the **whole-page batch version** of the existing
 * `findPendingForRow`: same lookup priority (anchor first, recomputed identity
 * second), same uniqueness guard (an ambiguous row is never marked), same
 * notion of "really changed" (`valuesEqual`). The two must agree row by row —
 * that equivalence is pinned by a test, not by discipline.
 */
import {
  buildRowIdentity,
  duplicateRowIdentityKeys,
  rowIdentityKey,
  valuesEqual,
  type PendingRowChange,
} from '../../lib/tableChanges';
import type { Value } from '../../types';
import type { TableState } from './types';

/**
 * The dirty state of one row, by row identity.
 *
 * - `dirty`    : the row has uncommitted column changes;
 * - `deleted`  : the row is marked for deletion (its cells carry no dirty
 *                mark — the same fact is never expressed twice);
 * - `inserted` : **not produced by this chapter**. DB-04 (INSERT) produces it on
 *                a draft row; adding it later is one member here plus one
 *                render branch, the selectors stay as they are.
 */
export type DirtyRowState = 'dirty' | 'deleted' | 'inserted';

/** A read-only snapshot of one uncommitted cell change. */
export interface DirtyCell {
  /** Column name — not the column index: the index loses its meaning when columns are hidden. */
  readonly columnName: string;
  /** The value before the change (`null` means SQL NULL). */
  readonly original: Value | null;
  /** The value the change would write (`null` means SQL NULL). */
  readonly next: Value | null;
}

/** The dirty state of one row, resolved by row identity. */
export interface DirtyRowEntry {
  /** Stable row identity key — the same key space as `pendingChanges`. */
  readonly identityKey: string;
  readonly state: DirtyRowState;
  /** The columns that really changed. Empty for a `deleted` row. */
  readonly changedColumns: readonly string[];
  /** One snapshot per changed column, in the same order. */
  readonly cells: readonly DirtyCell[];
}

export interface DirtyCounts {
  /** Rows with column changes that are not marked for deletion. */
  readonly updateRows: number;
  /** Rows marked for deletion. */
  readonly deleteRows: number;
  /** `updateRows + deleteRows`. */
  readonly totalRows: number;
  /** All really-changed cells of all non-deleted rows, across every page. */
  readonly totalCells: number;
  /** The changed cells that land on the current page. */
  readonly cellsOnPage: number;
  /** `totalCells - cellsOnPage`: the changed cells on other pages. */
  readonly cellsOffPage: number;
}

export interface DirtyIndex {
  /** Current page row index → that row's dirty state. */
  readonly byRowIndex: ReadonlyMap<number, DirtyRowEntry>;
  /** Staged changes that are not on the current page (they are never dropped). */
  readonly offPageKeys: readonly string[];
  readonly counts: DirtyCounts;
}

/**
 * The composite key of a cell-level dirty mark: row identity key + NUL + column name.
 *
 * Why the separator has to be NUL:
 * - `rowIdentityKey()` produces a JSON-shaped string (`["id":1]`) that already
 *   contains `"` `:` `[` `]` `,`;
 * - a column name may contain those characters too (e.g. `user:name`);
 * - so neither `:` nor `|` is injective — two different (row, column) pairs
 *   would produce the same key.
 *
 * `stableSerialize` / `JSON.stringify` escape control characters into the
 * two-character sequence `\u0000`, so a bare NUL never occurs in an identity
 * key, and SQL identifiers cannot contain one either. NUL is therefore
 * impossible on both sides.
 *
 * **This is a contract symbol, not a dead export.** It has no production
 * call site yet — the dirty marks are resolved per row, not per cell — but
 * 06 §3.1 freezes it as *the* key of a cell-level dirty mark: it is what a
 * consumer queries when it needs to address one changed cell (by row identity
 * + column name) instead of scanning the row's changed columns. Do **not**
 * delete it as an unused export, and do not invent a call site for it either:
 * the key space it defines (`rowIdentityKey` + NUL + column name) has to stay
 * the one every later chapter uses, otherwise two chapters would build
 * different keys for the same cell.
 */
export function dirtyCellKey(identityKey: string, columnName: string): string {
  return `${identityKey}\u0000${columnName}`;
}

/**
 * The empty index. Its identity is stable so a consumer can use it as a
 * default without re-creating a map on every render.
 */
export const EMPTY_DIRTY_INDEX: DirtyIndex = Object.freeze({
  byRowIndex: new Map<number, DirtyRowEntry>(),
  offPageKeys: Object.freeze([]) as readonly string[],
  counts: Object.freeze({
    updateRows: 0,
    deleteRows: 0,
    totalRows: 0,
    totalCells: 0,
    cellsOnPage: 0,
    cellsOffPage: 0,
  }),
});

/**
 * The columns of a staged change that really differ from the original value.
 *
 * `stageCellChange` already drops a column when the user reverts it, so this
 * filter is a **second line of defence**: if a future write path forgets that
 * rule, the renderer must still not paint a cell as changed.
 */
function reallyChangedColumns(change: PendingRowChange): string[] {
  return change.changedColumns.filter(
    (column) => !valuesEqual(change.originalValues[column], change.currentValues[column]),
  );
}

/**
 * The counts that depend on the staged changes alone.
 *
 * It deliberately knows nothing about the current page: a panel whose rows
 * were dropped (a write from elsewhere) or that has not loaded yet still has to
 * report the right number of staged changes. `cellsOnPage` is therefore `0` and
 * `cellsOffPage` is the whole `totalCells` — "without a page, every cell is off
 * the page". `buildDirtyIndex` fills in the real split.
 */
export function selectDirtyCounts(
  pendingChanges: ReadonlyMap<string, PendingRowChange>,
): DirtyCounts {
  let updateRows = 0;
  let deleteRows = 0;
  let totalCells = 0;
  for (const change of pendingChanges.values()) {
    if (change.deleteMarked) {
      deleteRows += 1;
      continue;
    }
    const columns = reallyChangedColumns(change);
    if (columns.length === 0) continue;
    updateRows += 1;
    totalCells += columns.length;
  }
  return {
    updateRows,
    deleteRows,
    totalRows: updateRows + deleteRows,
    totalCells,
    cellsOnPage: 0,
    cellsOffPage: totalCells,
  };
}

/** One row's dirty state, or `undefined` when the row is clean. */
export function selectDirtyRow(
  index: DirtyIndex | null | undefined,
  rowIndex: number,
): DirtyRowEntry | undefined {
  return index?.byRowIndex.get(rowIndex);
}

function sameCells(left: readonly DirtyCell[], right: readonly DirtyCell[]): boolean {
  if (left.length !== right.length) return false;
  return left.every((cell, index) => {
    const other = right[index];
    return (
      cell.columnName === other.columnName &&
      valuesEqual(cell.original, other.original) &&
      valuesEqual(cell.next, other.next)
    );
  });
}

/**
 * The entry of one row, reusing the previous object when nothing about it
 * changed.
 *
 * Structural sharing is what keeps `VirtualRow`'s `memo` alive: editing one row
 * must not change the props of every other row.
 */
function rowEntry(
  identityKey: string,
  change: PendingRowChange,
  previousByIdentity: Map<string, DirtyRowEntry>,
): DirtyRowEntry {
  const state: DirtyRowState = change.deleteMarked ? 'deleted' : 'dirty';
  const changedColumns = change.deleteMarked ? [] : reallyChangedColumns(change);
  const cells: DirtyCell[] = changedColumns.map((columnName) => ({
    columnName,
    original: change.originalValues[columnName] ?? null,
    next: change.currentValues[columnName] ?? null,
  }));

  const previous = previousByIdentity.get(identityKey);
  if (
    previous &&
    previous.state === state &&
    previous.changedColumns.length === changedColumns.length &&
    previous.changedColumns.every((column, index) => column === changedColumns[index]) &&
    sameCells(previous.cells, cells)
  ) {
    return previous;
  }

  return { identityKey, state, changedColumns, cells };
}

/**
 * Build the dirty index of a panel from its table state.
 *
 * @param ts        the panel slice (only `rows` / `columns` / `pendingChanges` /
 *                  `rowIdentityAnchors` are read — never the selection);
 * @param previous  the previous round's index, for structural sharing.
 */
export function buildDirtyIndex(ts: TableState, previous?: DirtyIndex | null): DirtyIndex {
  const pendingChanges = ts.pendingChanges;
  if (pendingChanges.size === 0) return EMPTY_DIRTY_INDEX;

  const base = selectDirtyCounts(pendingChanges);
  const pkColumns = ts.columns.filter((column) => column.isPrimaryKey);

  const byRowIndex = new Map<number, DirtyRowEntry>();
  const offPageKeys: string[] = [];
  let cellsOnPage = 0;

  if (pkColumns.length > 0) {
    /**
     * Rows with an ambiguous identity are never marked: painting a change on
     * one of two rows with the same key would be a guess. This is the same
     * guard `findPendingForRow` applies through `rowIdentityIsUnique`, and it
     * is why the duplicate keys are computed **once** for the whole page
     * instead of once per row.
     */
    const ambiguousKeys = new Set(duplicateRowIdentityKeys(ts.rows, pkColumns));
    const matchedKeys = new Set<string>();
    const previousByIdentity = previousEntries(previous);

    for (let rowIndex = 0; rowIndex < ts.rows.length; rowIndex += 1) {
      const row = ts.rows[rowIndex];
      const identity = buildRowIdentity(row, pkColumns);
      if (!identity) continue;
      const identityKey = rowIdentityKey(identity);
      if (ambiguousKeys.has(identityKey)) continue;

      /**
       * The anchor wins over the recomputed identity — the same priority
       * `findPendingForRow` uses. The anchor keeps pointing at the **original**
       * identity of a row whose primary key was edited, so the change stays on
       * the row the user edited.
       */
      const anchoredKey = ts.rowIdentityAnchors.get(rowIndex);
      const anchored =
        anchoredKey !== undefined ? pendingChanges.get(anchoredKey) : undefined;
      const key = anchored !== undefined && anchoredKey !== undefined ? anchoredKey : identityKey;
      const change = anchored ?? pendingChanges.get(identityKey);
      if (!change) continue;

      matchedKeys.add(key);
      const entry = rowEntry(key, change, previousByIdentity);
      byRowIndex.set(rowIndex, entry);
      cellsOnPage += entry.changedColumns.length;
    }

    for (const key of pendingChanges.keys()) {
      if (!matchedKeys.has(key)) offPageKeys.push(key);
    }
  } else {
    // No primary key: nothing can be staged against this table at all, so
    // every staged change is off-page by definition.
    for (const key of pendingChanges.keys()) offPageKeys.push(key);
  }

  return {
    byRowIndex,
    offPageKeys,
    counts: {
      ...base,
      cellsOnPage,
      cellsOffPage: Math.max(0, base.totalCells - cellsOnPage),
    },
  };
}

/**
 * The previous round's entries, indexed by identity key.
 *
 * Built once per round: looking the previous entry up per row would make the
 * index quadratic in the number of dirty rows.
 */
function previousEntries(previous: DirtyIndex | null | undefined): Map<string, DirtyRowEntry> {
  const byIdentity = new Map<string, DirtyRowEntry>();
  if (!previous) return byIdentity;
  for (const entry of previous.byRowIndex.values()) {
    byIdentity.set(entry.identityKey, entry);
  }
  return byIdentity;
}
