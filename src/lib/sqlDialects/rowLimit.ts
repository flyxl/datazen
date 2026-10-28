/**
 * The row cap of a `SELECT`, in the shape the dialect spells it.
 *
 * The cap is a dialect fact, not a host decision: `LIMIT 200` is a syntax
 * error on SQL Server (which wants `TOP 200` inside the select list) and on
 * Oracle 12c+ (which wants `FETCH FIRST 200 ROWS ONLY` at the end). The
 * dialect declares its own shape — see {@link RowLimitShape} — and this
 * module weaves that shape into the statement. The host never spells `TOP`
 * or `FETCH FIRST` itself, and never branches on a driver name.
 */
import type { RowLimitShape, SqlDialectStrategy } from '@datazen/extension-points';

/** The standard cap: `LIMIT n` at the end of the statement. */
export const LIMIT_ROW_LIMIT: RowLimitShape = {
  placement: 'trailing',
  clause: (limit) => `LIMIT ${limit}`,
};

/** The parts of a statement the row cap is woven into. */
export interface RowLimitedSelectParts {
  /** Whether the statement selects distinct values. */
  readonly distinct: boolean;
  /** The already-quoted column list. */
  readonly columns: readonly string[];
  /** The already-quoted table reference. */
  readonly table: string;
  /** The number of rows the statement is capped at. */
  readonly limit: number;
}

/**
 * Build a `SELECT` whose row count is capped the way the dialect caps it.
 *
 * `strategy` is the dialect's own strategy object; `null` means "no dialect
 * known", which falls back to the standard `LIMIT n` — the shape every
 * dialect that has not declared a cap of its own accepts.
 */
export function buildRowLimitedSelect(
  strategy: SqlDialectStrategy | null,
  parts: RowLimitedSelectParts,
): string {
  const shape = strategy?.rowLimit ?? LIMIT_ROW_LIMIT;
  const head = parts.distinct ? 'SELECT DISTINCT' : 'SELECT';
  const columns = parts.columns.join(', ');
  if (shape.placement === 'select-list') {
    return `${head} ${shape.clause(parts.limit)} ${columns} FROM ${parts.table}`;
  }
  return `${head} ${columns} FROM ${parts.table} ${shape.clause(parts.limit)}`;
}
