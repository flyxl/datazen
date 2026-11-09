/**
 * Frontend row formatter for provided-only export (frontend-DataFetcher plan §2.4).
 *
 * Successor of the retired `generateExport` in `exportData.ts` (as of `e9ace8d`):
 * data already loaded in the frontend is formatted here, in memory, with no
 * backend read, no session and no DTO. The SQL side formats through the Rust
 * `StreamFormatter`/`BinaryFormatter` instead; the two implementations are
 * kept in sync by parity tests (`__tests__/exportFormat.test.ts`), not by
 * shared code — XLSX bytes differ by design (SheetJS vs rust_xlsxwriter),
 * so XLSX parity is structural (read the workbook back, compare content).
 *
 * Shared escaping kernels live where they always did and are reused, not
 * re-implemented: CSV field escaping and the Markdown table serializer in
 * `gridClipboard.ts`. SQL identifier quoting and literal rendering are the
 * one exception — they live here, as a static per-dialect table, and must
 * NOT be imported from `databaseTypes.ts`: this module has to stay a leaf.
 * `blobTablesExporter` (re-exported from the platform-contracts entry,
 * which the test setup imports) uses it, and anything that reaches
 * `extensions/generated` from setup defeats test-file mocks of the driver
 * set and pre-evaluates `@codemirror/view` (its `browser.mac` is read at
 * module load) before macOS platform stubs run. `databaseTypes.ts` builds
 * `DB_REGISTRY` from the generated driver metas and calls into
 * `@datazen/extension-points` (whose index re-exports the sql-editor graph,
 * hence CodeMirror), so importing it here would drag all of that into
 * setup. The table below mirrors the driver metas
 * (`crates/drivers/<id>/ui/meta.ts`: `quoteChar`, `escapeSqlValue`) and is
 * pinned against `DB_REGISTRY` by the registry test in
 * `__tests__/exportFormat.test.ts` — a meta change that moves a quote char
 * or escaper fails that test instead of silently drifting. File extensions
 * and per-table filenames come from `exportPlan.ts` (`formatExtension`,
 * `{table}.{ext}` — the `build_file_plans` data-only rule).
 */

import * as XLSX from 'xlsx';
import type { ColumnInfo } from '../types';
import { formatExtension } from '../platform/exportPlan';
import { escapeCsvField, serializeCells } from './gridClipboard';
import type { ExportFormat } from './exportData';

/** Text formats handled by {@link formatRowsAsText}; XLSX is binary. */
export type ExportTextFormat = Exclude<ExportFormat, 'xlsx'>;

/**
 * One column of already-loaded rows. `ColumnInfo` is the current provided
 * shape; `isPrimaryKey` is the `ColumnSchema` carry-over the SQL UPDATE
 * branch reads — a column list built from either type fits.
 */
export type ExportColumn = ColumnInfo & { isPrimaryKey?: boolean };

/** Multi-row INSERT batch size (TablePlus-style), same as Rust `SQL_INSERT_BATCH_SIZE`. */
export const SQL_INSERT_BATCH_SIZE = 500;

export interface FormatRowsBaseOptions {
  tableName: string;
  columns: ExportColumn[];
  /** Already-loaded rows, positional over `columns`. */
  rows: unknown[][];
  /** Driver id for SQL identifier/literal dialect (`postgresql`, `mysql`, `sqlite`, `sqlserver`, …). */
  databaseType?: string;
  /** Key column for `sql_update`; a column's `isPrimaryKey`, else the first column. */
  primaryKey?: string | null;
}

export interface FormatRowsTextOptions extends FormatRowsBaseOptions {
  format: ExportTextFormat;
}

export interface FormatRowsXlsxOptions extends FormatRowsBaseOptions {
  /** Accepted for call-site symmetry; anything other than `'xlsx'` throws. */
  format?: 'xlsx';
}

export interface TextExportResult {
  content: string;
  extension: string;
  mimeType: string;
}

export interface BinaryExportResult {
  dataBase64: string;
  extension: string;
  mimeType: string;
}

const TEXT_MIME_TYPES: Record<ExportTextFormat, string> = {
  csv: 'text/csv',
  tsv: 'text/tab-separated-values',
  json: 'application/json',
  markdown: 'text/markdown',
  sql_insert: 'text/sql',
  sql_update: 'text/sql',
};

const XLSX_MIME_TYPE =
  'application/vnd.openxmlformats-officedocument.spreadsheetml.sheet';

