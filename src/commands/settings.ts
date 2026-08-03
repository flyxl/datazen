import { invoke } from '@tauri-apps/api/core';
import type { AppSettings } from '../types';

export const settingsCommands = {
  getSettings: () => invoke<AppSettings>('get_settings'),

  saveSettings: (settings: AppSettings) =>
    invoke<void>('save_settings', { settings }),

  getLogPath: () => invoke<string>('get_log_path'),

  openLogFolder: () => invoke<void>('open_log_folder'),
};
