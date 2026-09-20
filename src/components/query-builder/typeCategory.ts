/**
 * Classify raw database dataType strings into semantic categories.
 *
 * The QB cannot rely on `isExactNumericLiteral` alone — a date column typed
 * as `date` receives the literal `2025-01-01` (which looks numeric to no one)
 * but also the literal `10` (which *does* look numeric).  Without knowing the
 * column type, `10` is emitted unquoted → `WHERE created_at > 10` → PostgreSQL
 * rejects it with `operator does not exist: date > integer`.
 *
 * Each category maps to a quoting strategy in `formatValue`:
 *   - **numeric**: pure numeric literals unquoted, everything else quoted
 *   - **temporal**: always quoted
 *   - **boolean**: `TRUE` / `FALSE` keywords
 *   - **text / binary / json / unknown**: quoted (the safe default)
 */

export enum TypeCategory {
  Numeric = 'numeric',
  Temporal = 'temporal',
  Boolean = 'boolean',
  Text = 'text',
  Binary = 'binary',
  Json = 'json',
}

// ── Pattern tables ────────────────────────────────────────────

// Strip precision/size suffixes before matching: `varchar(255)` → `varchar`.
const STRIP_PRECISION = /\s*\(.*\)\s*$/;

// Exact matches (after normalization)
const EXACT_NUMERIC = new Set([
  'int',
  'integer',
  'int1',
  'int2',
  'int4',
  'int8',
  'int16',
  'int32',
  'int64',
  'smallint',
  'mediumint',
  'bigint',
  'tinyint',
  'serial',
  'smallserial',
  'bigserial',
  'decimal',
  'numeric',
  'float',
  'float4',
  'float8',
  'real',
  'double precision',
  'double',
  'number',
  'money',
  'smallmoney',
  'fixed',
]);

const EXACT_TEMPORAL = new Set([
  'date',
  'time',
  'timetz',
  'datetime',
  'datetime2',
  'datetimeoffset',
  'smalldatetime',
  'timestamp',
  'timestamptz',
  'timestamp without time zone',
  'timestamp with time zone',
  'time without time zone',
  'time with time zone',
]);

const EXACT_BOOLEAN = new Set(['boolean', 'bool']);

const EXACT_TEXT = new Set([
  'varchar',
  'varchar2',
  'character varying',
  'char',
  'character',
  'text',
  'ntext',
  'nvarchar',
  'national character varying',
  'national char',
  'nchar',
  'clob',
  'longtext',
  'mediumtext',
  'tinytext',
  'enum',
  'set',
  'uuid',
  'xml',
  'citext',
  'name',
  'bpchar',
]);

const EXACT_BINARY = new Set([
  'blob',
  'longblob',
  'mediumblob',
  'tinyblob',
  'bytea',
  'binary',
  'varbinary',
  'varbinary(max)',
  'image',
  'oid',
]);

const EXACT_JSON = new Set(['json', 'jsonb']);

// ── Suffix heuristics (for types not in exact sets) ───────────

const NUMERIC_SUFFIXES = ['int', 'serial', 'float', 'double', 'decimal', 'numeric'];
const TEXT_SUFFIXES = ['char', 'text', 'string', 'varchar', 'clob'];
const TEMPORAL_SUFFIXES = ['date', 'time', 'stamp'];

// ── Public API ────────────────────────────────────────────────

/**
 * Map a raw `dataType` string (as returned by the database driver) to a
 * semantic {@link TypeCategory}.  The input is lowercased and
 * precision/size suffixes are stripped before matching.
 */
export function classifyColumnType(rawType: string): TypeCategory {
  const normalized = rawType.toLowerCase().trim().replace(STRIP_PRECISION, '');

  if (!normalized) return TypeCategory.Text;

  // Exact set lookups (fast path)
  if (EXACT_NUMERIC.has(normalized)) return TypeCategory.Numeric;
  if (EXACT_TEMPORAL.has(normalized)) return TypeCategory.Temporal;
  if (EXACT_BOOLEAN.has(normalized)) return TypeCategory.Boolean;
  if (EXACT_TEXT.has(normalized)) return TypeCategory.Text;
  if (EXACT_BINARY.has(normalized)) return TypeCategory.Binary;
  if (EXACT_JSON.has(normalized)) return TypeCategory.Json;

  // Suffix heuristics for types not in the exact sets
  for (const suffix of NUMERIC_SUFFIXES) {
    if (normalized.endsWith(suffix)) return TypeCategory.Numeric;
  }
  for (const suffix of TEMPORAL_SUFFIXES) {
    if (normalized.endsWith(suffix)) return TypeCategory.Temporal;
  }
  for (const suffix of TEXT_SUFFIXES) {
    if (normalized.endsWith(suffix)) return TypeCategory.Text;
  }

  return TypeCategory.Text;
}
