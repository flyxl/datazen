import type { ExportTablesRequest } from '../commands/file';
import { requireTablesExporter } from '../platform/contracts';
import type { ColumnInfo, Value } from '../types';

export type BatchExportOutputMode = 'single' | 'zip';

export type BatchExportMode = 'data_only' | 'structure_only' | 'data_and_structure';
export type BatchExportDataFormat =
  | 'csv'
  | 'tsv'
  | 'json'
  | 'markdown'
  | 'sql_insert'
  | 'sql_update'
  | 'xlsx';

export interface BatchExportTableInput {
  tableName: string;
  /** Schema the table lives in (PostgreSQL family). */
  schema?: string | null;
  /** DDL text; required when mode includes structure */
  ddl?: string | null;
  columns: { name: string }[];
  rows: Record<string, unknown>[];
}

export interface BatchExportProgress {
  current: number;
  total: number;
  tableName: string;
}

export interface RunBatchExportJobOptions {
  tableNames: string[];
  mode: BatchExportMode;
  dataFormat: BatchExportDataFormat;
  outputMode: BatchExportOutputMode;
  databaseType?: string;
  dbSessionId?: string;
  /** Database to read from; the session's own database is only a fallback. */
  database?: string | null;
  /** Fallback schema for tables that do not carry one. */
  schema?: string | null;
  /** Required loader that returns per-table metadata (DDL + column names). */
  loadTableExportData: (tableName: string) => Promise<{
    tableName: string;
    schema?: string | null;
    ddl?: string | null;
    columns: { name: string }[];
    rows?: unknown[];
  }>;
  onProgress?: (progress: BatchExportProgress) => void;
  /**
   * Injectable for tests; defaults to the bound form's exporter (the F4
   * dual-flow port — desktop streams via the save-dialog token writer, a web
   * build Blob-downloads under the same signature).
   */
  exportTables?: (
    request: ExportTablesRequest,
  ) => Promise<{ Saved: number } | { Cancelled: null } | null>;
}

export type RunBatchExportJobResult = { status: 'saved' } | { status: 'cancelled' };

export interface SingleTableExportOptions {
  dbSessionId: string;
  databaseType?: string;
  /** Database to read from; the session's own database is only a fallback. */
  database?: string | null;
  /** Fallback schema for the table (PostgreSQL family). */
  schema?: string | null;
  tableName: string;
  /** Columns to export (empty => all). */
  columns: string[];
  dataFormat: BatchExportDataFormat;
  /**
   * The statement to stream, when the caller has one the product built
   * (the filtered full-table export streams the backend's own
   * `previewTableReadSql` statement). Absent, the plain table source runs.
   */
  sql?: string;
  /** Key column for `sql_update` output; first column when omitted. */
  primaryKey?: string | null;
  /**
   * Injectable for tests; defaults to the bound form's exporter (the same
   * `export_tables_stream` the batch job uses).
   */
  exportTables?: (
    request: ExportTablesRequest,
  ) => Promise<{ Saved: number } | { Cancelled: null } | null>;
}

/**
 * Export one table's entire / filtered rows through the Rust batch exporter
 * (`export_tables_stream`, single-table request). This is the converged
 * path for the old frontend streaming export (item 3b): the frontend no
 * longer streams query events into its own text formatter and save session
 * — the backend streams, formats (including XLSX) and opens the save dialog.
 */
export async function exportSingleTableViaRust(
  options: SingleTableExportOptions,
): Promise<'saved' | 'cancelled'> {
  const {
    dbSessionId,
    databaseType,
    database,
    schema,
    tableName,
    columns,
    dataFormat,
    sql,
    primaryKey,
    exportTables = (request: ExportTablesRequest) =>
      requireTablesExporter().exportTables(request),
  } = options;

  if (!dbSessionId) {
    throw new Error('Missing connection');
  }

  const request: ExportTablesRequest = {
    dbSessionId,
    databaseType: databaseType ?? undefined,
    database: database ?? undefined,
    schema: schema ?? undefined,
    mode: 'data_only',
    dataFormat,
    outputMode: 'single',
    tables: [
      {
        tableName,
        schema: schema ?? undefined,
        columns,
        source: sql ? { type: 'sql', sql } : { type: 'table' },
        primaryKey: primaryKey ?? undefined,
      },
    ],
  };

  const result = await exportTables(request);
  return result && 'Saved' in result ? 'saved' : 'cancelled';
}

