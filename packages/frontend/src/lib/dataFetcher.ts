/**
 * Frontend `DataFetcher` interface + resolver + `ProvidedDataFetcher`
 * (frontend-DataFetcher plan §2.1/§2.2, task 2).
 *
 * All table-row export goes through this one interface; callers (dialogs,
 * batch jobs, the three session-less surfaces) only know `DataFetcher`,
 * never a backend command. Implementations split on whether the backend
 * is involved at all:
 *
 * - `ProvidedDataFetcher`: the rows are already in the frontend. Format in
 *   memory (`exportFormat.ts`) and save — desktop via the dumb save-dialog
 *   commands, web via `downloadBlob`. Zero backend reads: it never invokes
 *   `export_tables_stream`, never puts rows in any invoke argument, and
 *   never touches a session or DTO. Only the two save-dialog commands
 *   (`save_text_with_dialog` / `save_base64_with_dialog`, which take
 *   already-formatted content and open a native dialog) are allowed.
 * - `SqlDataFetcher`: task 3. The resolver throws an explicit not-wired
 *   error for SQL scopes with a session until then — no fake
 *   implementation is built here.
 *
 * Progress is a single start/end pair (`rowsWritten: 0` then `total`,
 * `rowsTotal` known from `rows.length`); a cancelled save emits no end
 * event, because nothing was written.
 */

import { fileCommands } from '../commands/file';
import type { ExportTableSource } from '../commands/file';
import { downloadBlob } from '../platform/blobTablesExporter';
import { IS_DESKTOP_BUILD } from '../platform/env';
import { validateTableSource } from '../platform/exportPlan';
import type { ExportFormat, ExportScope } from './exportData';
import {
  buildProvidedExportFilename,
  formatRowsAsText,
  formatRowsAsXlsx,
  type ExportColumn,
} from './exportFormat';

export interface ExportProgress {
  table: string;
  rowsWritten: number;
  /** Known here (`rows.length`); unknown for SQL streaming. */
  rowsTotal?: number;
}

export interface DataFetcher {
  /** Whether the backend is involved (Rust command / session). Provided-only is false. */
  readonly needsBackend: boolean;
  /** Subscribe to progress; returns an unsubscribe function (same shape as `onExportProgress`). */
  onProgress(handler: (p: ExportProgress) => void): () => void;
  /** Run the export. Returns `'saved' | 'cancelled'`. */
  fetch(): Promise<'saved' | 'cancelled'>;
}

/**
 * Inputs bound at construction; `fetch()` takes no arguments (plan §2.1).
 * `columns`/`rows` are the provided-only inputs; `dbSessionId`/`sql` are
 * the SQL inputs. `primaryKey` is the `ExportTableInput.primaryKey`
 * carry-over for `sql_update` (the formatter falls back to the
 * `isPrimaryKey` column, then the first column, when it is absent).
 */
export interface ExportContext {
  tableName: string;
  scope: ExportScope;
  columns?: ExportColumn[];
  rows?: unknown[][];
  dbSessionId?: string;
  database?: string;
  schema?: string;
  databaseType?: string;
  sql?: string;
  mode?: 'data_only' | 'data_and_structure' | 'structure_only';
  ddl?: string;
  dataFormat: ExportFormat;
  outputMode?: 'single' | 'zip';
  primaryKey?: string | null;
}

export class ProvidedDataFetcher implements DataFetcher {
  readonly needsBackend = false;

  private readonly ctx: ExportContext;
  private readonly handlers = new Set<(p: ExportProgress) => void>();

  constructor(ctx: ExportContext) {
    this.ctx = ctx;
  }

  onProgress(handler: (p: ExportProgress) => void): () => void {
    this.handlers.add(handler);
    return () => {
      this.handlers.delete(handler);
    };
  }

  private emit(progress: ExportProgress): void {
    for (const handler of [...this.handlers]) {
      handler(progress);
    }
  }

