import { invoke } from '@tauri-apps/api/core';
import type { AppSettings } from '../types';
import { useBackendClient, toSharedSettings } from '@datazen/backend-client';

export const settingsCommands = {
  getSettings: async (): Promise<Partial<AppSettings>> => {
    const client = useBackendClient();
    const shared = await client.getSettings();
    const local = client.backendId === 'desktop' ? await invoke<AppSettings>('get_settings') : {};
    return {
      ...local,
      ...shared.preferences,
      ...shared.execution,
      sqlSyntaxTheme: shared.preferences.sqlSyntaxTheme ?? undefined,
    };
  },

  saveSettings: async (settings: AppSettings): Promise<Partial<AppSettings>> => {
    const client = useBackendClient();
    // The native adapter retains paths, onboarding and notification credentials.
    if (client.backendId === 'desktop') await invoke<void>('save_settings', { settings });
    const saved = await client.saveSettings(toSharedSettings(settings));
    return {
      ...saved.preferences,
      ...saved.execution,
      sqlSyntaxTheme: saved.preferences.sqlSyntaxTheme ?? undefined,
    };
  },

  syncLanguageMenu: async (language: string): Promise<void> => {
    if (useBackendClient().backendId === 'desktop')
      await invoke<void>('rebuild_menu', { language });
  },

  getLogPath: () => invoke<string>('get_log_path'),

  getAppExecutablePath: () => invoke<string>('get_app_executable_path'),

  /** @deprecated E2E-only; prefer openLogDir / openWorkflowsDir / openContextDir. */
  openPath: (path: string) => invoke<void>('open_path', { path }),

  openLogDir: () => invoke<void>('open_log_dir'),

  openWorkflowsDir: () => invoke<void>('open_workflows_dir'),

  openContextDir: () => invoke<void>('open_context_dir'),
};
