import { invoke } from '@tauri-apps/api/core';

export const backupCommands = {
  exportAppData: (path: string) => invoke<void>('export_app_data', { path }),

  importAppData: (path: string) => invoke<void>('import_app_data', { path }),

  restartApp: () => invoke<void>('restart_app'),
};