  async fetch(): Promise<'saved' | 'cancelled'> {
    const { tableName, columns = [], rows = [], dataFormat } = this.ctx;

    // 1. Validate — the TS mirror of the retired Rust
    // `validate_provided_data`; the only copy of this check from now on.
    // `rows` is `unknown[][]` while the source type carries the wire
    // `(Value | null)[][]`; validation only reads lengths and names, so
    // the cast is shape-only.
    validateTableSource({
      tableName,
      columns: columns.map((column) => column.name),
      source: {
        type: 'provided',
        columns,
        rows,
      } as ExportTableSource,
    });

    const rowsTotal = rows.length;
    this.emit({ table: tableName, rowsWritten: 0, rowsTotal });

    // 2. Format the whole export in memory (provided is ≤500/1000 rows).
    const filename = buildProvidedExportFilename(tableName, dataFormat);
    let saved: boolean;
    if (dataFormat === 'xlsx') {
      const { dataBase64, extension, mimeType } = formatRowsAsXlsx({
        tableName,
        columns,
        rows,
        format: 'xlsx',
      });
      saved = await this.saveBinary(dataBase64, filename, extension, mimeType);
    } else {
      const { content, extension, mimeType } = formatRowsAsText({
        tableName,
        columns,
        rows,
        format: dataFormat,
        databaseType: this.ctx.databaseType,
        primaryKey: this.ctx.primaryKey,
      });
      saved = await this.saveText(content, filename, extension, mimeType);
    }

    if (!saved) {
      return 'cancelled';
    }
    this.emit({ table: tableName, rowsWritten: rowsTotal, rowsTotal });
    return 'saved';
  }

  /**
   * Desktop: native save dialog (`save_text_with_dialog` — a dumb save of
   * already-formatted text, no rows, no session). Web: Blob download.
   * The web path has no cancel signal, so it always reports saved.
   */
  private async saveText(
    content: string,
    filename: string,
    extension: string,
    mimeType: string,
  ): Promise<boolean> {
    if (IS_DESKTOP_BUILD) {
      return fileCommands.saveTextWithDialog(
        content,
        filename,
        extension.toUpperCase(),
        [extension],
      );
    }
    downloadBlob(content, filename, mimeType);
    return true;
  }

  /** Same split for binary output (`save_base64_with_dialog` on desktop). */
  private async saveBinary(
    dataBase64: string,
    filename: string,
    extension: string,
    mimeType: string,
  ): Promise<boolean> {
    if (IS_DESKTOP_BUILD) {
      return fileCommands.saveBase64WithDialog(
        dataBase64,
        filename,
        extension.toUpperCase(),
        [extension],
      );
    }
    downloadBlob(base64ToBytes(dataBase64), filename, mimeType);
    return true;
  }
}

/** Base64 → bytes for the web Blob download (desktop hands base64 to Rust instead). */
function base64ToBytes(dataBase64: string): Uint8Array<ArrayBuffer> {
  const binary = atob(dataBase64);
  const bytes = new Uint8Array(binary.length);
  for (let i = 0; i < binary.length; i += 1) {
    bytes[i] = binary.charCodeAt(i);
  }
  return bytes;
}

/**
 * The single dispatch point (plan §2.1):
 *
 * - `current_page` / `selected` with rows → `ProvidedDataFetcher`,
 *   **whether or not a session exists** (the rows are already loaded;
 *   a session must never gate them).
 * - `entire_table` / `filtered_table` without a session → unresolvable
 *   (permanent: dialogs hide those scopes, the three session-less
 *   surfaces only ever walk the provided path).
 * - `entire_table` / `filtered_table` with a session → `SqlDataFetcher`,
 *   wired by task 3; until then an explicit not-wired error, never a
 *   fake implementation.
 * - Provided scope without rows → defensive error (must not happen).
 */
export function resolveDataFetcher(ctx: ExportContext): DataFetcher {
  const isProvidedScope = ctx.scope === 'current_page' || ctx.scope === 'selected';
  if (isProvidedScope) {
    if (ctx.rows === undefined) {
      throw new Error(
        `provided export for table '${ctx.tableName}' has no loaded rows ` +
          `(scope '${ctx.scope}'): current-page / selected exports need rows in the frontend`,
      );
    }
    return new ProvidedDataFetcher(ctx);
  }
  if (ctx.dbSessionId === undefined || ctx.dbSessionId === '') {
    throw new Error(
      `cannot resolve an exporter for table '${ctx.tableName}' ` +
        `(scope '${ctx.scope}'): entire-table and filtered exports need a database session`,
    );
  }
  throw new Error(
    `SqlDataFetcher is not wired yet (frontend-DataFetcher plan task 3): ` +
      `table '${ctx.tableName}' (scope '${ctx.scope}') needs a SQL-backed export`,
  );
}
