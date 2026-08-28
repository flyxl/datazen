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

  /** Native open dialog for app-data ZIP import (path only; no import). */
  pickAppDataImportFile: () => invoke<string | null>('pick_app_data_import_file'),

  /**
   * Decision 3 merged IPC: ZIP import from a path chosen in the UI.
   * Production callers pass `sourcePath` after pick + web confirm. The
   * wire-level `override_path` escape hatch is webdriver/E2E-only.
   */
  importAppData: (sourcePath: string) => invoke<boolean>('import_app_data', { sourcePath }),

  saveEncryptionKeyWithDialog: (defaultFileName: string) =>
    invoke<boolean>('save_encryption_key_with_dialog', { defaultFileName }),

  restartApp: () => invoke<void>('restart_app'),
};
