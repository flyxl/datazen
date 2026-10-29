/**
 * The desktop settings domain client (form-split F4 demonstration domain).
 *
 * The settings surface is split in two:
 *
 * - **Shared** — `SettingsClient` (backend-client F3 domain client) speaks the
 *   nested `Settings` wire over `BackendTransport`: `getSettings` /
 *   `saveSettings`, same signature in every form factor. On the desktop the
 *   transport maps them 1:1 onto the kernel commands `get_settings` /
 *   `save_settings` (renamed from `get_shared_settings` / `save_shared_settings`
 *   in this phase).
 * - **Host** — this file. The desktop settings file carries fields beyond the
 *   shared contract (paths, process, updater, grid appearance), and the
 *   `DesktopSettingsClient` owns them: it adapts `DesktopHostServices` onto
 *   `HostSettingsPort` and does the projection internally, so
 *   `src/commands/settings.ts` has no `backendId === 'desktop'` branch left.
 *
 * Field-ownership review (plan §5.4): the three *behavior* fields
 * (`monitor`, `onboarding`, `rowCountMode`) migrated into the shared
 * `Settings`; the path / process / updater fields stay here in
 * `DesktopSettings`. The grid-appearance family stays host-side for now —
 * F5's capabilities table is what makes those sections form-aware.
 */

import type { AppSettings } from '../../types';
import type { DesktopHostServices, HostSettingsPort } from '../contracts';
import type { SharedSettings } from '@datazen/backend-client';

/**
 * The desktop settings file's shape: the shared contract plus the host-only
 * extras. `extends SharedSettings` is the plan's own typing — the superset a
 * web form never sees.
 */
export interface DesktopSettings extends SharedSettings {
  logLevel: AppSettings['logLevel'];
  logPath: string;
  mcpServerEnabled: boolean;
  mcpClientServers?: AppSettings['mcpClientServers'];
  contextDir: string;
  favoritesRoot?: string;
  checkForUpdatesOnStartup: boolean;
  rowCountEstimateThreshold: number;
  defaultTableSort: AppSettings['defaultTableSort'];
  gridDensity: AppSettings['gridDensity'];
  gridAlternatingRows: boolean;
  gridRowNumbers: boolean;
  gridAutoHideScrollbar: boolean;
}

/** The host-only keys — everything `DesktopSettings` adds over `SharedSettings`. */
const HOST_EXTRAS_KEYS = [
  'logLevel',
  'logPath',
  'mcpServerEnabled',
  'mcpClientServers',
  'contextDir',
  'favoritesRoot',
  'checkForUpdatesOnStartup',
  'rowCountEstimateThreshold',
  'defaultTableSort',
  'gridDensity',
  'gridAlternatingRows',
  'gridRowNumbers',
  'gridAutoHideScrollbar',
] as const;

/**
 * Project a `DesktopSettings` down to the shared `Settings`: the extras drop,
 * the three behavior fields stay. This is the projection the F4 plan names —
 * the desktop implementation answers the shared signature with the shared
 * shape, never leaking a host field through it.
 */
export function toSettings(desktop: DesktopSettings): SharedSettings {
  const { preferences, execution, monitor, onboarding, rowCountMode } = desktop;
  return { preferences, execution, monitor, onboarding, rowCountMode };
}

/**
 * Wrap the host's flat settings blob into the typed `DesktopSettings`
 * superset (the shared half projected out of the same file via
 * `toSharedSettings`, so both halves read one storage).
 */
export function toDesktopSettings(
  host: AppSettings,
  toSharedSettings: (s: AppSettings) => SharedSettings,
): DesktopSettings {
  const shared = toSharedSettings(host);
  const extras = pickHostExtras(host);
  // The host blob is the full file, so every `DesktopSettings` key is present
  // at runtime; the `Partial` spread cannot express that, and the cast is the
  // honest widening.
  return { ...shared, ...extras } as DesktopSettings;
}

/** The host-only slice of a flat settings blob. */
export function pickHostExtras(host: Partial<AppSettings>): Partial<AppSettings> {
  const out: Partial<AppSettings> = {};
  for (const key of HOST_EXTRAS_KEYS) {
    const value = host[key];
    if (value !== undefined) {
      (out as Record<string, unknown>)[key] = value;
    }
  }
  return out;
}

/**
 * Build the desktop host-settings adapter: `DesktopHostServices` (the raw
 * `get_host_settings` / `save_host_settings` pair and friends) presented as
 * the shared-code-facing `HostSettingsPort`, with the projection inside.
 */
export function createDesktopSettingsClient(host: DesktopHostServices): HostSettingsPort {
  return {
    async readHostExtras() {
      // One host read; the extras are the projection's residue.
      const blob = await host.getHostSettings();
      return pickHostExtras(blob);
    },

    async writeHostSettings(settings) {
      // The host write persists the full blob and normalizes the shared half
      // inside (Rust `save_host_settings` runs `SettingsService::normalize`),
      // so monitor/tray side effects fire exactly once per save.
      await host.saveHostSettings(settings);
    },

    async rebuildMenu(language) {
      await host.rebuildMenu(language);
    },

    getLogPath: () => host.getLogPath(),
    getAppExecutablePath: () => host.getAppExecutablePath(),
    openPath: (path) => host.openPath(path),
    openLogDir: () => host.openLogDir(),
    openWorkflowsDir: () => host.openWorkflowsDir(),
    openContextDir: () => host.openContextDir(),
    checkUpdatesOnStartup: (enabled) => host.checkUpdatesOnStartup(enabled),
    getAppVersion: () => host.getAppVersion(),
    persistSurfaceBackground: (hex, dark) => host.persistSurfaceBackground(hex, dark),
    checkForUpdates: () => host.checkForUpdates(),
    downloadAndInstallUpdate: (onProgress) => host.downloadAndInstallUpdate(onProgress),
  };
}
