import { fileCommands } from '../commands/file';
import { queryCommands } from '../commands/query';
import type { ColumnInfo, QueryStreamEvent } from '../types';
import { escapeIdent } from './databaseTypes';
import {
  escapeCSV,
  escapeMarkdownCell,
  escapeSQLIdent,
  escapeSQLValue,
  escapeTSVField,
  formatSqlInsertHeader,
  formatSqlInsertTuple,
  getDefaultFilename,
  sqlBeginTransaction,
  sqlCommitTransaction,
  SQL_INSERT_BATCH_SIZE,
  type ExportFormat,
} from './exportData';

const APPEND_FLUSH_BYTES = 64 * 1024;

export type StreamableExportFormat = Exclude<ExportFormat, 'xlsx'>;

export function isStreamableExportFormat(format: ExportFormat): format is StreamableExportFormat {
  return format !== 'xlsx';
}

export function buildTableSelectSql(
  tableName: string,
  columns: string[],
  databaseType?: string,
): string {
  const quotedTable = escapeIdent(tableName, databaseType as never);
  if (columns.length === 0) {
    return `SELECT * FROM ${quotedTable}`;
  }
  const cols = columns.map((c) => escapeIdent(c, databaseType as never)).join(', ');
  return `SELECT ${cols} FROM ${quotedTable}`;
}

export function rowsToNamedRecords(
  columns: string[],
  rows: (unknown | null)[][],
): Record<string, unknown>[] {
  return rows.map((row) => {
    const record: Record<string, unknown> = {};
    for (let i = 0; i < columns.length; i += 1) {
      record[columns[i]!] = row[i] ?? null;
    }
    return record;
  });
}

export interface TableExportStreamer {
  header(): string;
  formatRows(rows: Record<string, unknown>[]): string;
  footer(): string;
}

export function createTableExportStreamer(opts: {
  format: StreamableExportFormat;
  tableName: string;
  columns: string[];
  databaseType?: string;
  pkName?: string;
}): TableExportStreamer {
  const { format, tableName, columns, databaseType, pkName } = opts;

  if (format === 'csv') {
    return {
      header: () => `${columns.map(escapeCSV).join(',')}\n`,
      formatRows(rows) {
        if (rows.length === 0) return '';
        return `${rows.map((row) => columns.map((col) => escapeCSV(row[col])).join(',')).join('\n')}\n`;
      },
      footer: () => '',
    };
  }

  if (format === 'tsv') {
    return {
      header: () => `${columns.map(escapeTSVField).join('\t')}\n`,
      formatRows(rows) {
        if (rows.length === 0) return '';
        return `${rows.map((row) => columns.map((col) => escapeTSVField(row[col])).join('\t')).join('\n')}\n`;
      },
      footer: () => '',
    };
  }

  if (format === 'json') {
    let written = 0;
    return {
      header: () => '[',
      formatRows(rows) {
        if (rows.length === 0) return '';
        const chunk = formatJsonChunk(rows, columns, written);
        written += rows.length;
        return chunk;
      },
      footer: () => '\n]',
    };
  }

  if (format === 'markdown') {
    return {
      header: () => `| ${columns.join(' | ')} |\n| ${columns.map(() => '---').join(' | ')} |\n`,
      formatRows(rows) {
        if (rows.length === 0) return '';
        return `${rows
          .map((row) => `| ${columns.map((col) => escapeMarkdownCell(row[col])).join(' | ')} |`)
          .join('\n')}\n`;
      },
      footer: () => '',
    };
  }

  if (format === 'sql_insert') {
    let pending: Record<string, unknown>[] = [];
    const flush = (forceAll: boolean): string => {
      if (pending.length === 0) return '';
      if (!forceAll && pending.length < SQL_INSERT_BATCH_SIZE) return '';
      const take = forceAll
        ? pending.splice(0, pending.length)
        : pending.splice(0, SQL_INSERT_BATCH_SIZE);
      const header = formatSqlInsertHeader(tableName, columns, databaseType);
      const tuples = take
        .map((row) => `  ${formatSqlInsertTuple(row, columns, databaseType)}`)
        .join(',\n');
      return `${header}\n${tuples};\n`;
    };
    return {
      header: () => `${sqlBeginTransaction(databaseType)}\n`,
      formatRows(rows) {
        pending.push(...rows);
        let out = '';
        while (pending.length >= SQL_INSERT_BATCH_SIZE) {
          out += flush(false);
        }
        return out;
      },
      footer: () => `${flush(true)}${sqlCommitTransaction()}\n`,
    };
  }

  // sql_update
  const key = pkName ?? columns[0] ?? 'id';
  return {
    header: () => `${sqlBeginTransaction(databaseType)}\n`,
    formatRows(rows) {
      if (rows.length === 0) return '';
      return `${rows
        .map((row) => {
          const setClauses = columns
            .filter((c) => c !== key)
            .map((col) => `${escapeSQLIdent(col, databaseType)} = ${escapeSQLValue(row[col])}`)
            .join(', ');
          const where = `${escapeSQLIdent(key, databaseType)} = ${escapeSQLValue(row[key])}`;
          return `UPDATE ${escapeSQLIdent(tableName, databaseType)} SET ${setClauses} WHERE ${where};`;
        })
        .join('\n')}\n`;
    },
    footer: () => `${sqlCommitTransaction()}\n`,
  };
}

