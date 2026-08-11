import { invoke } from '@tauri-apps/api/core';

export type HistoryPurgeScope = 'query' | 'workflow' | 'all';

export const historyCommands = {
  purgeHistory: (args: { scope: HistoryPurgeScope; retainDays: number | null }) =>
    invoke<number>('purge_history', args),
};
