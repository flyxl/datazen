import { invoke } from '@tauri-apps/api/core';

export interface OpenedTextFile {
  fileName: string;
  content: string;
}

export interface OpenedBinaryFile {
  fileName: string;
  dataBase64: string;
}

/**
 * File IO that never accepts a path from JS (XSS-safe).
 * Dialog + read/write happen atomically in Rust.
 */
export const fileCommands = {
  /** @deprecated Prefer saveTextWithDialog — path-based writes are E2E-only. */
  writeFile: (path: string, contents: string) => invoke<void>('write_file', { path, contents }),

  /** @deprecated Prefer saveBase64WithDialog. */
  writeFileBase64: (path: string, dataBase64: string) =>
    invoke<void>('write_file_base64', { path, dataBase64 }),

  /** @deprecated Prefer openTextWithDialog. */
  readFile: (path: string) => invoke<string>('read_file', { path }),

  /** Save UTF-8 text via native OS dialog. Returns false if cancelled. */
  saveTextWithDialog: (
    contents: string,
    defaultFileName: string,
    filterName: string,
    extensions: string[],
  ) =>
    invoke<boolean>('save_text_with_dialog', {
      contents,
      defaultFileName,
      filterName,
      extensions,
    }),

  /** Save base64 bytes via native OS dialog. Returns false if cancelled. */
  saveBase64WithDialog: (
    dataBase64: string,
    defaultFileName: string,
    filterName: string,
    extensions: string[],
  ) =>
    invoke<boolean>('save_base64_with_dialog', {
      dataBase64,
      defaultFileName,
      filterName,
      extensions,
    }),

  /** Open a text file via native dialog; returns basename + content (no path). */
  openTextWithDialog: (filterName: string, extensions: string[]) =>
    invoke<OpenedTextFile | null>('open_text_with_dialog', {
      filterName,
      extensions,
    }),

  /** Open a binary file via native dialog; returns basename + base64 (no path). */
  openBase64WithDialog: (filterName: string, extensions: string[]) =>
    invoke<OpenedBinaryFile | null>('open_base64_with_dialog', {
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
  columns: string[];
  ddl?: string | null;
}

export interface ExportTablesRequest {
  connectionId: string;
  databaseType?: string | null;
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
