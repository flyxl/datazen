import { invoke } from '@tauri-apps/api/core';
import { fileCommands as dialogFileCommands } from '@datazen/driver-sdk';

export interface OpenedTextFile {
  fileName: string;
  content: string;
}

export type { OpenedBinaryFile } from '@datazen/driver-sdk';

/**
 * File IO that never accepts a path from JS (XSS-safe).
 * Dialog + read/write happen atomically in Rust.
 * The native-dialog save/open helpers shared with drivers live in
 * `@datazen/driver-sdk`; host-only streaming/session commands stay here.
 */
export const fileCommands = {
  ...dialogFileCommands,

  /** Open a text file via native dialog; returns basename + content (no path). */
  openTextWithDialog: (filterName: string, extensions: string[]) =>
    invoke<OpenedTextFile | null>('open_text_with_dialog', {
      filterName,
      extensions,
    }),

  /**
   * Open a save dialog and keep an opaque write session (path never returns to JS).
   * Returns a token, or null if the user cancelled.
   */
  beginSaveWithDialog: (defaultFileName: string, filterName: string, extensions: string[]) =>
    invoke<string | null>('begin_save_with_dialog', {
      defaultFileName,
      filterName,
      extensions,
    }),

  appendSaveText: (token: string, chunk: string) =>
    invoke<void>('append_save_text', { token, chunk }),

  finishSave: (token: string) => invoke<void>('finish_save', { token }),

  abortSave: (token: string) => invoke<void>('abort_save', { token }),

  /**
   * Stream selected tables to a single file or ZIP entirely on the Rust side
   * (opens its own native save dialog). Never buffers whole tables in JS.
   */
  exportTablesStream: (request: ExportTablesRequest) =>
    invoke<ExportTablesResult | null>('export_tables_stream', { request }),
};

export type ExportMode = 'structure_only' | 'data_only' | 'data_and_structure';
export type ExportDataFormat = 'csv' | 'json' | 'sql_insert';
export type ExportOutputMode = 'single' | 'zip';

export interface ExportTableInput {
  tableName: string;
  /** Schema the table lives in (PostgreSQL family); resolved per table. */
  schema?: string | null;
  columns: string[];
  ddl?: string | null;
}

export interface ExportTablesRequest {
  /** Runtime db session id (backend `ExportTablesRequest.db_session_id`). */
  dbSessionId: string;
  databaseType?: string | null;
  /** Database to read from; the session's own database is only a fallback. */
  database?: string | null;
  /** Fallback schema for tables that do not carry one. */
  schema?: string | null;
  mode: ExportMode;
  dataFormat: ExportDataFormat;
  outputMode: ExportOutputMode;
  tables: ExportTableInput[];
}

export type ExportTablesResult = { Saved: number } | { Cancelled: null };

/** Progress emitted by the Rust exporter while streaming a table to disk. */
export interface ExportProgressEvent {
  table: string;
  rowsWritten: number;
}

/**
 * Subscribe to writer progress from `export_tables_stream`. Returns an
 * unlisten function. Setting it up lets the UI show row-write progress during
 * the (potentially long) streaming phase.
 */
export function onExportProgress(
  handler: (event: ExportProgressEvent) => void,
): Promise<import('@tauri-apps/api/event').UnlistenFn> {
  return import('@tauri-apps/api/event').then(({ listen }) =>
    listen<ExportProgressEvent>('batch-export-progress', (e) => handler(e.payload)),
  );
}
