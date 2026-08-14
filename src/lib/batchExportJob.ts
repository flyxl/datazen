import { fileCommands, type ExportTablesRequest } from '../commands/file';

export type BatchExportOutputMode = 'single' | 'zip';

export interface BatchExportProgress {
  current: number;
  total: number;
  tableName: string;
}

export interface RunBatchExportJobOptions {
  tableNames: string[];
  mode: 'structure_only' | 'data_only' | 'data_and_structure';
  dataFormat: 'csv' | 'json' | 'sql_insert';
  outputMode: BatchExportOutputMode;
  databaseType?: string;
  connectionId?: string;
  /** Required loader that returns per-table metadata (DDL + column names). */
  loadTableExportData: (tableName: string) => Promise<{
    tableName: string;
    ddl?: string | null;
    columns: { name: string }[];
    rows?: unknown[];
  }>;
  onProgress?: (progress: BatchExportProgress) => void;
  /** Injectable for tests; defaults to fileCommands.exportTablesStream. */
  exportTables?: (
    request: ExportTablesRequest,
  ) => Promise<{ Saved: number } | { Cancelled: null } | null>;
}

export type RunBatchExportJobResult = { status: 'saved' } | { status: 'cancelled' };

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
    connectionId,
    loadTableExportData,
    onProgress,
    exportTables = fileCommands.exportTablesStream,
  } = options;

  if (tableNames.length === 0) {
    throw new Error('no_tables_selected');
  }
  if (!connectionId) {
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
      columns: meta.columns.map((c) => c.name),
      ddl: meta.ddl ?? undefined,
    });
  }

  const request: ExportTablesRequest = {
    connectionId,
    databaseType: databaseType ?? undefined,
    mode,
    dataFormat,
    outputMode,
    tables,
  };

  const result = await exportTables(request);
  return result && 'Saved' in result ? { status: 'saved' } : { status: 'cancelled' };
}
