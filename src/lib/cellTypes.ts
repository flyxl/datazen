/**
 * The normalised cell type — the host's **single** answer to "what kind of
 * value does this column hold".
 *
 * ## Why a table and not a substring test
 *
 * The drivers report the *dialect's* type name, not a type system: MySQL
 * reports `COLUMN_TYPE` (`int(11) unsigned`, `enum('a','b')`), PostgreSQL
 * reports `information_schema.columns.data_type` (every enum and composite
 * type is `USER-DEFINED`, every array is `ARRAY`), SQL Server builds the
 * string with a `CASE` expression, and a query result reports whatever
 * sqlx calls the type (`INT4`, `TEXT`, `TIMESTAMPTZ`).
 *
 * The host used to guess with `type.includes('int')`, which is why
 * `classifyDataType('point')` answered `number` — a PostgreSQL geometry type
 * is not a number. The rules below therefore never use a bare substring for
 * the numeric families; they match whole tokens, and the few genuinely
 * ambiguous names (`point`, `inet`, `money`, …) are blocked **before** the
 * numeric rules can see them.
 *
 * ## One authority, many projections
 *
 * `CellType` is the only type judgement in the host. The cell colour
 * (`cellTypeToDataTypeFamily`), the editor control
 * (`resolveCellEditorKind`, `src/lib/cellEditorKind.ts`) and the filter
 * operators (`src/lib/filterOperators.ts`) are all **projections** of it, so
 * "the grid paints it as text but edits it as a number" cannot happen.
 *
 * ## Dialect traps belong to the driver
 *
 * A shared table can only carry the common SQL type names. The traps that
 * are specific to one dialect (SQL Server's `timestamp`/`rowversion`, MySQL's
 * `tinyint(1)`) are declared by the driver itself in
 * `DatabaseTypeMeta.cellTypes` — never by a `if (driverType === …)` in the
 * host.
 */
import type { DataTypeFamily } from './dataTypeColors';

/**
 * The normalised type of a column.
 *
 * `unknown` is a real value, not an error: it means "neither the shared table
 * nor the driver recognised this name". The editor treats it as text and the
 * grid never complains about it — but it is also the signal that a driver
 * owes the host one more rule.
 */
export type CellType =
  | 'text' // character / text / unknown text family
  | 'number' // integer, fixed point, float
  | 'bool' // boolean
  | 'date' // date only
  | 'time' // time only
  | 'datetime' // date + time, with or without a zone
  | 'json' // structured value (JSON / JSONB / array / VARIANT)
  | 'bytes' // binary
  | 'enum' // enum / low-cardinality column
  | 'uuid' // UUID / uniqueidentifier
  | 'unknown'; // not classified — handled as text, never as an error

/**
 * How a rule matches a type name.
 *
 * The kinds exist because "one substring test for everything" is the defect
 * this table replaces: `point`.includes('int') is true.
 */
export type CellTypeMatch =
  | 'exact' // the whole name equals the pattern
  | 'prefix' // the name starts with the pattern
  | 'suffix' // the name ends with the pattern
  | 'token' // some token equals the pattern
  | 'tokenPrefix' // some token starts with the pattern
  | 'substring'; // the name contains the pattern (only where it is unambiguous)

export interface CellTypeRule {
  readonly match: CellTypeMatch;
  /** A lower-case literal. Never a regular expression. */
  readonly pattern: string;
  readonly cellType: CellType;
}

/**
 * What a driver declares about its own types.
 *
 * Every field is optional: a driver that declares nothing keeps the shared
 * table's behaviour exactly (00-contracts R4).
 */
export interface CellTypeHints {
  /**
   * Driver rules, evaluated **before** the shared table; the first hit wins.
   * This is where a dialect trap is corrected (`timestamp` in SQL Server is a
   * row version, not a time).
   */
  readonly overrides?: readonly CellTypeRule[];
  /**
   * Type names this driver uses for enumerations (token / token-prefix
   * match) → `CellType` `enum`. PostgreSQL reports every enum column as
   * `USER-DEFINED`, so only the driver can know what that means.
   */
  readonly enumTypes?: readonly string[];
  /**
   * Type names whose values carry a zone (`timestamptz`, `datetimeoffset`).
   * They decide whether the date editor may use the picker: the picker
   * cannot carry an offset, so a zoned column is edited as plain text.
   */
  readonly zonedTypes?: readonly string[];
  /**
   * Type names that cannot be written in place in this database (SQL Server's
   * `rowversion`) → read-only reason `typeNotEditable`.
   */
  readonly readOnlyTypes?: readonly string[];
}