/**
 * Export one table's DDL only through the Rust batch exporter
 * (`export_tables_stream`, `structure_only` single-table request). This is
 * the converged path for the old structure-editor DDL export (item 6): the
 * caller fetches the DDL text first (the backend never invents DDL — a
 * missing/blank `ddl` falls back to the backend's `-- DDL unavailable …`
 * placeholder, exactly as the old direct save did), and the backend opens
 * the native save dialog and writes the file.
 */
export interface SingleTableStructureExportOptions {
  dbSessionId: string;
  databaseType?: string;
  /** Database to read from; the session's own database is only a fallback. */
  database?: string | null;
  /** Fallback schema for the table (PostgreSQL family). */
  schema?: string | null;
  tableName: string;
  /** DDL text the caller already fetched; blank/missing => backend placeholder. */
  ddl?: string | null;
  /**
   * Injectable for tests; defaults to the bound form's exporter (the same
   * `export_tables_stream` the batch job uses).
   */
  exportTables?: (
    request: ExportTablesRequest,
  ) => Promise<{ Saved: number } | { Cancelled: null } | null>;
}

export async function exportSingleTableStructureViaRust(
  options: SingleTableStructureExportOptions,
): Promise<'saved' | 'cancelled'> {
  const {
    dbSessionId,
    databaseType,
    database,
    schema,
    tableName,
    ddl,
    exportTables = (request: ExportTablesRequest) =>
      requireTablesExporter().exportTables(request),
  } = options;

  if (!dbSessionId) {
    throw new Error('Missing connection');
  }

  const request: ExportTablesRequest = {
    dbSessionId,
    databaseType: databaseType ?? undefined,
    database: database ?? undefined,
    schema: schema ?? undefined,
    mode: 'structure_only',
    // The planner never consults the format for `structure_only`; sql_insert
    // is chosen so the native save dialog offers a `.sql` extension and
    // default name (the old G save wrote `{table}.sql`).
    dataFormat: 'sql_insert',
    outputMode: 'single',
    tables: [
      {
        tableName,
        schema: schema ?? undefined,
        columns: [],
        ddl: ddl ?? undefined,
      },
    ],
  };

  const result = await exportTables(request);
  return result && 'Saved' in result ? 'saved' : 'cancelled';
}

/**
 * Map one loaded grid value onto the wire `Value` shape the Rust
 * `Provided` source deserializes (untagged `Value`): null/boolean/number/
 * string pass through (an integer-looking JSON number becomes `Integer`,
 * a fractional one `Float`); a `Uint8Array` becomes a number array
 * (`Bytes`); arrays and plain objects pass through as `Bytes`/`Json`
 * candidates — the same number-array ambiguity query results already
 * carry. Non-finite numbers have no JSON form and become null.
 */
export function toExportWireValue(value: unknown): Value | null {
  if (value === null || value === undefined) return null;
  if (typeof value === 'boolean' || typeof value === 'string') return value;
  if (typeof value === 'number') return Number.isFinite(value) ? value : null;
  if (typeof value === 'bigint') {
    const asNumber = Number(value);
    return Number.isSafeInteger(asNumber) ? asNumber : value.toString();
  }
  if (value instanceof Uint8Array) return Array.from(value);
  if (Array.isArray(value)) return value as unknown[];
  if (typeof value === 'object') return value as Record<string, unknown>;
  return String(value);
}

