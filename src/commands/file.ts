import { invoke } from '@tauri-apps/api/core';

export const fileCommands = {
  writeFile: (path: string, contents: string) =>
    invoke<void>('write_file', { path, contents }),

  readFile: (path: string) =>
    invoke<string>('read_file', { path }),
};
