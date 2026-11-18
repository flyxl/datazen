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
 * - `SqlDataFetcher`: task 3. A thin wrapper over the Rust batch exporter:
 *   it builds the same single-table `ExportTablesRequest` the old
 *   `exportSingleTableViaRust` built, subscribes to `onExportProgress`
 *   (`batch-export-progress`) and forwards it as `ExportProgress`
 *   (`rowsTotal` unknown for streaming), then invokes
 *   `export_tables_stream` through the tables-exporter port. It never
 *   touches rows in JS, never formats, never runs the guard — all of that
 *   stays in Rust.
 *
 * Cancellation (mid-flight, SQL path only): the fetcher mints one token
 * per run (`crypto.randomUUID`), carries it on the request, and
 * `cancel()` flips it through the port's `cancelExportTables`
 * (`cancel_export_tables` on desktop). An `Aborted` result maps to
 * `'aborted'` and records whether the partial file was deleted
 * (`abortedPartialDeleted`). Provided fetchers never mint a token.
 *
 * Progress is a single start/end pair (`rowsWritten: 0` then `total`,
 * `rowsTotal` known from `rows.length`); a cancelled save emits no end
 * event, because nothing was written.
 */

import { fileCommands, onExportProgress } from '../commands/file';
import type {
  ExportProgressEvent,
  ExportTablesRequest,
  ExportTablesResult,
  ExportTableSource,
} from '../commands/file';
import { requireTablesExporter } from '../platform/contracts';
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
  /**
   * Run the export. Returns `'saved' | 'cancelled' | 'aborted'`
   * (`'aborted'` only on the SQL path, when the run was cancelled
   * mid-flight; provided exports are one-shot and never abort).
   */
  fetch(): Promise<'saved' | 'cancelled' | 'aborted'>;
  /**
   * Ask a running export to stop (SQL path only). Provided fetchers have
   * no run to stop and leave this unset; dialogs only offer cancel when
   * it exists.
   */
  cancel?(): void;
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
  /** Column names only (the SQL path needs no types/nullability). */
  columnNames?: string[];
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

    // 1. Validate — the only copy of the provided check (the Rust
    // provided source was deleted in task 5, so there is no second copy
    // of this check to drift from).
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
    } else if (dataFormat === 'parquet') {
      // Provided-data Parquet is a PRD non-goal (the dialogs grey it out for
      // these scopes); a smuggled request fails loudly instead of silently
      // rendering a wrong-format file.
      throw new Error("provided-data export cannot format 'parquet'");
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
      return fileCommands.saveTextWithDialog(content, filename, extension.toUpperCase(), [
        extension,
      ]);
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
      return fileCommands.saveBase64WithDialog(dataBase64, filename, extension.toUpperCase(), [
        extension,
      ]);
    }
    downloadBlob(base64ToBytes(dataBase64), filename, mimeType);
    return true;
  }
}

/**
 * Injectable seams for `SqlDataFetcher` (tests; production uses the
 * defaults): the tables-exporter port's `exportTables` and the
 * `batch-export-progress` subscription. Both defaults are the same calls
 * the old `exportSingleTableViaRust` made.
 */
export interface SqlDataFetcherDeps {
  exportTables?: (request: ExportTablesRequest) => Promise<ExportTablesResult | null>;
  subscribeProgress?: (handler: (event: ExportProgressEvent) => void) => Promise<() => void>;
  /**
   * Injectable cancel for tests; defaults to the tables-exporter port's
   * `cancelExportTables` (desktop). A form without one (web) has no
   * cancellation to offer, so `cancel()` is a no-op there.
   */
  cancelExport?: (token: string) => Promise<boolean>;
}

/**
 * SQL-backed export (plan §2.3): a thin wrapper over the Rust batch
 * exporter. `fetch()` builds the same single-table `ExportTablesRequest`
 * `exportSingleTableViaRust` built, subscribes to `onExportProgress` and
 * forwards it as `ExportProgress` (`rowsTotal` unknown — streaming), then
 * invokes `export_tables_stream` and maps the result to
 * `'saved' | 'cancelled' | 'aborted'`. No rows are touched in JS, no formatting, no
 * guard — all of that stays in Rust. The subscription is released in a
 * `finally`, whether the export saves, cancels, or throws.
 */
