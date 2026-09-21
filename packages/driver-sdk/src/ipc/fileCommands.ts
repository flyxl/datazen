import { invoke } from '@tauri-apps/api/core';

export interface OpenedBinaryFile {
  fileName: string;
  dataBase64: string;
}

/**
 * Native-dialog file IO shared by drivers (e.g. Redis dump import/export).
 * Dialog + read/write happen atomically on the Rust side; paths never reach JS.
 */
export const fileCommands = {
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

  /** Open a binary file via native dialog; returns basename + base64 (no path). */
  openBase64WithDialog: (filterName: string, extensions: string[]) =>
    invoke<OpenedBinaryFile | null>('open_base64_with_dialog', {
      filterName,
      extensions,
    }),
};
