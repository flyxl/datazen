import type { DatabaseType } from '../types';
import { escapeIdent, getEscapeSqlValue } from './databaseTypes';

function ident(name: string, databaseType?: string): string {
  return escapeIdent(name, databaseType as DatabaseType | undefined);
}
import { queryCommands } from '../commands/query';

/**
 * Import execution (DBR-01).
 *
 * The old path concatenated every row into one giant multi-statement SQL
 * string and issued a single `executeQuery` — no batching, no progress, no
 * cancel, and a failure with an ambiguous committed prefix. This module runs
 * the import as bounded batches instead:
 *
 * - A **batch** is ONE multi-row `INSERT … VALUES (…), (…), …` statement, so
 *   it is atomic per statement: on failure the batch inserted zero rows, and
 *   the committed count stays exact. Batches close at the row cap or the SQL
 *   byte cap, whichever comes first.
 * - In **transaction mode** several batches travel as one payload wrapped in
 *   `BEGIN; … COMMIT;`. The stream executor runs a whole payload on a single
 *   connection, so the wrapper is a real transaction; a failing statement
 *   never reaches `COMMIT`, so the whole chunk rolls back.
 * - In **autocommit mode** each batch is its own payload; a failure stops the
 *   world only when the strategy says so, and committed batches stay
 *   committed (reported honestly, never shown as "rolled back").
 * - **Skip strategy** retries a failed batch/chunk row by row: single-row
 *   INSERTs isolate the offender and attribute the error to its source line.
 * - **Cancel** stops issuing new chunks and best-effort cancels the in-flight
 *   stream; the result reports exactly how many rows had been committed.
 */

export interface ImportColumnPair {
  source: string;
  target: string;
}

export interface ImportSqlOptions {
  table: string;
  targetColumns: readonly string[];
  databaseType?: string;
}

/** Build ONE multi-row INSERT statement for a row range. */
export function buildMultiRowInsertSql(
  options: ImportSqlOptions & {
    rows: readonly Record<string, unknown>[];
    rowStart: number;
    rowCount: number;
  },
): string {
  const { table, targetColumns, databaseType, rows, rowStart, rowCount } = options;
  const escape = getEscapeSqlValue(databaseType);
  const columnList = targetColumns.map((column) => ident(column, databaseType)).join(', ');
  const tuples: string[] = [];
  for (let index = rowStart; index < rowStart + rowCount; index++) {
    const row = rows[index];
    const values = targetColumns.map((column) => escape(row[column])).join(', ');
    tuples.push(`(${values})`);
  }
  return `INSERT INTO ${ident(table, databaseType)} (${columnList}) VALUES\n  ${tuples.join(',\n  ')};`;
}

export interface ImportBatch {
  rowStart: number;
  rowCount: number;
  /** The standalone INSERT statement for this batch. */
  sql: string;
}

export interface ImportChunk {
  batches: ImportBatch[];
  /** The payload actually sent: batches joined, wrapped in BEGIN/COMMIT in
   * transaction mode. */
  sql: string;
  rowStart: number;
  rowCount: number;
}

export interface PlanImportChunksOptions extends ImportSqlOptions {
  rows: readonly Record<string, unknown>[];
  columnPairs: readonly ImportColumnPair[];
  commitMode: ImportCommitMode;
  maxRowsPerBatch?: number;
  maxSqlBytes?: number;
  maxBatchesPerChunk?: number;
}

export type ImportCommitMode = 'transaction' | 'autocommit';
export type ImportErrorStrategy = 'stop' | 'skip';

export const IMPORT_DEFAULTS = {
  maxRowsPerBatch: 500,
  maxSqlBytes: 512 * 1024,
  maxBatchesPerChunk: 8,
} as const;

/**
 * Pure planning: group rows into row- and byte-capped batches, then group
 * batches into chunks. In autocommit mode every batch is its own chunk (one
 * statement per payload, committed on completion); in transaction mode up to
 * `maxBatchesPerChunk` batches share one `BEGIN; … COMMIT;` payload.
 */
