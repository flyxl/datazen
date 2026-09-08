import { invoke } from '@tauri-apps/api/core';
import type { WappManifest, WappSummary } from '../types/wapp';

/** Mirrors `WAPPS_CHANGED_EVENT` in `src-tauri/src/commands/wapps.rs`. */
export const WAPPS_CHANGED_EVENT = 'wapps:changed';

export type WappPackageKind = 'zip' | 'folder';

/** Preview returned after a native pick + validate-only inspect (no install). */
export interface WappPackagePreview {
  pickToken: string;
  packageLabel: string;
  manifest: WappManifest;
}

export const wappCommands = {
  listWapps: () => invoke<WappSummary[]>('list_wapps'),

  getWappManifest: (id: string) => invoke<WappManifest>('get_wapp_manifest', { id }),

  /**
   * Native picker + validate-only inspect. Returns `null` when cancelled.
   * The wire-level `overridePath` escape hatch is webdriver/E2E-only.
   */
  inspectWappPackageWithDialog: (packageKind: WappPackageKind, overridePath?: string) =>
    invoke<WappPackagePreview | null>('inspect_wapp_package_with_dialog', {
      packageKind,
      overridePath,
    }),

  /**
   * Install a package picked via {@link inspectWappPackageWithDialog}.
   * The wire-level `overridePath` escape hatch is webdriver/E2E-only.
   */
  installWapp: (pickToken: string, overridePath?: string) =>
    invoke<WappSummary>('install_wapp', { pickToken, overridePath }),

  removeWapp: (id: string) => invoke<void>('remove_wapp', { id }),

  setWappEnabled: (id: string, enabled: boolean) =>
    invoke<void>('set_wapp_enabled', { id, enabled }),

  wappStorageGet: (wappId: string, key: string) =>
    invoke<unknown>('wapp_storage_get', { wappId, key }),

  wappStorageSet: (wappId: string, key: string, value: unknown) =>
    invoke<void>('wapp_storage_set', { wappId, key, value }),

  wappStorageRemove: (wappId: string, key: string) =>
    invoke<void>('wapp_storage_remove', { wappId, key }),

  readWappFile: (id: string, relativePath: string) =>
    invoke<number[]>('read_wapp_file', { id, relativePath }),

  /**
   * Fire-and-forget audit entry for wapp-initiated sensitive calls. Lands
   * in `{dataDir}/logs/datazen.log` via the Rust `tracing` file sink. The
   * detail string must never contain argument contents — callers pass only
   * the command name and target connection id.
   */
  auditLog: (wappId: string, event: string, detail: string): void => {
    void invoke('wapp_audit_log', { wappId, event, detail }).catch(() => {
      /* audit is best-effort; IPC being down must not break the call path */
    });
  },

  // Legacy method aliases for backward compatibility
  listExtensions: () => invoke<WappSummary[]>('list_wapps'),
  getExtensionManifest: (id: string) => invoke<WappManifest>('get_wapp_manifest', { id }),
  inspectExtensionPackageWithDialog: (packageKind: WappPackageKind, overridePath?: string) =>
    invoke<WappPackagePreview | null>('inspect_wapp_package_with_dialog', {
      packageKind,
      overridePath,
    }),
  installExtension: (pickToken: string, overridePath?: string) =>
    invoke<WappSummary>('install_wapp', { pickToken, overridePath }),
  removeExtension: (id: string) => invoke<void>('remove_wapp', { id }),
  setExtensionEnabled: (id: string, enabled: boolean) =>
    invoke<void>('set_wapp_enabled', { id, enabled }),
  extensionStorageGet: (extensionId: string, key: string) =>
    invoke<unknown>('wapp_storage_get', { wappId: extensionId, key }),
  extensionStorageSet: (extensionId: string, key: string, value: unknown) =>
    invoke<void>('wapp_storage_set', { wappId: extensionId, key, value }),
  extensionStorageRemove: (extensionId: string, key: string) =>
    invoke<void>('wapp_storage_remove', { wappId: extensionId, key }),
  readExtensionFile: (id: string, relativePath: string) =>
    invoke<number[]>('read_wapp_file', { id, relativePath }),
};

// ---------------------------------------------------------------------------
// Backward-compatible aliases for legacy Extension naming
// ---------------------------------------------------------------------------
export const EXTENSIONS_CHANGED_EVENT = WAPPS_CHANGED_EVENT;
export type ExtensionPackageKind = WappPackageKind;
export type ExtensionPackagePreview = WappPackagePreview;
export const extensionCommands = wappCommands;
