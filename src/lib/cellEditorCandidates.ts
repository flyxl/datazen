/**
 * The candidate values of an enumeration column.
 *
 * A `SELECT DISTINCT` is built **here**, as a pure function, and executed by
 * the caller through the existing query command. Two reasons for the split:
 *
 * - the statement must be assertable — one statement, a hard limit, properly
 *   quoted identifiers — and a builder that reaches for the network cannot be
 *   unit tested at all;
 * - the host never branches on a driver id for SQL. The identifiers are
 *   quoted with the dialect's own quote character, and the caller supplies
 *   it;
 * - the row cap is the dialect's own shape too: `LIMIT` is not legal on
 *   SQL Server or Oracle, so the dialect declares how it caps rows and the
 *   host asks it.
 *
 * The query is only ever run **after the user asks for it**. Opening an
 * editor never queries anything (05 §4.3 #5): a `DISTINCT` over a big table
 * on every double click is how a grid starts to stutter.
 */
import type { DatabaseType } from '../types';
import { escapeIdent } from './databaseTypes';
import { buildRowLimitedSelect, getSqlDialect } from './sqlDialects';

/** The hard cap on the number of candidates. */
export const CANDIDATE_VALUE_LIMIT = 200;

export interface DistinctCandidatesArgs {
  /** The table the column belongs to. */
  readonly table: string;
  /** The column the candidates are read from. */
  readonly column: string;
  /** The schema, when the dialect has one. */
  readonly schema?: string | null;
  /** The database, when the dialect has one. */
  readonly database?: string | null;
  /** The dialect whose quote character is used for the identifiers. */
  readonly dialect?: DatabaseType;
  /** The row limit; capped at {@link CANDIDATE_VALUE_LIMIT}. */
  readonly limit?: number;
}

/**
 * The `SELECT DISTINCT` that reads the candidates of one column.
 *
 * Returns `null` when the request cannot be expressed (no column name): the
 * caller then shows "no candidates" instead of running a broken statement.
 * The result is always a **single** statement — no semicolon, no second
 * query — so it can never be used to smuggle anything into the session.
 */
export function buildDistinctCandidatesSql(args: DistinctCandidatesArgs): string | null {
  const column = args.column.trim();
  if (column === '') return null;
  const table = args.table.trim();
  if (table === '') return null;

  const limit = Math.min(
    Math.max(1, Math.trunc(args.limit ?? CANDIDATE_VALUE_LIMIT)),
    CANDIDATE_VALUE_LIMIT,
  );

  const dialect = args.dialect;
  const quotedColumn = escapeIdent(column, dialect);
  const schema = args.schema?.trim();
  const quotedTable =
    schema && schema !== ''
      ? `${escapeIdent(schema, dialect)}.${escapeIdent(table, dialect)}`
      : escapeIdent(table, dialect);

  // The cap is the dialect's own shape: `LIMIT` on PostgreSQL and SQLite,
  // `TOP n` inside the select list on SQL Server, `FETCH FIRST n ROWS ONLY`
  // on Oracle. `LIMIT` is a syntax error on the latter two, so the host
  // asks the dialect instead of spelling the cap itself.
  return buildRowLimitedSelect(dialect ? getSqlDialect(dialect) : null, {
    distinct: true,
    columns: [quotedColumn],
    table: quotedTable,
    limit,
  });
}
