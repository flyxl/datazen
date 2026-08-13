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
};
