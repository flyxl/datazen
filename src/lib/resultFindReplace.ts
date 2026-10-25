/**
 * The in-result find and replace (DB-14): the pure rules over one statement's
 * loaded rows.
 *
 * Everything here is a plain function over plain data, because the rules are
 * the part that can be wrong in ways a DOM cannot show: what "not contains"
 * means for a NULL cell, which rows a filter keeps, which cells a replace-all
 * would write. The result grid renders the answers; it never decides them.
 *
 * Three rules run through the module:
 *
 * * **The search works on the text the grid shows.** A cell matches as its
 *   rendered text (`String(value)`); a NULL never contains anything.
 * * **"Not contains" keeps the rows that do not hold the value — including
 *   NULL cells.** A NULL does not contain the needle, so it belongs in the
 *   answer; a "not contains" that loses every NULL row would read as "the
 *   filter ate my data".
 * * **A replacement plan writes by original coordinates.** The filter may
 *   hide rows, but a write always names the row it means: the plan carries
 *   the index into the statement's own rows, never the filtered view's.
 */

import type { Value } from '../types';

/** How the needle must relate to a cell's text. */
export type ResultFindOperator = 'contains' | 'notContains' | 'prefix' | 'suffix';

/** The find bar's whole state. */
export interface ResultFindQuery {
  /** The text to look for; an empty needle means "no filter". */
  needle: string;
  /** Which column to look in; `null` means any column. */
  column: string | null;
  /** How the needle must relate to a cell's text. */
  operator: ResultFindOperator;
  /** Whether upper and lower case are distinct. */
  matchCase: boolean;
}

/** The column names a query may address, in result order. */
export type ResultColumnNames = readonly string[];

/** One planned cell write, in the statement's own coordinates. */
export interface ResultReplacePlanItem {
  /** The index into the statement's own rows. */
  row: number;
  /** The column's name. */
  column: string;
  /** The value the cell becomes. */
  value: string;
}

/** The empty query: no needle, any column, contains, case-insensitive. */
export const EMPTY_RESULT_FIND_QUERY: ResultFindQuery = {
  needle: '',
  column: null,
  operator: 'contains',
  matchCase: false,
};

/** A cell's text as the grid shows it; NULL shows as nothing at all. */
function cellText(value: Value | null): string | null {
  if (value === null || value === undefined) return null;
  if (typeof value === 'string') return value;
  return String(value);
}

/** Whether one cell matches the query. */
export function cellMatches(
  value: Value | null,
  query: Pick<ResultFindQuery, 'needle' | 'operator' | 'matchCase'>,
): boolean {
  const text = cellText(value);
  // The case folding of both sides is decided once, by the query.
  const needle = query.matchCase ? query.needle : query.needle.toLowerCase();
  const hay = query.matchCase ? text : (text?.toLowerCase() ?? null);
  if (hay === null) {
    // A NULL contains nothing: "not contains" is the only operator it
    // answers for.
    return query.operator === 'notContains' && needle !== '';
  }
  switch (query.operator) {
    case 'contains':
      return needle !== '' && hay.includes(needle);
    case 'notContains':
      return !hay.includes(needle);
    case 'prefix':
      return needle !== '' && hay.startsWith(needle);
    case 'suffix':
      return needle !== '' && hay.endsWith(needle);
  }
}

/** Whether one row matches the query in its addressed column (or any). */
export function rowMatches(
  row: readonly (Value | null)[],
  columns: ResultColumnNames,
  query: ResultFindQuery,
): boolean {
  if (query.needle === '') return true;
  if (query.column === null) {
    // Any-column "does not contain" means **the row** does not hold the
    // value: matching on any single column would match every row that has
    // at least one other cell.
    if (query.operator === 'notContains') {
      return columns.every((_, index) => cellMatches(row[index] ?? null, query));
    }
    return columns.some((_, index) => cellMatches(row[index] ?? null, query));
  }
  const index = columns.indexOf(query.column);
  if (index < 0) return true;
  return cellMatches(row[index] ?? null, query);
}

/**
 * The indexes of the rows a filter keeps, in the statement's own order.
 *
 * An empty needle keeps every row: opening the bar must not blank the grid.
 */
export function filterResultRows(
  rows: readonly (readonly (Value | null)[])[],
  columns: ResultColumnNames,
  query: ResultFindQuery,
): number[] {
  const kept: number[] = [];
  for (let index = 0; index < rows.length; index += 1) {
    if (rowMatches(rows[index] ?? [], columns, query)) kept.push(index);
  }
  return kept;
}

/**
 * The cells a replacement would write.
 *
 * Only the **matching** cells are replaced — a replace-all over "contains"
 * replaces the needle where it appears, and leaves the rest of every cell's
 * text alone (a full-cell overwrite would destroy data the needle never
 * named). The plan is ordered row-first, so a caller committing it in order
 * reads like reading the grid.
 */
export function planResultReplacements(
  rows: readonly (readonly (Value | null)[])[],
  columns: ResultColumnNames,
  query: ResultFindQuery,
  replacement: string,
  options: { limit?: number } = {},
): ResultReplacePlanItem[] {
  if (query.needle === '' || query.needle === replacement) return [];
  const limit = options.limit ?? Number.POSITIVE_INFINITY;
  const plan: ResultReplacePlanItem[] = [];
  for (let rowIndex = 0; rowIndex < rows.length && plan.length < limit; rowIndex += 1) {
    const row = rows[rowIndex] ?? [];
    for (
      let columnIndex = 0;
      columnIndex < columns.length && plan.length < limit;
      columnIndex += 1
    ) {
      const name = columns[columnIndex];
      if (query.column !== null && name !== query.column) continue;
      const value = row[columnIndex] ?? null;
      if (!cellMatches(value, query)) continue;
      const text = cellText(value);
      if (text === null) continue;
      plan.push({ row: rowIndex, column: name, value: replaceText(text, query, replacement) });
    }
  }
  return plan;
}

/** Replace every occurrence of the needle inside one cell's text. */
function replaceText(text: string, query: ResultFindQuery, replacement: string): string {
  if (query.matchCase) {
    return text.split(query.needle).join(replacement);
  }
  // Case-insensitive without regex: fold both sides, splice by offsets.
  let result = '';
  let rest = text;
  const needle = query.needle.toLowerCase();
  for (;;) {
    const at = rest.toLowerCase().indexOf(needle);
    if (at < 0 || needle === '') break;
    result += rest.slice(0, at) + replacement;
    rest = rest.slice(at + needle.length);
  }
  return result + rest;
}

/** The first plan item at or after `from`, or `null` when none remains. */
export function nextPlanItem(
  plan: readonly ResultReplacePlanItem[],
  from: number,
): ResultReplacePlanItem | null {
  return plan.find((item) => item.row >= from) ?? null;
}
