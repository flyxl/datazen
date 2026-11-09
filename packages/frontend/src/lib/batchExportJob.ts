import type { ExportTablesRequest } from '../commands/file';
import { requireTablesExporter } from '../platform/contracts';

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
  /** Pre-formatted data file body from a streaming export (skips generateExport). */
  streamedData?: string;
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
