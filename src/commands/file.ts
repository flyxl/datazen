import { invoke } from '@tauri-apps/api/core';

export const fileCommands = {
  writeFile: (path: string, contents: string) =>
    invoke<void>('write_file', { path, contents }),

  /** Write binary content (e.g. PNG). `dataBase64` is raw base64 without data-URL prefix. */
  writeFileBase64: (path: string, dataBase64: string) =>
    invoke<void>('write_file_base64', { path, dataBase64 }),

  readFile: (path: string) =>
    invoke<string>('read_file', { path }),
};
