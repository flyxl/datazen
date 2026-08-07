import { invoke } from '@tauri-apps/api/core';

export const backupCommands = {
  /** @deprecated E2E-only; prefer exportAppDataWithDialog. */
  exportAppData: (path: string) => invoke<void>('export_app_data', { path }),

  /** @deprecated E2E-only; prefer importAppDataWithDialog. */
  importAppData: (path: string) => invoke<void>('import_app_data', { path }),

  exportAppDataWithDialog: (defaultFileName: string) =>
    invoke<boolean>('export_app_data_with_dialog', { defaultFileName }),

  importAppDataWithDialog: (confirmTitle: string, confirmMessage: string) =>
    invoke<boolean>('import_app_data_with_dialog', {
      confirmTitle,
      confirmMessage,
    }),

  saveEncryptionKeyWithDialog: (defaultFileName: string) =>
    invoke<boolean>('save_encryption_key_with_dialog', { defaultFileName }),

  restartApp: () => invoke<void>('restart_app'),
};
