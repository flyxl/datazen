import { escapeIdent } from './databaseTypes';
import type { DatabaseType, Value } from '../types';

/**
 * Column statistics (DB-12): `count` / `distinct` / `nulls` / `min` / `max` /
 * `avg` for one column of one table, computed **by the server**.
 *
 * The numbers must never come from the loaded page — a 100M-row table (the
 * PRD's acceptance case) would be both wrong and slow. Everything here is a
 * thin SQL builder over the connection's own query path, so the aggregate
 * runs where the data lives and the UI only renders six values.
 */

/** The table half of a stats target: identity + dialect for quoting. */
export interface ColumnStatsTable {
  databaseType: DatabaseType;
  /** PG-family schema qualifier; `null` routes via the database only. */
  schema: string | null;
  tableName: string;
}

/** What one column's stats came out as. `null` = "the server had no value". */
export interface ColumnStats {
  /** `COUNT(*)` — the rows of the table, independent of the column. */
  rowCount: number | null;
  /** `COUNT(col)` — the non-null values of the column. */
  nonNullCount: number | null;
  /** `rowCount - nonNullCount` — the NULL cells of the column. */
  nullCount: number | null;
  /** `COUNT(DISTINCT col)`. */
  distinctCount: number | null;
  /** `MIN(col)` as the server reported it. */
  min: Value | null;
  /** `MAX(col)` as the server reported it. */
  max: Value | null;
  /** `AVG(col)`; `null` when the column type cannot be averaged. */
  avg: Value | null;
  /** Whether `avg` was asked of the server at all. */
  supportsAvg: boolean;
}

/**
 * Base type names whose `AVG(col)` is defined in every supported dialect.
 *
 * Chosen from the driver-reported `ColumnSchema.dataType` (lower-cased, the
 * width suffix cut): the numeric families aggregate everywhere, while e.g.
 * `AVG(text)` does not exist in PostgreSQL and would fail the whole single
 * statement — one non-aggregable column must not cost the other five values.
 */
const AVERAGABLE_TYPES: ReadonlySet<string> = new Set([
  'int',
  'integer',
  'tinyint',
  'smallint',
  'mediumint',
  'bigint',
  'int2',
  'int4',
  'int8',
  'decimal',
  'numeric',
  'number',
  'float',
  'float4',
  'float8',
  'double',
  'real',
  'money',
  'serial',
  'smallserial',
  'bigserial',
]);

/**
 * Whether `AVG(col)` is worth asking for. Unknown types err on the side of
 * *not* averaging: a text column shows five honest values instead of six —
 * asking would abort the statement on PostgreSQL.
 *
 * The base name — lower-cased, width suffix cut — decides, but the first
 * word decides too: `double precision` (PG) and `int unsigned` (MySQL) are
 * numeric families written in two words.
 */
export function isAveragableType(dataType: string | null | undefined): boolean {
  if (!dataType) return false;
  const base = dataType.trim().toLowerCase().split('(')[0].trim();
  if (AVERAGABLE_TYPES.has(base)) return true;
  const firstWord = base.split(/\s+/)[0];
  return firstWord != null && AVERAGABLE_TYPES.has(firstWord);
}

/** Quote `schema.table` for the dialect — the same walk the SQL templates use. */
function qualifiedTable(table: ColumnStatsTable): string {
  return [table.schema, table.tableName]
    .filter((segment): segment is string => Boolean(segment))
    .map((segment) => escapeIdent(segment, table.databaseType))
    .join('.');
}

/** The stable projection order the row parser below expects. */
const AVGLESS_ALIASES = [
  'row_count',
  'non_null_count',
  'distinct_count',
  'min_value',
  'max_value',
] as const;

/**
 * The single-statement aggregate for one column: one round trip, computed
 * server-side (DB-12 acceptance: a 100M-row table is aggregated by the
 * server, never pulled).
 *
 * Aliases stay unquoted snake_case — valid as bare identifiers in every
 * dialect, and nothing reads them (the row is parsed by position).
 */
export function buildColumnStatsSql(
  table: ColumnStatsTable,
  column: string,
  opts: { includeAvg: boolean },
): string {
  const quotedColumn = escapeIdent(column, table.databaseType);
  const aliases = opts.includeAvg ? [...AVGLESS_ALIASES, 'avg_value'] : [...AVGLESS_ALIASES];
  const selectors = [
    `COUNT(*) AS ${aliases[0]}`,
    `COUNT(${quotedColumn}) AS ${aliases[1]}`,
    `COUNT(DISTINCT ${quotedColumn}) AS ${aliases[2]}`,
    `MIN(${quotedColumn}) AS ${aliases[3]}`,
    `MAX(${quotedColumn}) AS ${aliases[4]}`,
    ...(opts.includeAvg ? [`AVG(${quotedColumn}) AS ${aliases[5]}`] : []),
  ];
  return `SELECT ${selectors.join(', ')} FROM ${qualifiedTable(table)}`;
}

/**
 * `COUNT` reaches the UI as a number in most drivers, but wider counts may
 * arrive as decimal strings; both parse, anything unusable reads as `null`.
 */
function toCount(value: Value | null | undefined): number | null {
  if (value == null) return null;
  if (typeof value === 'number' && Number.isFinite(value)) return value;
  if (typeof value === 'string' && value.trim() !== '') {
    const parsed = Number(value);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

/**
 * Parse the single aggregate row, by the position `buildColumnStatsSql`
 * projected it in. A missing row part (a driver that skipped a value) reads
 * as `null` — the dialog renders an honest "—", never a wrong number.
 */
export function parseColumnStatsRow(
  row: readonly (Value | null)[] | null | undefined,
  opts: { includeAvg: boolean },
): ColumnStats {
  const cells = row ?? [];
  const rowCount = toCount(cells[0]);
  const nonNullCount = toCount(cells[1]);
  return {
    rowCount,
    nonNullCount,
    nullCount:
      rowCount != null && nonNullCount != null ? Math.max(0, rowCount - nonNullCount) : null,
    distinctCount: toCount(cells[2]),
    min: cells[3] ?? null,
    max: cells[4] ?? null,
    avg: opts.includeAvg ? (cells[5] ?? null) : null,
    supportsAvg: opts.includeAvg,
  };
}