function formatJsonChunk(
  rows: Record<string, unknown>[],
  columns: string[],
  alreadyWritten: number,
): string {
  const parts: string[] = [];
  for (let i = 0; i < rows.length; i += 1) {
    const obj: Record<string, unknown> = {};
    for (const col of columns) obj[col] = rows[i]![col] ?? null;
    const prefix = alreadyWritten + i === 0 ? '\n' : ',\n';
    parts.push(`${prefix}  ${JSON.stringify(obj)}`);
  }
  return parts.join('');
}

export interface StreamQueryFn {
  (
    dbSessionId: string,
    sql: string,
    onEvent: (event: QueryStreamEvent) => void,
    options?: { applyResultLimit?: boolean; recordHistory?: boolean },
  ): Promise<void>;
}

export async function streamQueryIntoExporter(opts: {
  dbSessionId: string;
  sql: string;
  streamer: TableExportStreamer;
  write: (chunk: string) => void | Promise<void>;
  streamQuery?: StreamQueryFn;
  columns?: string[];
  /** Rows-written progress: one call per `rows` event (DBR-06 行数进度). */
  onRows?: (rowCount: number) => void;
  /** The backend execution id, as soon as the stream announces it (DBR-06 取消). */
  onExecutionId?: (executionId: string) => void;
  /** Stops consuming when it reads true; the stream itself keeps draining. */
  isCancelled?: () => boolean;
}): Promise<void> {
  const streamQuery = opts.streamQuery ?? queryCommands.executeQueryStream;
  // The statement's own column names — records are keyed by them so a caller
  // may export a *subset* of `opts.columns` while the SQL selects more
  // (DBR-06: the filtered scope streams the backend's full read, the streamer
  // then writes only the columns the user picked).
  let sqlColumns: string[] = [];
  let started = false;
  let writeQueue: Promise<void> = Promise.resolve();
  const enqueue = (chunk: string) => {
    if (!chunk) return;
    if (opts.isCancelled?.()) return;
    writeQueue = writeQueue.then(() => opts.write(chunk));
  };

  await streamQuery(
    opts.dbSessionId,
    opts.sql,
    (event) => {
      if (event.type === 'executionStarted') {
        opts.onExecutionId?.(event.executionId);
        return;
      }
      if (event.type === 'statementStart') {
        if (sqlColumns.length === 0) {
          sqlColumns = event.columns.map((c: ColumnInfo) => c.name);
        }
        if (!started) {
          started = true;
          enqueue(opts.streamer.header());
        }
        return;
      }
      if (event.type === 'rows') {
        if (!started) {
          started = true;
          enqueue(opts.streamer.header());
        }
        opts.onRows?.(event.rows.length);
        const recordColumns =
          sqlColumns.length > 0
            ? sqlColumns
            : opts.columns !== undefined && opts.columns.length > 0
              ? opts.columns
              : inferColumnNames(event.rows);
        const records = rowsToNamedRecords(recordColumns, event.rows);
        enqueue(opts.streamer.formatRows(records));
      }
    },
    { applyResultLimit: false, recordHistory: false },
  );

  await writeQueue;
  if (!started) {
    await opts.write(opts.streamer.header());
  }
  await opts.write(opts.streamer.footer());
}