export function planImportChunks(options: PlanImportChunksOptions): ImportChunk[] {
  const {
    table,
    targetColumns,
    databaseType,
    rows,
    columnPairs,
    commitMode,
    maxRowsPerBatch = IMPORT_DEFAULTS.maxRowsPerBatch,
    maxSqlBytes = IMPORT_DEFAULTS.maxSqlBytes,
    maxBatchesPerChunk = IMPORT_DEFAULTS.maxBatchesPerChunk,
  } = options;

  // The imported columns, in mapping order — a row is projected onto exactly
  // these targets before it becomes SQL.
  const projected: Record<string, unknown>[] = rows.map((row) => {
    const out: Record<string, unknown> = {};
    for (const pair of columnPairs) out[pair.target] = row[pair.source];
    return out;
  });

  const escape = getEscapeSqlValue(databaseType);
  const columnList = targetColumns.map((column) => ident(column, databaseType)).join(', ');
  const prefixLength =
    `INSERT INTO ${ident(table, databaseType)} (${columnList}) VALUES\n  `.length;

  const batches: ImportBatch[] = [];
  let batchStart = 0;
  while (batchStart < projected.length) {
    // Incremental tuple building: appending a row is O(row), never a rebuild
    // of the whole statement (a rebuild per candidate row would be quadratic).
    const tuples: string[] = [];
    let bytes = prefixLength;
    let rowCount = 0;
    while (batchStart + rowCount < projected.length) {
      const row = projected[batchStart + rowCount];
      const values = targetColumns.map((column) => escape(row[column])).join(', ');
      const tuple = `(${values})`;
      const addedBytes = tuple.length + (tuples.length > 0 ? 2 : 0);
      if (rowCount > 0 && (rowCount + 1 > maxRowsPerBatch || bytes + addedBytes > maxSqlBytes)) {
        break;
      }
      tuples.push(tuple);
      bytes += addedBytes;
      rowCount += 1;
    }
    const sql = `INSERT INTO ${ident(table, databaseType)} (${columnList}) VALUES\n  ${tuples.join(',\n  ')};`;
    batches.push({ rowStart: batchStart, rowCount, sql });
    batchStart += rowCount;
  }

  if (commitMode === 'autocommit') {
    return batches.map((batch) => ({
      batches: [batch],
      sql: batch.sql,
      rowStart: batch.rowStart,
      rowCount: batch.rowCount,
    }));
  }

  const chunks: ImportChunk[] = [];
  for (let index = 0; index < batches.length; index += maxBatchesPerChunk) {
    const group = batches.slice(index, index + maxBatchesPerChunk);
    const sql = `BEGIN;\n${group.map((batch) => batch.sql).join('\n')}\nCOMMIT;`;
    chunks.push({
      batches: group,
      sql,
      rowStart: group[0].rowStart,
      rowCount: group.reduce((sum, batch) => sum + batch.rowCount, 0),
    });
  }
  return chunks;
}

export interface ImportProgress {
  status: 'running' | 'completed' | 'stopped' | 'cancelled';
  processedRows: number;
  committedRows: number;
  failedRows: number;
}

export interface ImportRowFailure {
  rowIndex: number;
  /** Physical source line (CSV) or 0/absent when the format has none. */
  sourceLine: number | null;
  message: string;
}

export interface ImportRunResult {
  status: 'completed' | 'stopped' | 'cancelled';
  committedRows: number;
  failedRows: ImportRowFailure[];
  /** The failure that stopped a `stop`-strategy run. */
  stoppedBy: ImportRowFailure | null;
}

/** The execution seam: one payload per call, cancellable. */
export interface ImportTransport {
  /** Run one SQL payload. Resolves with total affected rows when the stream
   * completes; rejects when any statement fails or the run is cancelled. */
  runStream: (sql: string) => Promise<number>;
  /** Best-effort cancel of the in-flight `runStream`. */
  cancelInFlight: () => Promise<void>;
}

