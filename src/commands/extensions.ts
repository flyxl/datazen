import { invoke } from '@tauri-apps/api/core';
import type { ExtensionManifest, ExtensionSummary } from '../types/extension';

/** Mirrors `EXTENSIONS_CHANGED_EVENT` in `src-tauri/src/commands/extensions.rs`. */
export const EXTENSIONS_CHANGED_EVENT = 'plugins:changed';

export type ExtensionPackageKind = 'zip' | 'folder';

/** Preview returned after a native pick + validate-only inspect (no install). */
export interface ExtensionPackagePreview {
  pickToken: string;
  packageLabel: string;
  manifest: ExtensionManifest;
}

export const extensionCommands = {
  listExtensions: () => invoke<ExtensionSummary[]>('list_extensions'),

  getExtensionManifest: (id: string) => invoke<ExtensionManifest>('get_extension_manifest', { id }),

  /**
   * Native picker + validate-only inspect. Returns `null` when cancelled.
   * The wire-level `overridePath` escape hatch is webdriver/E2E-only.
   */
  inspectExtensionPackageWithDialog: (
    packageKind: ExtensionPackageKind,
    overridePath?: string,
  ) =>
    invoke<ExtensionPackagePreview | null>('inspect_extension_package_with_dialog', {
      packageKind,
      overridePath,
    }),

  /**
   * Install a package picked via {@link inspectExtensionPackageWithDialog}.
   * The wire-level `overridePath` escape hatch is webdriver/E2E-only.
   */
  installExtension: (pickToken: string, overridePath?: string) =>
    invoke<ExtensionSummary>('install_extension', { pickToken, overridePath }),

  removeExtension: (id: string) => invoke<void>('remove_extension', { id }),

  setExtensionEnabled: (id: string, enabled: boolean) =>
    invoke<void>('set_extension_enabled', { id, enabled }),

  extensionStorageGet: (extensionId: string, key: string) =>
    invoke<unknown>('extension_storage_get', { extensionId, key }),

  extensionStorageSet: (extensionId: string, key: string, value: unknown) =>
    invoke<void>('extension_storage_set', { extensionId, key, value }),

  extensionStorageRemove: (extensionId: string, key: string) =>
    invoke<void>('extension_storage_remove', { extensionId, key }),

  readExtensionFile: (id: string, relativePath: string) =>
    invoke<number[]>('read_extension_file', { id, relativePath }),

  /**
   * Fire-and-forget audit entry for extension-initiated sensitive calls. Lands
   * in `{dataDir}/logs/datazen.log` via the Rust `tracing` file sink. The
   * detail string must never contain argument contents — callers pass only
   * the command name and target connection id.
   */
  auditLog: (pluginId: string, event: string, detail: string): void => {
    void invoke('extension_audit_log', { pluginId, event, detail }).catch(() => {
      /* audit is best-effort; IPC being down must not break the call path */
    });
  },
};