function inferColumnNames(rows: (unknown | null)[][]): string[] {
  const width = rows[0]?.length ?? 0;
  return Array.from({ length: width }, (_, i) => `col_${i}`);
}

/** Stream a table export into a single in-memory string (one table at a time). */
export async function streamTableExportText(opts: {
  dbSessionId: string;
  tableName: string;
  columns: string[];
  format: StreamableExportFormat;
  databaseType?: string;
  pkName?: string;
  streamQuery?: StreamQueryFn;
}): Promise<string> {
  const streamer = createTableExportStreamer({
    format: opts.format,
    tableName: opts.tableName,
    columns: opts.columns,
    databaseType: opts.databaseType,
    pkName: opts.pkName,
  });
  let content = '';
  await streamQueryIntoExporter({
    dbSessionId: opts.dbSessionId,
    sql: buildTableSelectSql(opts.tableName, opts.columns, opts.databaseType),
    streamer,
    write: (chunk) => {
      content += chunk;
    },
    streamQuery: opts.streamQuery,
    columns: opts.columns,
  });
  return content;
}

export interface SaveSessionApi {
  begin: (
    defaultFileName: string,
    filterName: string,
    extensions: string[],
  ) => Promise<string | null>;
  append: (token: string, chunk: string) => Promise<void>;
  finish: (token: string) => Promise<void>;
  /** Removes the partial `.datazen-part` sibling; resolves to whether it was deleted. */
  abort: (token: string) => Promise<boolean>;
}

const defaultSaveSession: SaveSessionApi = {
  begin: fileCommands.beginSaveWithDialog,
  append: fileCommands.appendSaveText,
  finish: fileCommands.finishSave,
  abort: fileCommands.abortSave,
};

/** The honest end states of a streamed export (DBR-06: 取消、磁盘写入失败
 * 与保存对话框取消必须是不同的结果状态):
 * - `saved`: the file was finished and renamed into place.
 * - `cancelled`: the save dialog was dismissed before anything was written.
 * - `aborted`: the run was cancelled mid-flight; `partialDeleted` says whether
 *   the in-progress file was removed (an abort that could not delete it is
 *   reported instead of assumed).
 * Errors (disk failure, driver failure) still throw — the dialogs' error
 * surface is a distinct state from every one of these. */
export type TableExportResult =
  | { status: 'saved' }
  | { status: 'cancelled' }
  | { status: 'aborted'; partialDeleted: boolean };

/** Cancellation handle of a running streamed export (DBR-06). */
export interface TableExportRunHandle {
  /** Ask the run to stop. Idempotent; safe to call more than once. */
  cancel: () => void;
}

