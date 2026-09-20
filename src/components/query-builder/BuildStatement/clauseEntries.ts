import type { QbAggregate, QbColumnSelection, QbGroupByItem, QbSortItem } from '../types';

/**
 * A GROUP BY / ORDER BY entry as shown in the clause list.
 *
 * `GROUP BY` and `ORDER BY` are each fed by *two* store fields: the dedicated
 * array (`groupBy` / `orderBy`) and the per-column flags
 * (`QbColumnSelection.groupBy` / `.sort`). The clause list shows their union,
 * so every entry has to remember which side it came from — that is what makes
 * removing it here clear the right one instead of leaving a phantom key behind.
 */
export interface ClauseEntry {
  table: string;
  column: string;
  source: 'store' | 'column';
  /** Index into the store-level array (`source === 'store'` only). */
  index?: number;
  /** ORDER BY only: direction. */
  direction?: 'ASC' | 'DESC';
  /** ORDER BY only: aggregate wrapping the key (`ORDER BY SUM(x)`). */
  aggregate?: QbAggregate;
}

/** Union of the store-level GROUP BY list and the per-column group-by flags. */
export function buildGroupByEntries(
  groupBy: QbGroupByItem[],
  selectedColumns: QbColumnSelection[],
): ClauseEntry[] {
  const entries: ClauseEntry[] = groupBy.map((item, index) => ({
    table: item.table,
    column: item.column,
    source: 'store' as const,
    index,
  }));
  const seen = new Set(entries.map((e) => `${e.table}.${e.column}`));
  for (const col of selectedColumns) {
    const key = `${col.table}.${col.column}`;
    if (!col.groupBy || seen.has(key)) continue;
    seen.add(key);
    entries.push({ table: col.table, column: col.column, source: 'column' });
  }
  return entries;
}

/** Union of the store-level ORDER BY list and the per-column sort flags. */
export function buildOrderByEntries(
  orderBy: QbSortItem[],
  selectedColumns: QbColumnSelection[],
): ClauseEntry[] {
  const entries: ClauseEntry[] = orderBy.map((item, index) => ({
    table: item.table,
    column: item.column,
    source: 'store' as const,
    index,
    direction: item.direction,
    aggregate: item.aggregate,
  }));
  const seen = new Set(entries.map((e) => `${e.table}.${e.column}`));
  for (const col of selectedColumns) {
    const key = `${col.table}.${col.column}`;
    if (!col.sort || seen.has(key)) continue;
    seen.add(key);
    entries.push({
      table: col.table,
      column: col.column,
      source: 'column',
      direction: col.sort,
      aggregate: col.aggregate,
    });
  }
  return entries;
}
