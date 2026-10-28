/**
 * Desktop host overlay services (form-split F2, port 4).
 *
 * These capabilities are desktop-only: the host settings pair
 * (`get_host_settings` / `save_host_settings` — the full desktop field set,
 * renamed from the legacy `get_settings` / `save_settings` in F4 so the
 * neutral kernel names could serve the shared contract), the menu rebuild and
 * the host directory openers. The implementation is exported from this
 * desktop zone only; shared code never sees it by name (ruling #7).
 *
 * The settings pair feeds the F4 `DesktopSettingsClient`
 * (`./settingsClient.ts`), which projects the host blob onto the shared
 * contract; F5 moves the updater entry behind capabilities.
 */

import { invoke } from '@tauri-apps/api/core';
import { getVersion } from '@tauri-apps/api/app';
import type { AppSettings } from '../../types';
import type { DesktopHostServices } from '../contracts';
import { maybeCheckOnStartup } from './updater';

export function createDesktopHostServices(): DesktopHostServices {
  return {
    getHostSettings(): Promise<AppSettings> {
      return invoke<AppSettings>('get_host_settings');
    },
    async saveHostSettings(s: AppSettings): Promise<void> {
      await invoke<void>('save_host_settings', { settings: s });
    },
    async rebuildMenu(language: string): Promise<void> {
      await invoke<void>('rebuild_menu', { language });
    },
    getLogPath(): Promise<string> {
      return invoke<string>('get_log_path');
    },
    getAppExecutablePath(): Promise<string> {
      return invoke<string>('get_app_executable_path');
    },
    async openPath(path: string): Promise<void> {
      await invoke<void>('open_path', { path });
    },
    async openLogDir(): Promise<void> {
      await invoke<void>('open_known_dir', { kind: 'log' });
    },
    async openWorkflowsDir(): Promise<void> {
      await invoke<void>('open_known_dir', { kind: 'workflows' });
    },
    async openContextDir(): Promise<void> {
      await invoke<void>('open_known_dir', { kind: 'context' });
    },
    checkUpdatesOnStartup(enabled: boolean): void {
      void maybeCheckOnStartup(enabled);
    },
    async getAppVersion(): Promise<string> {
      // Zone-local import surface: the version read belongs to the host app
      // bundle, and `@tauri-apps/api/app` may only be imported inside the
      // desktop zone (ruling #5).
      return getVersion();
    },
    persistSurfaceBackground(hex: string, dark: boolean): void {
      // Best-effort host-file persistence for the next window's init-script
      // bake — fire and forget, failures only lose the paint continuity.
      void invoke('set_surface_background', { hex, dark }).catch(() => {
        // Cache write is best-effort; next window still has in-memory Rust
        // state if the IPC reached `set`.
      });
    },
  };
}