/** The production transport over the cancellable stream command. */
export function createIpcImportTransport(options: {
  dbSessionId: string;
  database?: string | null;
  schema?: string | null;
}): ImportTransport {
  const { dbSessionId, database, schema } = options;
  let executionId: string | null = null;
  return {
    runStream: (sql) =>
      new Promise<number>((resolve, reject) => {
        let totalAffected = 0;
        void queryCommands
          .executeQueryStream(
            dbSessionId,
            sql,
            (event) => {
              if (event.type === 'executionStarted') executionId = event.executionId;
              if (event.type === 'statementEnd' && event.rowsAffected) {
                totalAffected += event.rowsAffected;
              }
            },
            {
              applyResultLimit: false,
              recordHistory: false,
              database,
              schema,
            },
          )
          .then(() => resolve(totalAffected), reject);
      }),
    cancelInFlight: async () => {
      const current = executionId;
      if (!current) return;
      try {
        await queryCommands.cancelQuery(dbSessionId, current);
      } catch {
        // Best-effort: the stream may already have completed.
      }
    },
  };
}

export interface ExecuteImportOptions extends PlanImportChunksOptions {
  rowOrigins: readonly number[];
  onError: ImportErrorStrategy;
  /** Checked between chunks; the dialog also cancels the in-flight stream. */
  signal: { aborted: boolean };
  transport: ImportTransport;
  onProgress?: (progress: ImportProgress) => void;
}

function errorMessage(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

function failureFor(
  rowIndex: number,
  rowOrigins: readonly number[],
  error: unknown,
): ImportRowFailure {
  const origin = rowOrigins[rowIndex] ?? 0;
  return {
    rowIndex,
    sourceLine: origin > 0 ? origin : null,
    message: errorMessage(error),
  };
}

/**
 * Run the planned chunks. Never throws: every outcome — including errors and
 * cancellation — is a value the wizard can render, because the user must see
 * what was committed regardless of how the run ended.
 */
export async function executeImport(options: ExecuteImportOptions): Promise<ImportRunResult> {
  const { rowOrigins, onError, signal, transport, onProgress } = options;
  const chunks = planImportChunks(options);

  let committedRows = 0;
  let processedRows = 0;
  const failedRows: ImportRowFailure[] = [];
  const report = (status: ImportProgress['status']): void => {
    onProgress?.({
      status,
      processedRows,
      committedRows,
      failedRows: failedRows.length,
    });
  };

  const runOneRow = async (rowIndex: number): Promise<boolean> => {
    const single = planImportChunks({
      ...options,
      rows: options.rows.slice(rowIndex, rowIndex + 1),
      commitMode: 'autocommit',
    });
    const sql = single[0]?.sql;
    if (sql == null) return false;
    try {
      await transport.runStream(sql);
      committedRows += 1;
      return true;
    } catch (error) {
      failedRows.push(failureFor(rowIndex, rowOrigins, error));
      return false;
    }
  };

  for (const chunk of chunks) {
    if (signal.aborted) {
      report('cancelled');
      return { status: 'cancelled', committedRows, failedRows, stoppedBy: null };
    }
    try {
      await transport.runStream(chunk.sql);
      committedRows += chunk.rowCount;
      processedRows += chunk.rowCount;
      report('running');
    } catch (error) {
      // A rejection after the user asked to cancel is a cancellation, even
      // when the driver words it as a plain failure.
      if (signal.aborted) {
        report('cancelled');
        return { status: 'cancelled', committedRows, failedRows, stoppedBy: null };
      }
      if (onError === 'stop') {
        const stoppedBy = failureFor(chunk.rowStart, rowOrigins, error);
        processedRows += chunk.rowCount;
        report('stopped');
        return { status: 'stopped', committedRows, failedRows, stoppedBy };
      }
      // Skip strategy: the failed chunk rolled back as a whole (transaction)
      // or the failed batch inserted nothing (single-statement atomicity), so
      // every row in it can be retried one-by-one without double inserts.
      for (let rowIndex = chunk.rowStart; rowIndex < chunk.rowStart + chunk.rowCount; rowIndex++) {
        if (signal.aborted) break;
        await runOneRow(rowIndex);
        processedRows += 1;
      }
      if (signal.aborted) {
        report('cancelled');
        return { status: 'cancelled', committedRows, failedRows, stoppedBy: null };
      }
      report('running');
    }
  }

  report('completed');
  return { status: 'completed', committedRows, failedRows, stoppedBy: null };
}
