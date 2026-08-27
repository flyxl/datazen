import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

import { EXTENSIONS_CHANGED_EVENT, extensionCommands } from '../extensions';
import { EXTENSION_API_VERSION } from '../../types/extension';

describe('extensionCommands', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it('listExtensions invokes list_extensions without args', async () => {
    invokeMock.mockResolvedValueOnce([]);
    await expect(extensionCommands.listExtensions()).resolves.toEqual([]);
    expect(invokeMock).toHaveBeenCalledWith('list_extensions');
  });

  it('getExtensionManifest passes camelCase id', async () => {
    await extensionCommands.getExtensionManifest('acme.demo');
    expect(invokeMock).toHaveBeenCalledWith('get_extension_manifest', {
      id: 'acme.demo',
    });
  });

  it('installExtensionFromPath passes path', async () => {
    await extensionCommands.installExtensionFromPath('/tmp/acme.demo');
    expect(invokeMock).toHaveBeenCalledWith('install_extension_from_path', {
      path: '/tmp/acme.demo',
    });
  });

  it('removeExtension and setExtensionEnabled pass id/enabled', async () => {
    await extensionCommands.removeExtension('acme.demo');
    expect(invokeMock).toHaveBeenCalledWith('remove_extension', {
      id: 'acme.demo',
    });

    await extensionCommands.setExtensionEnabled('acme.demo', false);
    expect(invokeMock).toHaveBeenCalledWith('set_extension_enabled', {
      id: 'acme.demo',
      enabled: false,
    });
  });

  it('storage commands namespace by extensionId with camelCase keys', async () => {
    await extensionCommands.extensionStorageGet('acme.demo', 'lastUid');
    expect(invokeMock).toHaveBeenCalledWith('extension_storage_get', {
      extensionId: 'acme.demo',
      key: 'lastUid',
    });

    await extensionCommands.extensionStorageSet('acme.demo', 'lastUid', 42);
    expect(invokeMock).toHaveBeenCalledWith('extension_storage_set', {
      extensionId: 'acme.demo',
      key: 'lastUid',
      value: 42,
    });

    await extensionCommands.extensionStorageRemove('acme.demo', 'lastUid');
    expect(invokeMock).toHaveBeenCalledWith('extension_storage_remove', {
      extensionId: 'acme.demo',
      key: 'lastUid',
    });
  });

  it('readExtensionFile returns byte array payload shape', async () => {
    invokeMock.mockResolvedValueOnce([60, 104, 116]);
    await expect(extensionCommands.readExtensionFile('acme.demo', 'index.html')).resolves.toEqual([
      60, 104, 116,
    ]);
    expect(invokeMock).toHaveBeenCalledWith('read_extension_file', {
      id: 'acme.demo',
      relativePath: 'index.html',
    });
  });

  it('exposes the Rust event name and API version contract', () => {
    expect(EXTENSIONS_CHANGED_EVENT).toBe('plugins:changed');
    expect(EXTENSION_API_VERSION).toBe(2);
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
    await expect(extensionCommands.installExtensionFromPath('/tmp/acme')).resolves.toBe(summary);
    expect(invokeMock).toHaveBeenLastCalledWith('install_extension_from_path', { path: '/tmp/acme' });

    const manifest = { id: 'acme.demo', apiVersion: 2, permissions: [] };
    invokeMock.mockResolvedValueOnce(manifest);
    await expect(extensionCommands.getExtensionManifest('acme.demo')).resolves.toBe(manifest);
  });

  it('extensionStorageGet returns the raw storage value without wrapping', async () => {
    const value = { nested: [1, 'a'] };
    invokeMock.mockResolvedValueOnce(value);
    await expect(extensionCommands.extensionStorageGet('acme.demo', 'k')).resolves.toBe(value);
    expect(invokeMock).toHaveBeenLastCalledWith('extension_storage_get', {
      extensionId: 'acme.demo',
      key: 'k',
    });
  });
});
