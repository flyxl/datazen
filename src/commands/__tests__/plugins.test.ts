import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

import { PLUGINS_CHANGED_EVENT, pluginCommands } from '../plugins';
import { UI_PLUGIN_API_VERSION } from '../../types/plugin';

describe('pluginCommands', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it('listPlugins invokes list_plugins without args', async () => {
    invokeMock.mockResolvedValueOnce([]);
    await expect(pluginCommands.listPlugins()).resolves.toEqual([]);
    expect(invokeMock).toHaveBeenCalledWith('list_plugins');
  });

  it('getPluginManifest passes camelCase id', async () => {
    await pluginCommands.getPluginManifest('acme.demo');
    expect(invokeMock).toHaveBeenCalledWith('get_plugin_manifest', {
      id: 'acme.demo',
    });
  });

  it('installPluginFromPath passes path', async () => {
    await pluginCommands.installPluginFromPath('/tmp/acme.demo');
    expect(invokeMock).toHaveBeenCalledWith('install_plugin_from_path', {
      path: '/tmp/acme.demo',
    });
  });

  it('removePlugin and setPluginEnabled pass id/enabled', async () => {
    await pluginCommands.removePlugin('acme.demo');
    expect(invokeMock).toHaveBeenCalledWith('remove_plugin', {
      id: 'acme.demo',
    });

    await pluginCommands.setPluginEnabled('acme.demo', false);
    expect(invokeMock).toHaveBeenCalledWith('set_plugin_enabled', {
      id: 'acme.demo',
      enabled: false,
    });
  });

  it('storage commands namespace by pluginId with camelCase keys', async () => {
    await pluginCommands.pluginStorageGet('acme.demo', 'lastUid');
    expect(invokeMock).toHaveBeenCalledWith('plugin_storage_get', {
      pluginId: 'acme.demo',
      key: 'lastUid',
    });

    await pluginCommands.pluginStorageSet('acme.demo', 'lastUid', 42);
    expect(invokeMock).toHaveBeenCalledWith('plugin_storage_set', {
      pluginId: 'acme.demo',
      key: 'lastUid',
      value: 42,
    });

    await pluginCommands.pluginStorageRemove('acme.demo', 'lastUid');
    expect(invokeMock).toHaveBeenCalledWith('plugin_storage_remove', {
      pluginId: 'acme.demo',
      key: 'lastUid',
    });
  });

  it('readPluginFile returns byte array payload shape', async () => {
    invokeMock.mockResolvedValueOnce([60, 104, 116]);
    await expect(pluginCommands.readPluginFile('acme.demo', 'index.html')).resolves.toEqual([
      60, 104, 116,
    ]);
    expect(invokeMock).toHaveBeenCalledWith('read_plugin_file', {
      id: 'acme.demo',
      relativePath: 'index.html',
    });
  });

  it('exposes the Rust event name and API version contract', () => {
    expect(PLUGINS_CHANGED_EVENT).toBe('plugins:changed');
    expect(UI_PLUGIN_API_VERSION).toBe(2);
  });

  // --- F3 supplementary (test agent): payload passthrough ---

  it('passes install and manifest payloads through untouched', async () => {
    const summary = {
      id: 'acme.demo',
      name: 'Demo Plugin',
      version: '1.0.0',
      apiVersion: 2,
      enabled: true,
      permissions: ['storage:local'],
      pages: [],
      themes: [],
    };
    invokeMock.mockResolvedValueOnce(summary);
    await expect(pluginCommands.installPluginFromPath('/tmp/acme')).resolves.toBe(summary);
    expect(invokeMock).toHaveBeenLastCalledWith('install_plugin_from_path', { path: '/tmp/acme' });

    const manifest = { id: 'acme.demo', apiVersion: 2, permissions: [] };
    invokeMock.mockResolvedValueOnce(manifest);
    await expect(pluginCommands.getPluginManifest('acme.demo')).resolves.toBe(manifest);
  });

  it('pluginStorageGet returns the raw storage value without wrapping', async () => {
    const value = { nested: [1, 'a'] };
    invokeMock.mockResolvedValueOnce(value);
    await expect(pluginCommands.pluginStorageGet('acme.demo', 'k')).resolves.toBe(value);
    expect(invokeMock).toHaveBeenLastCalledWith('plugin_storage_get', {
      pluginId: 'acme.demo',
      key: 'k',
    });
  });
});