export interface ProvidedRowsExportOptions {
  dbSessionId: string;
  databaseType?: string;
  /** Database to read from; the session's own database is only a fallback. */
  database?: string | null;
  /** Fallback schema for the table (PostgreSQL family). */
  schema?: string | null;
  tableName: string;
  /** Full schema of the provided rows (names, types, nullability). */
  columns: ColumnInfo[];
  /** Already-loaded rows, positional over `columns` (current page or the
   * user's selection — the caller scopes them; both shapes share this one
   * provided-data source, item 3c). Values are mapped with
   * {@link toExportWireValue}. */
  rows: unknown[][];
  dataFormat: BatchExportDataFormat;
  /** Key column for `sql_update` output; first column when omitted. */
  primaryKey?: string | null;
  /**
   * Injectable for tests; defaults to the bound form's exporter (the same
   * `export_tables_stream` the batch job uses).
   */
  exportTables?: (
    request: ExportTablesRequest,
  ) => Promise<{ Saved: number } | { Cancelled: null } | null>;
}

/**
 * Export already-loaded rows (current page / selected rows) through the
 * Rust batch exporter (item 3c): the rows + schema travel as a `provided`
 * source, so the backend never re-queries and the selected rows are never
 * translated back into SQL (a primary key cannot always reconstruct them).
 * This is the converged path for the old in-memory export (A).
 */
export async function exportProvidedRowsViaRust(
  options: ProvidedRowsExportOptions,
): Promise<'saved' | 'cancelled'> {
  const {
    dbSessionId,
    databaseType,
    database,
    schema,
    tableName,
    columns,
    rows,
    dataFormat,
    primaryKey,
    exportTables = (request: ExportTablesRequest) =>
      requireTablesExporter().exportTables(request),
  } = options;

  if (!dbSessionId) {
    throw new Error('Missing connection');
  }

  const request: ExportTablesRequest = {
    dbSessionId,
    databaseType: databaseType ?? undefined,
    database: database ?? undefined,
    schema: schema ?? undefined,
    mode: 'data_only',
    dataFormat,
    outputMode: 'single',
    tables: [
      {
        tableName,
        schema: schema ?? undefined,
        columns: columns.map((c) => c.name),
        source: {
          type: 'provided',
          columns,
          rows: rows.map((row) => row.map((cell) => toExportWireValue(cell))),
        },
        primaryKey: primaryKey ?? undefined,
      },
    ],
  };

  const result = await exportTables(request);
  return result && 'Saved' in result ? 'saved' : 'cancelled';
}

/**
 * Export one or more tables by streaming each query to disk entirely on the
 * Rust backend (which opens the native save dialog and, for multi-file output,
 * writes temp files and zips them). JS only gathers per-table metadata and
 * sends the request — it never buffers whole tables in memory.
 */
export async function runBatchExportJob(
  options: RunBatchExportJobOptions,
): Promise<RunBatchExportJobResult> {
  const {
    tableNames,
    mode,
    dataFormat,
    outputMode,
    databaseType,
    dbSessionId,
    database,
    schema,
    loadTableExportData,
    onProgress,
    exportTables = (request: ExportTablesRequest) => requireTablesExporter().exportTables(request),
  } = options;

  if (tableNames.length === 0) {
    throw new Error('no_tables_selected');
  }
  if (!dbSessionId) {
    throw new Error('Missing connection');
  }

  const tables = [];
  const total = tableNames.length;
  for (let i = 0; i < tableNames.length; i += 1) {
    const tableName = tableNames[i]!;
    onProgress?.({ current: i + 1, total, tableName });
    const meta = await loadTableExportData(tableName);
    tables.push({
      tableName: meta.tableName ?? tableName,
      schema: meta.schema ?? undefined,
      columns: meta.columns.map((c) => c.name),
      ddl: meta.ddl ?? undefined,
    });
  }

  const request: ExportTablesRequest = {
    dbSessionId,
    databaseType: databaseType ?? undefined,
    database: database ?? undefined,
    schema: schema ?? undefined,
    mode,
    dataFormat,
    outputMode,
    tables,
  };

  const result = await exportTables(request);
  return result && 'Saved' in result ? { status: 'saved' } : { status: 'cancelled' };
}
