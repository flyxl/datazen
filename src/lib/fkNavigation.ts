/**
 * Pure foreign-key navigation logic (DB-07).
 *
 * Everything in this file is a plain function over plain data: no React, no
 * store, no IPC. The grid asks three questions — "which columns are part of a
 * foreign key?", "may this cell be followed?", "which equality conditions
 * describe the target?" — and the answers must be testable without a DOM.
 *
 * Two rules run through the whole module:
 *
 * - **A composite key is one group.** The i-th local column pairs with the
 *   i-th referenced column. A group is navigable only when *every* member has
 *   a value; one NULL member makes the whole group unlocatable, because
 *   `WHERE a = 1` alone would silently point at the wrong parent row.
 * - **A NULL never degrades into `IS NULL`.** A missing value means "there is
 *   nothing to follow", not "follow the rows where the key is null".
 */

import type { FilterCondition, ForeignKeyInfo, Value } from '../types';

/** Maximum depth of the navigation stack (the oldest entry is dropped beyond it). */
export const FK_NAV_MAX_DEPTH = 32;

/** The viewport snapshot taken when the grid is left. */
export interface GridViewport {
  scrollTop: number;
  /** Anchor row inside the page; `null` means there is no anchor. */
  anchorRow: number | null;
  /** Anchor column name; `null` means there is no anchor. */
  anchorColumn: string | null;
}

/**
 * One foreign-key group as the grid sees it.
 *
 * Every member column of the group maps to the same entry, so a cell can ask
 * "which group am I part of?" with one lookup.
 */
export interface FkGroupAffordance {
  /** The local columns of the group, in `ForeignKeyInfo.columns` order. */
  localColumns: string[];
  /** The referenced columns, aligned with `localColumns`. */
  targetColumns: string[];
  /** Display label of the target: `customers.id` or `sales.customers.(x, y)`. */
  targetLabel: string;
  /** The constraint name; disambiguates two constraints over the same column. */
  constraintName: string;
  /** The referenced table as the driver reported it (never split here). */
  referenceTable: string;
}

/**
 * The target identity a navigation key is built from.
 *
 * **Not the same type as the backend's `ForeignKeyTargetRef`**: this one is a
 * *field subset* of it — the four fields a navigation key needs — so it is a
 * separate concept with a separate name. The backend's `ForeignKeyTargetRef`
 * (the port's type in `packages/backend-client`) carries the same four fields
 * plus the referenced columns, their data types, the primary keys, the
 * uniqueness flag and the candidate limit.
 */
export interface FkNavTargetIdentity {
  readonly database: string;
  readonly schema: string | null;
  readonly table: string;
  readonly columns: readonly string[];
}

/** The breadcrumb data; the wording itself is an i18n concern. */
export interface FkBreadcrumbModel {
  sourceTable: string;
  localColumns: string[];
  targetTable: string;
  targetColumns: string[];
}

/** The display label of a foreign-key target. */
function targetLabelOf(referenceTable: string, targetColumns: readonly string[]): string {
  if (targetColumns.length === 1) return `${referenceTable}.${targetColumns[0]}`;
  return `${referenceTable}.(${targetColumns.join(', ')})`;
}

/**
 * Flatten a table's foreign keys into a `column → group` index.
 *
 * Every member column of a group points at the same group, so a composite key
 * stays one thing instead of splitting into single-column lookups. When two
 * constraints cover the same column the **first** one wins, mirroring the
 * "take the first ordered-columns match" rule of the request path.
 */
export function indexForeignKeys(
  foreignKeys: readonly ForeignKeyInfo[],
): ReadonlyMap<string, FkGroupAffordance> {
  const index = new Map<string, FkGroupAffordance>();
  for (const foreignKey of foreignKeys) {
    const group: FkGroupAffordance = {
      localColumns: [...foreignKey.columns],
      targetColumns: [...foreignKey.referencedColumns],
      targetLabel: targetLabelOf(foreignKey.referencedTable, foreignKey.referencedColumns),
      constraintName: foreignKey.name,
      referenceTable: foreignKey.referencedTable,
    };
    for (const column of foreignKey.columns) {
      if (index.has(column)) continue;
      index.set(column, group);
    }
  }
  return index;
}