export class SqlDataFetcher implements DataFetcher {
  readonly needsBackend = true;

  private readonly ctx: ExportContext;
  private readonly handlers = new Set<(p: ExportProgress) => void>();
  private readonly exportTables: (
    request: ExportTablesRequest,
  ) => Promise<ExportTablesResult | null>;
  private readonly subscribeProgress: (
    handler: (event: ExportProgressEvent) => void,
  ) => Promise<() => void>;
  /** Per-run cancellation token, minted at construction so `cancel()` works as soon as the fetcher exists. */
  private readonly cancelToken: string;
  private readonly cancelExport: (token: string) => Promise<boolean>;
  /** Whether the aborted run's partial file was deleted (set by `fetch()` on `'aborted'`). */
  abortedPartialDeleted: boolean | null = null;
  private cancelRequested = false;

  constructor(ctx: ExportContext, deps: SqlDataFetcherDeps = {}) {
    this.ctx = ctx;
    this.cancelToken = crypto.randomUUID();
    this.exportTables =
      deps.exportTables ??
      ((request: ExportTablesRequest) => requireTablesExporter().exportTables(request));
    this.subscribeProgress = deps.subscribeProgress ?? onExportProgress;
    this.cancelExport =
      deps.cancelExport ??
      (async (token: string) =>
        (await requireTablesExporter().cancelExportTables?.(token)) ?? false);
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

  /**
   * Ask the running Rust export to stop. Idempotent; safe to call more
   * than once, and safe to call before `fetch()` starts (the token is
   * already minted, so a pre-registered cancel still aborts the run).
   * Best effort: a `false` answer (no live run, web form) only means the
   * Rust side had nothing to stop.
   */
  cancel(): void {
    if (this.cancelRequested) return;
    this.cancelRequested = true;
    void this.cancelExport(this.cancelToken).catch(() => undefined);
  }

  async fetch(): Promise<'saved' | 'cancelled' | 'aborted'> {
    const { dbSessionId, databaseType, database, schema, tableName, dataFormat, sql, primaryKey } =
      this.ctx;

    if (!dbSessionId) {
      throw new Error('Missing connection');
    }

    const columns = this.ctx.columnNames ?? this.ctx.columns?.map((c) => c.name) ?? [];
    const request: ExportTablesRequest = {
      dbSessionId,
      cancelToken: this.cancelToken,
      databaseType: databaseType ?? undefined,
      database: database ?? undefined,
      schema: schema ?? undefined,
      mode: this.ctx.mode ?? 'data_only',
      dataFormat,
      outputMode: this.ctx.outputMode ?? 'single',
      tables: [
        {
          tableName,
          schema: schema ?? undefined,
          columns,
          source: sql ? { type: 'sql', sql } : { type: 'table' },
          primaryKey: primaryKey ?? undefined,
          // DDL travels only when the caller carries one (structure
          // modes); a data-only request keeps the old shape exactly.
          ...(this.ctx.ddl !== undefined && this.ctx.ddl !== null ? { ddl: this.ctx.ddl } : {}),
        },
      ],
    };

    const unlisten = await this.subscribeProgress((event) =>
      this.emit({ table: event.table, rowsWritten: event.rowsWritten }),
    );
    try {
      const result = await this.exportTables(request);
      if (result && 'Aborted' in result) {
        this.abortedPartialDeleted = result.Aborted.partialDeleted;
        return 'aborted';
      }
      return result && 'Saved' in result ? 'saved' : 'cancelled';
    } finally {
      unlisten();
    }
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
 * - `entire_table` / `filtered_table` with a session → `SqlDataFetcher`
 *   (task 3).
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
  return new SqlDataFetcher(ctx);
}