function columnNames(columns: ExportColumn[]): string[] {
  return columns.map((column) => column.name);
}

function requireColumns(columns: ExportColumn[]): string[] {
  if (columns.length === 0) {
    throw new Error('export needs at least one column');
  }
  return columnNames(columns);
}

/** Cell value at a positional index; a short row reads as `null`, extras are ignored. */
function cellAt(row: unknown[], index: number): unknown {
  return row[index] ?? null;
}

// MySQL-family drivers quote identifiers with backticks and escape string
// literals with backslashes (`mysqlEscapeSqlValue` in `mysql/ui/meta.ts`).
const BACKTICK_DIALECTS: ReadonlySet<string> = new Set([
  'mysql',
  'mariadb',
  'doris',
  'starrocks',
  'manticore',
  'ob_oracle',
]);
// SQL Server brackets identifiers; the no-SQL drivers below leave
// identifiers bare; everything else double-quotes (also the fallback for
// unknown types, matching `databaseTypes.escapeIdent`'s `?? '"'`).
const NO_QUOTE_DIALECTS: ReadonlySet<string> = new Set([
  'elasticsearch',
  'hbase',
  'influxdb',
  'mongodb',
  'redis',
  'vector',
  'victoriametrics',
]);

function sqlQuoteChar(databaseType?: string): '`' | '"' | '[' | '' {
  if (!databaseType) return '"';
  if (BACKTICK_DIALECTS.has(databaseType)) return '`';
  if (databaseType === 'sqlserver') return '[';
  if (NO_QUOTE_DIALECTS.has(databaseType)) return '';
  return '"';
}

function escapeSqlIdent(name: string, databaseType?: string): string {
  const q = sqlQuoteChar(databaseType);
  if (q === '`') return `\`${name.replaceAll('`', '``')}\``;
  if (q === '"') return `"${name.replaceAll('"', '""')}"`;
  if (q === '[') return `[${name.replaceAll(']', ']]')}]`;
  return name;
}

/**
 * Default SQL literal escaper (PostgreSQL-style), as in
 * `databaseTypes.defaultEscapeSqlValue`.
 */
function defaultEscapeSqlLiteral(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
  const str = typeof value === 'object' ? JSON.stringify(value) : String(value);
  return `'${str.replaceAll("'", "''")}'`;
}

/**
 * MySQL-family literal escaper, as in `mysql/ui/meta.ts`: backslash escapes
 * in three passes — `\` → `\\`, `"` → `\"`, `'` → `''`.
 */
function mysqlEscapeSqlLiteral(value: unknown): string {
  if (value === null || value === undefined) return 'NULL';
  if (typeof value === 'number' && Number.isFinite(value)) return String(value);
  if (typeof value === 'boolean') return value ? 'TRUE' : 'FALSE';
  const str = typeof value === 'object' ? JSON.stringify(value) : String(value);
  return `'${str.replaceAll('\\', '\\\\').replaceAll('"', '\\"').replaceAll("'", "''")}'`;
}

function escapeSqlLiteral(value: unknown, databaseType?: string): string {
  if (databaseType && BACKTICK_DIALECTS.has(databaseType)) {
    return mysqlEscapeSqlLiteral(value);
  }
  return defaultEscapeSqlLiteral(value);
}

function sqlBeginTransaction(databaseType?: string): string {
  return databaseType === 'sqlserver' ? 'BEGIN TRANSACTION;' : 'BEGIN;';
}

function sqlCommitTransaction(): string {
  return 'COMMIT;';
}

function formatSqlInsertHeader(
  tableName: string,
  cols: string[],
  databaseType?: string,
): string {
  const colList = cols.map((c) => escapeSqlIdent(c, databaseType)).join(', ');
  return `INSERT INTO ${escapeSqlIdent(tableName, databaseType)} (${colList}) VALUES`;
}

function formatSqlInsertTuple(
  row: unknown[],
  cols: string[],
  databaseType?: string,
): string {
  return `(${cols.map((_, i) => escapeSqlLiteral(cellAt(row, i), databaseType)).join(', ')})`;
}

