import { invoke } from '@tauri-apps/api/core';
import type { AppSettings } from '../types';

export const settingsCommands = {
  getSettings: () => invoke<AppSettings>('get_settings'),

  getSystemUiLanguage: () => invoke<string>('get_system_ui_language'),

  saveSettings: (settings: AppSettings) =>
    invoke<void>('save_settings', { settings }),

  getLogPath: () => invoke<string>('get_log_path'),

  /** @deprecated E2E-only; prefer openLogDir / openWorkflowsDir / openContextDir. */
  openPath: (path: string) => invoke<void>('open_path', { path }),

  openLogDir: () => invoke<void>('open_log_dir'),

  openWorkflowsDir: () => invoke<void>('open_workflows_dir'),

  openContextDir: () => invoke<void>('open_context_dir'),
};
