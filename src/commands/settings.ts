import { invoke } from '@tauri-apps/api/core';
import type { AppSettings } from '../types';

export const settingsCommands = {
  getSettings: () => invoke<AppSettings>('get_settings'),

  saveSettings: (settings: AppSettings) =>
    invoke<void>('save_settings', { settings }),

  getLogPath: () => invoke<string>('get_log_path'),

  openPath: (path: string) => invoke<void>('open_path', { path }),
};