/** Batched INSERT statements wrapped in a transaction (old `formatSqlInsertScript`). */
function formatSqlInsertScript(
  tableName: string,
  cols: string[],
  rows: unknown[][],
  databaseType?: string,
  batchSize: number = SQL_INSERT_BATCH_SIZE,
): string {
  const lines: string[] = [sqlBeginTransaction(databaseType)];
  if (rows.length === 0) {
    lines.push(sqlCommitTransaction());
    return lines.join('\n');
  }
  const header = formatSqlInsertHeader(tableName, cols, databaseType);
  const size = Math.max(1, batchSize);
  for (let i = 0; i < rows.length; i += size) {
    const chunk = rows.slice(i, i + size);
    const tuples = chunk
      .map((row) => `  ${formatSqlInsertTuple(row, cols, databaseType)}`)
      .join(',\n');
    lines.push(`${header}\n${tuples};`);
  }
  lines.push(sqlCommitTransaction());
  return lines.join('\n');
}

/**
 * Key column for `sql_update`: the caller's `primaryKey` when it is among
 * the exported columns, else the `isPrimaryKey` column, else the first
 * column — the same fallback the Rust `StreamFormatter::update_key` uses.
 * `undefined` when there are no columns at all.
 */
function updateKeyColumn(
  cols: string[],
  columns: ExportColumn[],
  primaryKey?: string | null,
): string | undefined {
  if (primaryKey && cols.includes(primaryKey)) return primaryKey;
  return columns.find((c) => c.isPrimaryKey)?.name ?? cols[0];
}

function formatSqlUpdateScript(
  tableName: string,
  columns: ExportColumn[],
  rows: unknown[][],
  databaseType?: string,
  primaryKey?: string | null,
): string {
  const cols = columnNames(columns);
  // Empty columns are rejected upstream by `requireColumns` in
  // `formatRowsAsText`, so a key column always exists here.
  const pkName = updateKeyColumn(cols, columns, primaryKey) as string;
  const pkIndex = cols.indexOf(pkName);
  const setIndexes = cols.map((_, i) => i).filter((i) => i !== pkIndex);
  if (setIndexes.length === 0) {
    throw new Error('sql_update export needs at least one non-key column to SET');
  }
  const lines: string[] = [sqlBeginTransaction(databaseType)];
  for (const row of rows) {
    const setClauses = setIndexes
      .map(
        (i) =>
          `${escapeSqlIdent(cols[i]!, databaseType)} = ${escapeSqlLiteral(cellAt(row, i), databaseType)}`,
      )
      .join(', ');
    const where = `${escapeSqlIdent(pkName, databaseType)} = ${escapeSqlLiteral(cellAt(row, pkIndex), databaseType)}`;
    lines.push(
      `UPDATE ${escapeSqlIdent(tableName, databaseType)} SET ${setClauses} WHERE ${where};`,
    );
  }
  lines.push(sqlCommitTransaction());
  return lines.join('\n');
}

/**
 * JSON-safe cell: `bigint` cannot go through `JSON.stringify` (and loses
 * precision past 2^53), so a safe bigint stays a number and a big one
 * becomes a string — the same rule the Rust XLSX writer applies to i64.
 * `Uint8Array` becomes a plain number array.
 */
function toJsonValue(value: unknown): unknown {
  if (typeof value === 'bigint') {
    return Number.isSafeInteger(Number(value)) ? Number(value) : value.toString();
  }
  if (value instanceof Uint8Array) return Array.from(value);
  if (Array.isArray(value)) return value.map((item) => toJsonValue(item));
  if (typeof value === 'object' && value !== null && !(value instanceof Date)) {
    const out: Record<string, unknown> = {};
    for (const [key, item] of Object.entries(value as Record<string, unknown>)) {
      out[key] = toJsonValue(item);
    }
    return out;
  }
  return value ?? null;
}

function rowsAsRecords(
  cols: string[],
  rows: unknown[][],
): Record<string, unknown>[] {
  return rows.map((row) => {
    const obj: Record<string, unknown> = {};
    for (let i = 0; i < cols.length; i += 1) {
      obj[cols[i]!] = toJsonValue(cellAt(row, i));
    }
    return obj;
  });
}

function escapeTsvField(value: unknown): string {
  if (value === null || value === undefined) return '';
  const s = typeof value === 'object' ? JSON.stringify(value) : String(value);
  return s.replaceAll('\t', ' ').replaceAll('\n', ' ').replaceAll('\r', '');
}

/**
 * Format already-loaded rows as text. Row values are the `unknown[][]`
 * positional shape over `columns`; a missing cell reads
 * as `null`, an over-long row's extras are ignored.
 */