/**
 * The shared default table, in evaluation order.
 *
 * The order **is** the priority, and two orderings are load-bearing:
 *
 * - `json` comes before the binary rules, so a composite name like
 *   `binary json` is JSON;
 * - the explicit text block (geometry, network, `money`, …) comes **before**
 *   the numeric rules, so `point` is text and not a number.
 *
 * Both are asserted by tests; changing the order without changing the tests
 * is a behaviour change, not a refactor.
 */
export const DEFAULT_CELL_TYPE_RULES: readonly CellTypeRule[] = [
  // ── low-cardinality enumerations ───────────────────────────────────────
  { match: 'tokenPrefix', pattern: 'enum', cellType: 'enum' },
  { match: 'tokenPrefix', pattern: 'set', cellType: 'enum' },

  // ── structured values ──────────────────────────────────────────────────
  { match: 'substring', pattern: 'json', cellType: 'json' },
  { match: 'token', pattern: 'array', cellType: 'json' },
  { match: 'suffix', pattern: '[]', cellType: 'json' },

  // ── binary ─────────────────────────────────────────────────────────────
  { match: 'tokenPrefix', pattern: 'fixedstring', cellType: 'bytes' },
  { match: 'substring', pattern: 'bytea', cellType: 'bytes' },
  { match: 'substring', pattern: 'blob', cellType: 'bytes' },
  { match: 'substring', pattern: 'binary', cellType: 'bytes' },
  { match: 'exact', pattern: 'raw', cellType: 'bytes' },
  { match: 'exact', pattern: 'image', cellType: 'bytes' },

  // ── uuid ───────────────────────────────────────────────────────────────
  { match: 'tokenPrefix', pattern: 'uuid', cellType: 'uuid' },
  { match: 'exact', pattern: 'uniqueidentifier', cellType: 'uuid' },

  // ── booleans ───────────────────────────────────────────────────────────
  { match: 'substring', pattern: 'bool', cellType: 'bool' },

  // ── date and time ──────────────────────────────────────────────────────
  // `timestamp` and `datetime` come before the plain date/time rules, so
  // `datetime` is not stolen by `date` and `timestamp` not by `time`.
  { match: 'substring', pattern: 'timestamp', cellType: 'datetime' },
  { match: 'substring', pattern: 'datetime', cellType: 'datetime' },
  // PostgreSQL's `interval` is a text form — it gets no date picker.
  { match: 'token', pattern: 'interval', cellType: 'text' },
  { match: 'exact', pattern: 'date', cellType: 'date' },
  { match: 'tokenPrefix', pattern: 'date', cellType: 'date' },
  { match: 'exact', pattern: 'time', cellType: 'time' },
  { match: 'tokenPrefix', pattern: 'time', cellType: 'time' },

  // ── the explicit text block ────────────────────────────────────────────
  // These names are text on the wire, whatever they look like. They are
  // listed **before** the numeric rules on purpose: `point` contains `int`.
  { match: 'token', pattern: 'bit', cellType: 'text' },
  { match: 'token', pattern: 'varbit', cellType: 'text' },
  { match: 'token', pattern: 'xml', cellType: 'text' },
  { match: 'token', pattern: 'inet', cellType: 'text' },
  { match: 'token', pattern: 'cidr', cellType: 'text' },
  { match: 'token', pattern: 'macaddr', cellType: 'text' },
  { match: 'token', pattern: 'macaddr8', cellType: 'text' },
  { match: 'token', pattern: 'geometry', cellType: 'text' },
  { match: 'token', pattern: 'geography', cellType: 'text' },
  { match: 'token', pattern: 'point', cellType: 'text' },
  { match: 'token', pattern: 'polygon', cellType: 'text' },
  { match: 'token', pattern: 'linestring', cellType: 'text' },
  { match: 'token', pattern: 'hstore', cellType: 'text' },
  { match: 'token', pattern: 'year', cellType: 'text' },
  // `money` is money-formatted text on the wire (`$1,234.56`); parsing it as
  // a number would corrupt it (05 §7 #21 — no amount parsing).
  { match: 'token', pattern: 'money', cellType: 'text' },

  // ── numbers ────────────────────────────────────────────────────────────
  { match: 'tokenPrefix', pattern: 'int', cellType: 'number' },
  { match: 'token', pattern: 'bigint', cellType: 'number' },
  { match: 'token', pattern: 'smallint', cellType: 'number' },
  { match: 'token', pattern: 'tinyint', cellType: 'number' },
  { match: 'token', pattern: 'mediumint', cellType: 'number' },
  { match: 'substring', pattern: 'serial', cellType: 'number' },
  { match: 'tokenPrefix', pattern: 'numeric', cellType: 'number' },
  { match: 'tokenPrefix', pattern: 'decimal', cellType: 'number' },
  { match: 'tokenPrefix', pattern: 'dec', cellType: 'number' },
  { match: 'tokenPrefix', pattern: 'float', cellType: 'number' },
  { match: 'tokenPrefix', pattern: 'double', cellType: 'number' },
  { match: 'tokenPrefix', pattern: 'real', cellType: 'number' },
  { match: 'tokenPrefix', pattern: 'number', cellType: 'number' },

  // ── the character family ───────────────────────────────────────────────
  { match: 'tokenPrefix', pattern: 'varchar', cellType: 'text' },
  { match: 'tokenPrefix', pattern: 'nvarchar', cellType: 'text' },
  { match: 'tokenPrefix', pattern: 'nchar', cellType: 'text' },
  { match: 'tokenPrefix', pattern: 'char', cellType: 'text' },
  { match: 'token', pattern: 'bpchar', cellType: 'text' },
  { match: 'token', pattern: 'character', cellType: 'text' },
  { match: 'token', pattern: 'varying', cellType: 'text' },
  { match: 'tokenPrefix', pattern: 'text', cellType: 'text' },
  { match: 'token', pattern: 'tinytext', cellType: 'text' },
  { match: 'token', pattern: 'mediumtext', cellType: 'text' },
  { match: 'token', pattern: 'longtext', cellType: 'text' },
  { match: 'token', pattern: 'clob', cellType: 'text' },
  { match: 'token', pattern: 'string', cellType: 'text' },
  { match: 'token', pattern: 'citext', cellType: 'text' },
  { match: 'token', pattern: 'name', cellType: 'text' },
];

