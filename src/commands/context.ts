import { invoke } from '@tauri-apps/api/core';
import type { ContextEntry } from '../types';

export const contextCommands = {
  getDir: () => invoke<string>('context_get_dir'),
  listFiles: (query?: string) =>
    invoke<ContextEntry[]>('context_list_files', { query: query ?? null }),
  readFiles: (paths: string[]) =>
    invoke<[string, string][]>('context_read_files', { paths }),
};
