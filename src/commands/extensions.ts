import { invoke } from '@tauri-apps/api/core';
import type { ExtensionManifest, ExtensionSummary } from '../types/extension';

/** Mirrors `EXTENSIONS_CHANGED_EVENT` in `src-tauri/src/commands/extensions.rs`. */
export const EXTENSIONS_CHANGED_EVENT = 'plugins:changed';

export const extensionCommands = {
  listExtensions: () => invoke<ExtensionSummary[]>('list_extensions'),

  getExtensionManifest: (id: string) => invoke<ExtensionManifest>('get_extension_manifest', { id }),

  /** Validates a package without installing; returns its manifest for review. */
  inspectExtensionPackage: (path: string) =>
    invoke<ExtensionManifest>('inspect_extension_package', { path }),

  installExtensionFromPath: (path: string) =>
    invoke<ExtensionSummary>('install_extension_from_path', { path }),

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
