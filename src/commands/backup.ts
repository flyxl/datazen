import { invoke } from '@tauri-apps/api/core';

export const backupCommands = {
  /**
   * Decision 3 merged IPC: native save dialog + ZIP export.
   * The wire-level `override_path` escape hatch is webdriver/E2E-only and is
   * never sent from production code. Returns false when the dialog was
   * dismissed.
   */
  exportAppData: (defaultFileName: string) =>
    invoke<boolean>('export_app_data', { defaultFileName }),

  /**
   * Decision 3 merged IPC: native open + confirm + ZIP import.
   * The wire-level `override_path` escape hatch is webdriver/E2E-only and is
   * never sent from production code. Returns false on cancel/decline.
   */
  importAppData: (confirmTitle: string, confirmMessage: string) =>
    invoke<boolean>('import_app_data', {
      confirmTitle,
      confirmMessage,
    }),

  saveEncryptionKeyWithDialog: (defaultFileName: string) =>
    invoke<boolean>('save_encryption_key_with_dialog', { defaultFileName }),

  restartApp: () => invoke<void>('restart_app'),
};
