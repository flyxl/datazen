import { fileCommands } from '../commands/file';
import { queryCommands } from '../commands/query';
import type { ColumnInfo, QueryStreamEvent } from '../types';
import { escapeIdent } from './databaseTypes';
import {
  escapeCSV,
  escapeMarkdownCell,
  escapeSQLIdent,
  escapeSQLValue,
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
    const escapeTSV = (v: unknown) => {
      if (v === null || v === undefined) return '';
      const s = typeof v === 'object' ? JSON.stringify(v) : String(v);
      return s.replaceAll('\t', ' ').replaceAll('\n', ' ').replaceAll('\r', '');
    };
    return {
      header: () => `${columns.map(escapeTSV).join('\t')}\n`,
      formatRows(rows) {
        if (rows.length === 0) return '';
        return `${rows.map((row) => columns.map((col) => escapeTSV(row[col])).join('\t')).join('\n')}\n`;
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
      const tuples = take.map((row) => `  ${formatSqlInsertTuple(row, columns)}`).join(',\n');
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
}): Promise<void> {
  const streamQuery = opts.streamQuery ?? queryCommands.executeQueryStream;
  let columnNames = opts.columns ?? [];
  let started = false;
  let writeQueue: Promise<void> = Promise.resolve();
  const enqueue = (chunk: string) => {
    if (!chunk) return;
    writeQueue = writeQueue.then(() => opts.write(chunk));
  };

  await streamQuery(
    opts.dbSessionId,
    opts.sql,
    (event) => {
      if (event.type === 'statementStart') {
        if (columnNames.length === 0) {
          columnNames = event.columns.map((c: ColumnInfo) => c.name);
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
        const records = rowsToNamedRecords(
          columnNames.length > 0 ? columnNames : inferColumnNames(event.rows),
          event.rows,
        );
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
  abort: (token: string) => Promise<void>;
}

const defaultSaveSession: SaveSessionApi = {
  begin: fileCommands.beginSaveWithDialog,
  append: fileCommands.appendSaveText,
  finish: fileCommands.finishSave,
  abort: fileCommands.abortSave,
};

/** Stream a table export directly to a user-chosen file (no full-table row array). */
export async function streamTableExportToSaveDialog(opts: {
  dbSessionId: string;
  tableName: string;
  columns: string[];
  format: StreamableExportFormat;
  databaseType?: string;
  pkName?: string;
  streamQuery?: StreamQueryFn;
  saveSession?: SaveSessionApi;
}): Promise<'saved' | 'cancelled'> {
  const save = opts.saveSession ?? defaultSaveSession;
  const defaultName = getDefaultFilename(opts.tableName, opts.format);
  const ext = defaultName.split('.').pop() ?? 'txt';
  const token = await save.begin(defaultName, ext.toUpperCase(), [ext]);
  if (!token) return 'cancelled';

  const streamer = createTableExportStreamer({
    format: opts.format,
    tableName: opts.tableName,
    columns: opts.columns,
    databaseType: opts.databaseType,
    pkName: opts.pkName,
  });

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
      sql: buildTableSelectSql(opts.tableName, opts.columns, opts.databaseType),
      streamer,
      write: async (chunk) => {
        if (!chunk) return;
        buffer += chunk;
        await flush(false);
      },
      streamQuery: opts.streamQuery,
      columns: opts.columns,
    });
    await flush(true);
    await save.finish(token);
    return 'saved';
  } catch (err) {
    await save.abort(token).catch(() => undefined);
    throw err;
  }
}
