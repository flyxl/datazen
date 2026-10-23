/**
 * Where did the row inserted by a commit end up?
 *
 * This batch does **not** implement the RETURNING backfill: the host never
 * calls `insert_returning_clause`, so an insert statement comes back without a
 * row identity (04 §5 Step 6, decision 7). The rule is therefore:
 *
 * 1. an insert statement that *does* carry a row identity is located exactly;
 * 2. otherwise a **candidate** is taken — the row with the largest value of a
 *    single comparable primary key — and it is only accepted when that value
 *    grew compared to the page before the commit;
 * 3. otherwise the row is **not** located: the page is refreshed and the UI
 *    says the new row could not be located. The interface must never claim
 *    that it can point at the new row.
 *
 * Rule 2 is a candidate, not a guarantee — a concurrent insert can produce the
 * same maximum — which is exactly why the UI wording stays neutral.
 */

import type { ColumnSchema, Value } from '../../types';
import {
  buildRowIdentity,
  rowIdentityKey,
  type CommitStatementResult,
  type InsertLocation,
} from '../../lib/tableChanges';

export interface InsertLocationResult {
  status: InsertLocation;
  /** Identity key of the located row; `null` when the row could not be located. */
  key: string | null;
}

const UNAVAILABLE: InsertLocationResult = { status: 'unavailable', key: null };
const NOT_APPLICABLE: InsertLocationResult = { status: 'not-applicable', key: null };

/** Identity key of a row, or `null` when the row has no stable identity. */
function identityKeyOf(
  row: Record<string, unknown>,
  primaryKeyColumns: ColumnSchema[],
): string | null {
  const identity = buildRowIdentity(row, primaryKeyColumns);
  if (!identity) return null;
  try {
    return rowIdentityKey(identity);
  } catch {
    return null;
  }
}

/** Does the row carry exactly the values of the given identity? */
function identityMatches(
  row: Record<string, unknown>,
  identity: Record<string, Value | null>,
): boolean {
  const columns = Object.keys(identity);
  if (columns.length === 0) return false;
  return columns.every((column) => {
    const actual = row[column];
    if (actual === undefined || actual === null) return false;
    return JSON.stringify(actual) === JSON.stringify(identity[column]);
  });
}

function compareValues(left: number | string, right: number | string): number {
  if (typeof left === 'number' && typeof right === 'number') return left - right;
  if (typeof left === 'string' && typeof right === 'string') {
    if (left < right) return -1;
    if (left > right) return 1;
  }
  return 0;
}

interface ComparableMax {
  row: Record<string, unknown>;
  value: number | string;
}

/**
 * The largest value of a column, when every row carries a comparable one.
 *
 * `comparable: false` means "the values cannot be ordered" (a UUID, a date, a
 * buffer, or a mix of numbers and strings) — such a column can never locate a
 * row, so the caller has to degrade.
 */
/** A value that can be ordered: a number or a string. */
function isComparableValue(value: unknown): value is number | string {
  return typeof value === 'number' || typeof value === 'string';
}

function comparableMax(
  rows: Record<string, unknown>[],
  column: string,
): { max: ComparableMax | null; comparable: boolean } {
  if (rows.length === 0) return { max: null, comparable: true };
  let max: ComparableMax | null = null;
  let kind: 'number' | 'string' | null = null;
  for (const row of rows) {
    const raw = row[column];
    if (!isComparableValue(raw)) return { max: null, comparable: false };
    // `typeof` is not narrowed by the type guard, so the kind is derived explicitly.
    const valueKind = typeof raw === 'number' ? 'number' : 'string';
    if (kind === null) kind = valueKind;
    else if (kind !== valueKind) return { max: null, comparable: false };
    if (max === null || compareValues(raw, max.value) > 0) max = { row, value: raw };
  }
  return { max, comparable: true };
}

/**
 * Locate the row inserted by a commit.
 *
 * @param previousRows the rows as they were before the commit (the candidate
 *   rule needs the previous maximum to tell "the new row" from "a row that was
 *   already there");
 * @param refreshedRows the rows of the page after the refresh;
 * @param primaryKeyColumns the primary key columns of the table;
 * @param statements the statements the commit reported.
 */
export function locateInsertedRow(
  previousRows: Record<string, unknown>[],
  refreshedRows: Record<string, unknown>[],
  primaryKeyColumns: ColumnSchema[],
  statements: CommitStatementResult[],
): InsertLocationResult {
  const inserts = statements.filter((statement) => statement.operation === 'insert');
  if (inserts.length === 0) return NOT_APPLICABLE;

  // 1. An insert that carries an identity is located exactly.
  for (const statement of inserts) {
    const identity = statement.rowIdentity;
    if (Object.keys(identity).length === 0) continue;
    const match = refreshedRows.find((row) => identityMatches(row, identity));
    if (!match) continue;
    const key = identityKeyOf(match, primaryKeyColumns);
    if (key !== null) return { status: 'located', key };
  }

  // 2. The candidate rule: one comparable primary key, and its maximum grew.
  if (primaryKeyColumns.length !== 1) return UNAVAILABLE;
  const column = primaryKeyColumns[0].name;
  const refreshed = comparableMax(refreshedRows, column);
  if (!refreshed.comparable || refreshed.max === null) return UNAVAILABLE;
  const previous = comparableMax(previousRows, column);
  if (previous.max === null) {
    // Nothing comparable before the commit: only a page that holds exactly one
    // row can be attributed to the insert.
    if (refreshedRows.length !== 1) return UNAVAILABLE;
  } else if (compareValues(refreshed.max.value, previous.max.value) <= 0) {
    // The maximum did not grow, so the candidate is a row that was already
    // there. Highlighting it would point at the wrong row.
    return UNAVAILABLE;
  }

  const key = identityKeyOf(refreshed.max.row, primaryKeyColumns);
  if (key === null) return UNAVAILABLE;
  return { status: 'located', key };
}
