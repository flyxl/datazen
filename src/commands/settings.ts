import { toSharedSettings, useBackendClient, type SharedSettings } from '@datazen/backend-client';
import { requireHostSettings } from '../platform/contracts';
import type { AppSettings } from '../types';

/**
 * Settings commands, form-split F4 shape: **no form-factor branch**.
 *
 * The shared half rides the kernel track (`BackendClient.getSettings()` /
 * `saveSettings()` — nested `Settings` wire); the host half rides the
 * `HostSettingsPort` (desktop: the projecting `DesktopSettingsClient`; web:
 * the inert port). The three `backendId === 'desktop'` ternaries this file
 * used to carry are gone — what used to be "local" is now a port, and the
 * desktop adapter owns the projection internally.
 */

/** Flatten the nested shared wire into the store's flat `AppSettings` view. */
function flattenShared(shared: SharedSettings): Partial<AppSettings> {
  return {
    ...shared.preferences,
    ...shared.execution,
    monitor: shared.monitor,
    onboarding: shared.onboarding ?? undefined,
    rowCountMode: shared.rowCountMode,
    sqlSyntaxTheme: shared.preferences.sqlSyntaxTheme ?? undefined,
  };
}

export const settingsCommands = {
  getSettings: async (): Promise<Partial<AppSettings>> => {
    // Both halves in parallel; extras merge *under* the shared contract so the
    // kernel stays the authority for every field it owns.
    const [shared, extras] = await Promise.all([
      useBackendClient().getSettings(),
      requireHostSettings().readHostExtras(),
    ]);
    return { ...extras, ...flattenShared(shared) };
  },

  saveSettings: async (settings: AppSettings): Promise<Partial<AppSettings>> => {
    // Desktop: the host write persists the full blob (shared half normalized
    // inside, monitor/tray side effects included); web: a no-op, and the
    // kernel save below is the whole write. Either way the kernel save
    // follows so both tracks agree on the shared contract.
    await requireHostSettings().writeHostSettings(settings);
    const saved = await useBackendClient().saveSettings(toSharedSettings(settings));
    return flattenShared(saved);
  },

  syncLanguageMenu: async (language: string): Promise<void> => {
    await requireHostSettings().rebuildMenu(language);
  },

  getLogPath: () => requireHostSettings().getLogPath(),

  getAppExecutablePath: () => requireHostSettings().getAppExecutablePath(),

  /** @deprecated E2E-only; prefer openLogDir / openWorkflowsDir / openContextDir (one IPC: open_known_dir). */
  openPath: (path: string) => requireHostSettings().openPath(path),

  openLogDir: () => requireHostSettings().openLogDir(),

  openWorkflowsDir: () => requireHostSettings().openWorkflowsDir(),

  openContextDir: () => requireHostSettings().openContextDir(),
};