/**
 * Type names whose value carries a zone.
 *
 * These are standard SQL names plus the two de-facto spellings the drivers
 * use (`timestamptz`, `datetimeoffset`); they are **not** driver branches —
 * a driver may add more through {@link CellTypeHints.zonedTypes}.
 */
const DEFAULT_ZONED_TYPE_NAMES: readonly string[] = [
  'timestamp with time zone',
  'datetime with time zone',
  'timestamptz',
  'timetz',
  'datetimeoffset',
];

/** Type names that denote an integer column (no fraction). */
const INTEGER_TYPE_TOKENS: ReadonlySet<string> = new Set([
  'int',
  'int2',
  'int4',
  'int8',
  'int16',
  'int32',
  'int64',
  'integer',
  'bigint',
  'smallint',
  'tinyint',
  'mediumint',
  'serial',
  'serial2',
  'serial4',
  'serial8',
  'bigserial',
  'smallserial',
]);

/** Lower-case, trimmed type name. */
function normalizeTypeName(dataType: string | undefined | null): string {
  return (dataType ?? '').trim().toLowerCase();
}

/**
 * The tokens of a type name.
 *
 * A token is a run of letters, digits and underscores; everything else is a
 * separator. `int(11) unsigned` → `['int', '11', 'unsigned']`,
 * `Nullable(Int32)` → `['nullable', 'int32']`.
 */
export function tokenizeTypeName(type: string): string[] {
  return normalizeTypeName(type)
    .split(/[^a-z0-9_]+/)
    .filter((token) => token.length > 0);
}

/** Whether one rule matches a type name. */
export function matchCellTypeRule(type: string, rule: CellTypeRule): boolean {
  const normalized = normalizeTypeName(type);
  const pattern = rule.pattern.toLowerCase();
  switch (rule.match) {
    case 'exact':
      return normalized === pattern;
    case 'prefix':
      return normalized.startsWith(pattern);
    case 'suffix':
      return normalized.endsWith(pattern);
    case 'substring':
      return normalized.includes(pattern);
    case 'token':
      return tokenizeTypeName(normalized).includes(pattern);
    case 'tokenPrefix':
      return tokenizeTypeName(normalized).some((token) => token.startsWith(pattern));
  }
}