/**
 * Whether the cell may be followed.
 *
 * The cell must be a member of a group, the group must be complete in this
 * row (no member may be `null`/`undefined`), the cell must be the group's
 * first **visible** member (a composite key draws one arrow, on the first
 * visible column), and the cell must not be occupied by an editor.
 */
export function isFkCellNavigable(input: {
  columnName: string;
  row: Record<string, unknown> | undefined;
  index: ReadonlyMap<string, FkGroupAffordance>;
  visibleColumns: readonly string[];
  editing: boolean;
}): boolean {
  if (input.editing) return false;
  const group = input.index.get(input.columnName);
  if (!group) return false;
  const row = input.row;
  if (!row) return false;
  // The group is located as a whole: one missing member makes it unlocatable.
  for (const column of group.localColumns) {
    const value = row[column];
    if (value === null || value === undefined) return false;
  }
  // The arrow sits on the first visible member, in display order.
  const firstVisible = input.visibleColumns.find((column) => group.localColumns.includes(column));
  if (firstVisible === undefined) return false;
  return firstVisible === input.columnName;
}

/**
 * Build the equality conditions that describe the target row.
 *
 * `null` means "do not navigate": the lengths differ or a value is missing.
 * The conditions are never degraded into `IS NULL`.
 */
export function buildFkFilters(
  target: { columns: readonly string[] },
  values: readonly Value[],
): FilterCondition[] | null {
  if (target.columns.length === 0) return null;
  if (target.columns.length !== values.length) return null;
  const filters: FilterCondition[] = [];
  for (let index = 0; index < target.columns.length; index += 1) {
    const value = values[index];
    if (value === null || value === undefined) return null;
    filters.push({ column: target.columns[index], operator: 'eq', value });
  }
  return filters;
}

/**
 * A stable signature of one navigation step.
 *
 * The schema is part of the key: `public.orders` and `audit.orders` are two
 * different targets, and treating them as one would call a round trip a cycle.
 */
export function fkNavKey(target: FkNavTargetIdentity, values: readonly Value[]): string {
  return JSON.stringify([
    target.database,
    target.schema,
    target.table,
    [...target.columns],
    values.map((value) => (value === undefined ? null : value)),
  ]);
}

/**
 * Find the constraint a request refers to.
 *
 * The constraint name wins when it matches; otherwise the ordered local
 * columns must match exactly, because the pairing is positional.
 */
export function findForeignKey(
  foreignKeys: readonly ForeignKeyInfo[],
  columns: readonly string[],
  constraintName: string | null,
): ForeignKeyInfo | null {
  if (constraintName) {
    const named = foreignKeys.find((foreignKey) => foreignKey.name === constraintName);
    if (named) return named;
  }
  return (
    foreignKeys.find(
      (foreignKey) =>
        foreignKey.columns.length === columns.length &&
        foreignKey.columns.every((column, index) => column === columns[index]),
    ) ?? null
  );
}

/**
 * The row a returning tab should be anchored on.
 *
 * The active cell is the best anchor **only while it is on screen**: the grid
 * keeps its active cell when the user scrolls, so a cell that scrolled out of
 * the viewport says nothing about what the user was looking at — restoring to
 * it would jump the tab back to the top and lose the position the jump started
 * from. When the active cell is off screen the first visible row is the anchor
 * instead.
 */
export function pickAnchorRow(input: {
  scrollTop: number;
  rowHeight: number;
  viewportHeight: number;
  activeRow: number | null;
}): { anchorRow: number; anchorVisible: boolean } {
  const rowHeight = Math.max(1, input.rowHeight);
  const firstVisible = Math.max(0, Math.floor(input.scrollTop / rowHeight));
  const visibleRows = Math.max(1, Math.floor(input.viewportHeight / rowHeight));
  const lastVisible = firstVisible + visibleRows - 1;
  const activeRow = input.activeRow;
  if (activeRow !== null && activeRow >= firstVisible && activeRow <= lastVisible) {
    return { anchorRow: activeRow, anchorVisible: true };
  }
  return { anchorRow: firstVisible, anchorVisible: false };
}

/**
 * The scroll offset that brings an anchor row back into view.
 *
 * Row virtualization destroys the row, so the position is restored first and
 * the anchor is located afterwards.
 */
export function scrollTopForAnchorRow(anchorRow: number, rowHeight: number): number {
  if (!Number.isFinite(anchorRow) || !Number.isFinite(rowHeight)) return 0;
  return Math.max(0, anchorRow * rowHeight);
}
