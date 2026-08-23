import { invoke } from '@tauri-apps/api/core';
import type { PluginManifest, PluginSummary } from '../types/plugin';

/** Mirrors `PLUGINS_CHANGED_EVENT` in `src-tauri/src/commands/plugins.rs`. */
export const PLUGINS_CHANGED_EVENT = 'plugins:changed';

export const pluginCommands = {
  listPlugins: () => invoke<PluginSummary[]>('list_plugins'),

  getPluginManifest: (id: string) => invoke<PluginManifest>('get_plugin_manifest', { id }),

  /** Validates a package without installing; returns its manifest for review. */
  inspectPluginPackage: (path: string) =>
    invoke<PluginManifest>('inspect_plugin_package', { path }),

  installPluginFromPath: (path: string) =>
    invoke<PluginSummary>('install_plugin_from_path', { path }),

  removePlugin: (id: string) => invoke<void>('remove_plugin', { id }),

  setPluginEnabled: (id: string, enabled: boolean) =>
    invoke<void>('set_plugin_enabled', { id, enabled }),

  pluginStorageGet: (pluginId: string, key: string) =>
    invoke<unknown>('plugin_storage_get', { pluginId, key }),

  pluginStorageSet: (pluginId: string, key: string, value: unknown) =>
    invoke<void>('plugin_storage_set', { pluginId, key, value }),

  pluginStorageRemove: (pluginId: string, key: string) =>
    invoke<void>('plugin_storage_remove', { pluginId, key }),

  readPluginFile: (id: string, relativePath: string) =>
    invoke<number[]>('read_plugin_file', { id, relativePath }),

  /**
   * Fire-and-forget audit entry for plugin-initiated sensitive calls. Lands
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