export function formatRowsAsText(options: FormatRowsTextOptions): TextExportResult {
  const { tableName, columns, rows, format, databaseType, primaryKey } = options;
  const cols = requireColumns(columns);
  const extension = formatExtension(format);
  const mimeType = TEXT_MIME_TYPES[format];

  switch (format) {
    case 'csv': {
      const header = cols.map((col) => escapeCsvField(col)).join(',');
      const body = rows.map((row) =>
        cols.map((_, i) => escapeCsvField(cellAt(row, i))).join(','),
      );
      return { content: [header, ...body].join('\n'), extension, mimeType };
    }

    case 'tsv': {
      const header = cols.map((col) => escapeTsvField(col)).join('\t');
      const body = rows.map((row) =>
        cols.map((_, i) => escapeTsvField(cellAt(row, i))).join('\t'),
      );
      return { content: [header, ...body].join('\n'), extension, mimeType };
    }

    case 'json': {
      // `rowsAsRecords` already normalises bigints, so plain `stringify` is enough.
      return {
        content: JSON.stringify(rowsAsRecords(cols, rows), null, 2),
        extension,
        mimeType,
      };
    }

    case 'markdown': {
      // The one serializer's Markdown branch (DB-10): same escaping as the
      // clipboard's "Copy as Markdown Table" — one kernel, no drift.
      const content = serializeCells(rows, { format: 'markdown', columnNames: cols });
      return { content, extension, mimeType };
    }

    case 'sql_insert': {
      return {
        content: formatSqlInsertScript(tableName, cols, rows, databaseType),
        extension,
        mimeType,
      };
    }

    case 'sql_update': {
      return {
        content: formatSqlUpdateScript(tableName, columns, rows, databaseType, primaryKey),
        extension,
        mimeType,
      };
    }

    default: {
      // Unreachable through the type (`ExportTextFormat` has no `'xlsx'`),
      // but a caller that smuggles one in must fail loudly, not silently
      // return `undefined` past the switch.
      const _exhaustive: never = format;
      throw new Error(`formatRowsAsText cannot format '${String(_exhaustive)}' as text`);
    }
  }
}

/**
 * XLSX cell value: numbers/booleans/strings pass through, `null` stays a
 * blank cell, objects become their JSON text (as the Rust writer does),
 * bytes become bare hex (no `datazen:bytes:hex:` CSV marker in a workbook),
 * and a `Date` stays a date so SheetJS writes a real date cell.
 */
function toXlsxCell(value: unknown): unknown {
  if (value === null || value === undefined) return null;
  if (typeof value === 'bigint') {
    return Number.isSafeInteger(Number(value)) ? Number(value) : value.toString();
  }
  if (value instanceof Uint8Array) {
    return Array.from(value)
      .map((byte) => byte.toString(16).padStart(2, '0'))
      .join('');
  }
  if (value instanceof Date) return value;
  if (typeof value === 'object') return JSON.stringify(value);
  return value;
}

/**
 * Format already-loaded rows as an XLSX workbook (SheetJS), returned as
 * base64 so the caller can hand it to `save_base64_with_dialog` (desktop)
 * or a Blob download (web) without touching the backend.
 */
export function formatRowsAsXlsx(options: FormatRowsXlsxOptions): BinaryExportResult {
  if (options.format !== undefined && options.format !== 'xlsx') {
    throw new Error(`formatRowsAsXlsx only handles 'xlsx', got '${options.format}'`);
  }
  const { columns, rows } = options;
  const cols = requireColumns(columns);
  const sheetRows = rows.map((row) => {
    const obj: Record<string, unknown> = {};
    for (let i = 0; i < cols.length; i += 1) {
      obj[cols[i]!] = toXlsxCell(cellAt(row, i));
    }
    return obj;
  });
  const ws = XLSX.utils.json_to_sheet(sheetRows, { header: cols });
  const wb = XLSX.utils.book_new();
  XLSX.utils.book_append_sheet(wb, ws, 'Sheet1');
  const dataBase64 = XLSX.write(wb, { type: 'base64', bookType: 'xlsx' });
  return {
    dataBase64,
    extension: formatExtension('xlsx'),
    mimeType: XLSX_MIME_TYPE,
  };
}

/**
 * Per-table output filename — `{table}.{ext}`, the `build_file_plans`
 * data-only rule (extension via `exportPlan.formatExtension`, the same
 * `DataFormat::extension` mirror the Rust side uses).
 */
export function buildProvidedExportFilename(
  tableName: string,
  format: ExportFormat,
): string {
  return `${tableName}.${formatExtension(format)}`;
}