/** Stream a table export directly to a user-chosen file (no full-table row array). */
export async function streamTableExportToSaveDialog(opts: {
  dbSessionId: string;
  tableName: string;
  /** The columns to export, in display order — may be a subset of what `sql` selects. */
  columns: string[];
  format: StreamableExportFormat;
  databaseType?: string;
  pkName?: string;
  /**
   * The statement to stream, when the caller has one the product built
   * (DB-15: the filtered full-table export streams the backend's own
   * `previewTableReadSql` statement). Absent, the plain `SELECT` of the
   * given columns runs.
   */
  sql?: string;
  streamQuery?: StreamQueryFn;
  saveSession?: SaveSessionApi;
  /** Progress: rows written so far (DBR-06 大型导出任务行数进度). */
  onProgress?: (rowsWritten: number) => void;
  /** Receives the cancel handle once the save dialog returned a path (DBR-06). */
  onRunStart?: (handle: TableExportRunHandle) => void;
  /** Query cancellation, injectable for tests; defaults to `cancelQuery`. */
  cancelQuery?: (executionId: string) => Promise<void>;
}): Promise<TableExportResult> {
  const save = opts.saveSession ?? defaultSaveSession;
  const cancelQuery =
    opts.cancelQuery ??
    ((executionId: string) => queryCommands.cancelQuery(opts.dbSessionId, executionId));
  const defaultName = getDefaultFilename(opts.tableName, opts.format);
  const ext = defaultName.split('.').pop() ?? 'txt';
  const token = await save.begin(defaultName, ext.toUpperCase(), [ext]);
  if (!token) return { status: 'cancelled' };

  const streamer = createTableExportStreamer({
    format: opts.format,
    tableName: opts.tableName,
    columns: opts.columns,
    databaseType: opts.databaseType,
    pkName: opts.pkName,
  });

  let cancelRequested = false;
  let executionId: string | undefined;
  let rowsWritten = 0;
  let partialDeleted: Promise<boolean> | undefined;
  const handle: TableExportRunHandle = {
    cancel() {
      if (cancelRequested) return;
      cancelRequested = true;
      // Best effort: drivers without query cancellation keep draining
      // server-side, but nothing is written from here on. The save session
      // is aborted immediately so the partial `.datazen-part` is removed.
      if (executionId !== undefined) {
        void cancelQuery(executionId).catch(() => undefined);
      }
      partialDeleted = save.abort(token).then(
        (deleted) => deleted,
        () => false,
      );
    },
  };
  opts.onRunStart?.(handle);

  let buffer = '';
  const flush = async (force = false) => {
    if (buffer.length === 0) return;
    if (!force && buffer.length < APPEND_FLUSH_BYTES) return;
    const chunk = buffer;
    buffer = '';
    await save.append(token, chunk);
  };

  try {
    await streamQueryIntoExporter({
      dbSessionId: opts.dbSessionId,
      sql: opts.sql ?? buildTableSelectSql(opts.tableName, opts.columns, opts.databaseType),
      streamer,
      write: async (chunk) => {
        if (!chunk || cancelRequested) return;
        buffer += chunk;
        await flush(false);
      },
      streamQuery: opts.streamQuery,
      columns: opts.columns,
      onRows: (rowCount) => {
        if (cancelRequested) return;
        rowsWritten += rowCount;
        opts.onProgress?.(rowsWritten);
      },
      onExecutionId: (id) => {
        executionId = id;
      },
      isCancelled: () => cancelRequested,
    });
    if (cancelRequested) {
      return { status: 'aborted', partialDeleted: (await partialDeleted) ?? false };
    }
    await flush(true);
    await save.finish(token);
    return { status: 'saved' };
  } catch (err) {
    if (cancelRequested) {
      return { status: 'aborted', partialDeleted: (await partialDeleted) ?? false };
    }
    // A disk/driver failure: abort the session (the partial sibling is
    // removed) and rethrow — the error path is its own end state, distinct
    // from both the cancel and the saved outcomes (DBR-06).
    partialDeleted = save.abort(token).then(
      (deleted) => deleted,
      () => false,
    );
    try {
      await partialDeleted;
    } catch {
      // The abort outcome is best effort; the original error is the report.
    }
    throw err;
  }
}
