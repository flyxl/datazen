/**
 * Desktop host overlay services (form-split F2, port 4).
 *
 * These capabilities are desktop-only: the legacy host settings pair
 * (`get_settings` / `save_settings` — full desktop field set), the menu
 * rebuild and the host directory openers. The implementation is exported from
 * this desktop zone only; shared code never sees it by name (ruling #7).
 * F4 retires the legacy commands by folding the projection into the desktop
 * settings domain client; F5 moves the updater entry behind capabilities.
 */

import { invoke } from '@tauri-apps/api/core';
import type { AppSettings } from '../../types';
import type { DesktopHostServices } from '../contracts';
import { maybeCheckOnStartup } from './updater';

export function createDesktopHostServices(): DesktopHostServices {
  return {
    getHostSettings(): Promise<AppSettings> {
      return invoke<AppSettings>('get_settings');
    },
    async saveHostSettings(s: AppSettings): Promise<void> {
      await invoke<void>('save_settings', { settings: s });
    },
    async rebuildMenu(language: string): Promise<void> {
      await invoke<void>('rebuild_menu', { language });
    },
    getLogPath(): Promise<string> {
      return invoke<string>('get_log_path');
    },
    openLogDir(): Promise<void> {
      return invoke<void>('open_log_dir');
    },
    openWorkflowsDir(): Promise<void> {
      return invoke<void>('open_workflows_dir');
    },
    openContextDir(): Promise<void> {
      return invoke<void>('open_context_dir');
    },
    checkUpdatesOnStartup(enabled: boolean): void {
      void maybeCheckOnStartup(enabled);
    },
  };
}