/**
 * Whether a driver's hint name matches a type name.
 *
 * Hints are written the way a driver writes its types, so they match on
 * tokens: `rowversion` matches `rowversion`, `timestamp` matches
 * `timestamp` and `timestamp with time zone`.
 */
function matchesHintName(type: string, name: string): boolean {
  const pattern = name.trim().toLowerCase();
  if (pattern === '') return false;
  const tokens = tokenizeTypeName(type);
  if (tokens.some((token) => token === pattern || token.startsWith(pattern))) return true;
  // A hint may name a **whole** type ("user-defined"), which is not a token
  // of it — PostgreSQL reports every enum column under that name.
  return type.includes(pattern);
}

/**
 * The normalised type of a column.
 *
 * Never throws: an unusable name answers `unknown`, which the grid treats as
 * text. The evaluation order is
 *
 * 1. the driver's own `overrides` (a dialect trap beats the shared table),
 * 2. the driver's `enumTypes` (only the driver knows that `USER-DEFINED` is
 *    an enum),
 * 3. the shared table,
 * 4. `unknown`.
 */
export function resolveCellType(
  dataType: string | undefined | null,
  hints?: CellTypeHints,
): CellType {
  const type = normalizeTypeName(dataType);
  if (type === '') return 'unknown';

  for (const rule of hints?.overrides ?? []) {
    if (matchCellTypeRule(type, rule)) return rule.cellType;
  }
  for (const name of hints?.enumTypes ?? []) {
    if (matchesHintName(type, name)) return 'enum';
  }
  for (const rule of DEFAULT_CELL_TYPE_RULES) {
    if (matchCellTypeRule(type, rule)) return rule.cellType;
  }
  return 'unknown';
}

/**
 * The render family of a cell type.
 *
 * The switch is **exhaustive and has no `default`**: adding a `CellType`
 * without projecting it here is a compile error, not a silent drift. The
 * `null` family is deliberately unreachable — it is a property of the
 * *value*, not of the type.
 */
export function cellTypeToDataTypeFamily(cellType: CellType): DataTypeFamily {
  switch (cellType) {
    case 'text':
      return 'text';
    case 'number':
      return 'number';
    case 'bool':
      return 'bool';
    case 'date':
    case 'time':
    case 'datetime':
      return 'datetime';
    case 'json':
      return 'json';
    case 'bytes':
      return 'binary';
    case 'enum':
    case 'uuid':
    case 'unknown':
      return 'text';
  }
}

/**
 * Whether the column's values carry a zone.
 *
 * A zoned column is edited as **plain text**: the date picker cannot carry
 * an offset, and silently dropping it would move the instant (05 §7 #8).
 */
export function isZonedCellType(
  dataType: string | undefined | null,
  hints?: CellTypeHints,
): boolean {
  const type = normalizeTypeName(dataType);
  if (type === '') return false;
  if (DEFAULT_ZONED_TYPE_NAMES.some((name) => type.includes(name))) return true;
  return (hints?.zonedTypes ?? []).some((name) => matchesHintName(type, name));
}

/**
 * Whether the driver declared this column type as not writable in place.
 *
 * Only the driver can answer this (SQL Server's `rowversion` is a row
 * version, not a timestamp), so there is no shared default.
 */
export function isReadOnlyCellType(
  dataType: string | undefined | null,
  hints?: CellTypeHints,
): boolean {
  const type = normalizeTypeName(dataType);
  if (type === '') return false;
  return (hints?.readOnlyTypes ?? []).some((name) => matchesHintName(type, name));
}

/**
 * Whether the column holds whole numbers.
 *
 * The editor needs it to reject `1.5` for an integer column. The answer
 * comes from the type **name**, because `CellType` folds integers and
 * decimals into one `number`.
 */
export function isIntegerCellType(
  dataType: string | undefined | null,
  hints?: CellTypeHints,
): boolean {
  const type = normalizeTypeName(dataType);
  if (type === '') return false;
  if (resolveCellType(type, hints) !== 'number') return false;
  return tokenizeTypeName(type).some((token) => INTEGER_TYPE_TOKENS.has(token));
}
