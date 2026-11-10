import { SqlDataFetcher } from './dataFetcher';
import type { ExportProgressEvent } from '../commands/file';
import type { ExportTablesRequest, ExportTablesResult } from '../commands/file';
import { requireTablesExporter } from '../platform/contracts';

export type BatchExportOutputMode = 'single' | 'zip';

export type BatchExportMode = 'data_only' | 'structure_only' | 'data_and_structure';

/** The honest end states of a Rust-side export run (DBR-06, restored): */
export type SingleTableExportResult =
  | { status: 'saved' }
  | { status: 'cancelled' }
  | { status: 'aborted'; partialDeleted: boolean };
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
  exportTables?: (request: ExportTablesRequest) => Promise<ExportTablesResult | null>;
  /**
   * Injectable cancel for tests; defaults to the tables-exporter port's
   * `cancelExportTables` (desktop). A form without one (web) has no
   * cancellation to offer, so the handle is a no-op there.
   */
  cancelExport?: (token: string) => Promise<boolean>;
  /**
   * Receives the cancel function for the run, before any table is
   * exported. The batch dialog wires its "Cancel export" button to it.
   */
  onCancelHandle?: (cancel: () => void) => void;
}

export type RunBatchExportJobResult = SingleTableExportResult;

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
  exportTables?: (request: ExportTablesRequest) => Promise<ExportTablesResult | null>;
  /**
   * Injectable cancel for tests; defaults to the tables-exporter port.
   * Threaded through to the `SqlDataFetcher` so a test that injects
   * `exportTables` can observe the cancel call without binding a platform.
   */
  cancelExport?: (token: string) => Promise<boolean>;
  /**
   * Injectable for tests; defaults to `onExportProgress`
   * (`batch-export-progress`). Threaded through to the `SqlDataFetcher`
   * so a test that injects `exportTables` need not bind a platform.
   */
  subscribeProgress?: (
    handler: (event: ExportProgressEvent) => void,
  ) => Promise<() => void>;
  /**
   * Receives the cancel function for the run, before the export starts
   * (the fetcher mints its token at construction). The dialogs wire
   * their "Cancel export" button to it. Omitted callers get no handle.
   */
  onCancelHandle?: (cancel: () => void) => void;
}

/**
 * Export one table's entire / filtered rows through the Rust batch exporter
 * (`export_tables_stream`, single-table request). This is the converged
 * path for the old frontend streaming export (item 3b): the frontend no
 * longer streams query events into its own text formatter and save session
 * — the backend streams, formats (including XLSX) and opens the save dialog.
 *
 * Implemented through `SqlDataFetcher` (frontend-DataFetcher plan §2.3,
 * task 3): the request, the progress subscription and the result mapping
 * are the fetcher's; this function stays as the shared entry point for the
 * dialogs and the structure export so their call sites do not change.
 */
export async function exportSingleTableViaRust(
  options: SingleTableExportOptions,
): Promise<SingleTableExportResult> {
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
    exportTables,
    cancelExport,
    subscribeProgress,
    onCancelHandle,
  } = options;

  const fetcher = new SqlDataFetcher(
    {
      tableName,
      scope: sql ? 'filtered_table' : 'entire_table',
      dbSessionId,
      databaseType,
      database: database ?? undefined,
      schema: schema ?? undefined,
      sql,
      mode: 'data_only',
      dataFormat,
      outputMode: 'single',
      columnNames: columns,
      primaryKey: primaryKey ?? undefined,
    },
    {
      ...(exportTables ? { exportTables } : {}),
      ...(cancelExport ? { cancelExport } : {}),
      ...(subscribeProgress ? { subscribeProgress } : {}),
    },
  );
  onCancelHandle?.(() => fetcher.cancel());
  const result = await fetcher.fetch();
  if (result === 'aborted') {
    // The fetcher records the partial-file outcome from the `Aborted`
    // result; unknown (a test double that never set it) reports "kept"
    // rather than claiming a deletion.
    return { status: 'aborted', partialDeleted: fetcher.abortedPartialDeleted ?? false };
  }
  return { status: result };
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
  exportTables?: (request: ExportTablesRequest) => Promise<ExportTablesResult | null>;
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
 * Export one or more tables by streaming each query to disk entirely on the
 * Rust backend (which opens the native save dialog and, for multi-file output,
 * writes temp files and zips them). JS only gathers per-table metadata and
 * sends the request — it never buffers whole tables in memory.
 *
 * NOTE (frontend-DataFetcher plan task 3): this keeps one combined
 * multi-table `ExportTablesRequest` instead of one `SqlDataFetcher.fetch()`
 * per table — a per-table fetch would open one save dialog / zip per table
 * and break the multi-table zip, which must stay a single Rust-side request.
 * The batch path still rides the same tables-exporter port
 * (`requireTablesExporter().exportTables`) as `SqlDataFetcher`.
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
    cancelExport = async (token: string) =>
      (await requireTablesExporter().cancelExportTables?.(token)) ?? false,
    onCancelHandle,
  } = options;

  if (tableNames.length === 0) {
    throw new Error('no_tables_selected');
  }
  if (!dbSessionId) {
    throw new Error('Missing connection');
  }

  // One token for the whole batch: the batch is a single combined Rust
  // request, so cancelling it interrupts the table in flight and stops
  // the batch. The handle goes out before any table is loaded, so a
  // cancel during metadata loading stops the batch without ever calling
  // the exporter (nothing was written, so there is no partial file).
  const cancelToken = crypto.randomUUID();
  let cancelRequested = false;
  const cancel = () => {
    if (cancelRequested) return;
    cancelRequested = true;
    void cancelExport(cancelToken).catch(() => undefined);
  };
  onCancelHandle?.(cancel);

  const tables = [];
  const total = tableNames.length;
  for (let i = 0; i < tableNames.length; i += 1) {
    if (cancelRequested) {
      return { status: 'aborted', partialDeleted: true };
    }
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

  // A cancel that lands during the last table's metadata load has no
  // further loop iteration to catch it — check again before the
  // exporter is called, so the batch never starts after a cancel.
  if (cancelRequested) {
    return { status: 'aborted', partialDeleted: true };
  }

  const request: ExportTablesRequest = {
    dbSessionId,
    cancelToken,
    databaseType: databaseType ?? undefined,
    database: database ?? undefined,
    schema: schema ?? undefined,
    mode,
    dataFormat,
    outputMode,
    tables,
  };

  const result = await exportTables(request);
  if (result && 'Aborted' in result) {
    return { status: 'aborted', partialDeleted: result.Aborted.partialDeleted };
  }
  return result && 'Saved' in result ? { status: 'saved' } : { status: 'cancelled' };
}
