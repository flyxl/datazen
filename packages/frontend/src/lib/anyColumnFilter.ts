/**
 * The Any Column filter (DBR-04): one value, matched across every column
 * that can be text-searched.
 *
 * The expansion is deliberately a **generator of structured conditions** —
 * one `contains` condition per searchable column, joined with `or` — and not
 * a second query dialect. Everything downstream (the operator whitelist, the
 * SQL rendering, the preview, the count, the saved views) already speaks
 * `FilterCondition`; reusing that channel means the Any Column filter can
 * never drift from the structured one, and the SQL preview shows exactly the
 * statement that runs.
 *
 * Which columns are searchable follows the value search's declaration
 * (DBR-03): text / enum / unknown always, plus the type groups the driver
 * declared its `LIKE` can text-match. A column whose type is not in the set
 * is **skipped and reported**, never silently matched with a cast the
 * database may reject — a type-cast failure must not masquerade as
 * "no match" (PRD DBR-04).
 */

import { resolveCellType, type CellTypeHints } from './cellTypes';
import { cellTypesForScope, type ValueSearchTypeGroup } from './valueSearch';
import type { ColumnSchema, FilterCondition } from '../types';

/** What the expansion produced: the conditions, and what it left out. */
export interface AnyColumnExpansion {
  /** One `contains` condition per searchable column, OR-joined. */
  conditions: FilterCondition[];
  /** The columns the filter will actually search. */
  searchableColumns: string[];
  /** The columns skipped because their type is not text-searchable. */
  skippedColumns: string[];
}

/**
 * Expand a value into the structured conditions of the Any Column filter.
 *
 * The value is matched as a `contains` (a substring, escaped by the shared
 * filter layer — the user's `%` is literal here); an empty value expands to
 * **no conditions**, which is the caller's signal to clear the filter rather
 * than ask a question that matches nothing.
 */
export function expandAnyColumnFilter(
  columns: readonly ColumnSchema[],
  value: string,
  options: { groups: readonly ValueSearchTypeGroup[]; hints?: CellTypeHints },
): AnyColumnExpansion {
  const searchable = cellTypesForScope(
    options.groups.length === 0
      ? 'text'
      : options.groups.includes('number') && options.groups.includes('datetime')
        ? 'text+number+datetime'
        : options.groups.includes('number')
          ? 'text+number'
          : 'text+datetime',
  );

  const trimmed = value.trim();
  const conditions: FilterCondition[] = [];
  const searchableColumns: string[] = [];
  const skippedColumns: string[] = [];

  for (const column of columns) {
    const cellType = resolveCellType(column.dataType, options.hints);
    if (!searchable.has(cellType)) {
      skippedColumns.push(column.name);
      continue;
    }
    searchableColumns.push(column.name);
    if (trimmed === '') continue;
    conditions.push({
      column: column.name,
      operator: 'contains',
      value: trimmed,
      // Every condition joins the previous one with OR: the Any Column
      // question is "does the value appear in **any** column".
      ...(conditions.length > 0 ? { connector: 'or' as const } : {}),
    });
  }

  return { conditions, searchableColumns, skippedColumns };
}
