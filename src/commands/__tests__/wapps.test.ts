import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

import { WAPPS_CHANGED_EVENT, wappCommands } from '../wapps';
import { WAPP_API_VERSION } from '../../types/wapp';

describe('wappCommands', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it('listWapps invokes list_wapps without args', async () => {
    invokeMock.mockResolvedValueOnce([]);
    await expect(wappCommands.listWapps()).resolves.toEqual([]);
    expect(invokeMock).toHaveBeenCalledWith('list_wapps');
  });

  it('getWappManifest passes camelCase id', async () => {
    await wappCommands.getWappManifest('acme.demo');
    expect(invokeMock).toHaveBeenCalledWith('get_wapp_manifest', {
      id: 'acme.demo',
    });
  });

  it('inspectWappPackageWithDialog passes packageKind and omits overridePath in production', async () => {
    await wappCommands.inspectWappPackageWithDialog('folder');
    expect(invokeMock).toHaveBeenCalledWith('inspect_wapp_package_with_dialog', {
      packageKind: 'folder',
      overridePath: undefined,
    });
  });

  it('installWapp passes pickToken and omits overridePath in production', async () => {
    await wappCommands.installWapp('pick-token-1');
    expect(invokeMock).toHaveBeenCalledWith('install_wapp', {
      pickToken: 'pick-token-1',
      overridePath: undefined,
    });
  });

  it('removeWapp and setWappEnabled pass id/enabled', async () => {
    await wappCommands.removeWapp('acme.demo');
    expect(invokeMock).toHaveBeenCalledWith('remove_wapp', {
      id: 'acme.demo',
    });

    await wappCommands.setWappEnabled('acme.demo', false);
    expect(invokeMock).toHaveBeenCalledWith('set_wapp_enabled', {
      id: 'acme.demo',
      enabled: false,
    });
  });

  it('storage commands namespace by wappId with camelCase keys', async () => {
    await wappCommands.wappStorageGet('acme.demo', 'lastUid');
    expect(invokeMock).toHaveBeenCalledWith('wapp_storage_get', {
      wappId: 'acme.demo',
      key: 'lastUid',
    });

    await wappCommands.wappStorageSet('acme.demo', 'lastUid', 42);
    expect(invokeMock).toHaveBeenCalledWith('wapp_storage_set', {
      wappId: 'acme.demo',
      key: 'lastUid',
      value: 42,
    });

    await wappCommands.wappStorageRemove('acme.demo', 'lastUid');
    expect(invokeMock).toHaveBeenCalledWith('wapp_storage_remove', {
      wappId: 'acme.demo',
      key: 'lastUid',
    });
  });

  it('readWappFile returns byte array payload shape', async () => {
    invokeMock.mockResolvedValueOnce([60, 104, 116]);
    await expect(wappCommands.readWappFile('acme.demo', 'index.html')).resolves.toEqual([
      60, 104, 116,
    ]);
    expect(invokeMock).toHaveBeenCalledWith('read_wapp_file', {
      id: 'acme.demo',
      relativePath: 'index.html',
    });
  });

  it('exposes the Rust event name and API version contract', () => {
    expect(WAPPS_CHANGED_EVENT).toBe('wapps:changed');
    expect(WAPP_API_VERSION).toBe(2);
  });

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
    await expect(wappCommands.installWapp('token-abc')).resolves.toBe(summary);
    expect(invokeMock).toHaveBeenLastCalledWith('install_wapp', {
      pickToken: 'token-abc',
      overridePath: undefined,
    });

    const manifest = { id: 'acme.demo', apiVersion: 2, permissions: [] };
    invokeMock.mockResolvedValueOnce(manifest);
    await expect(wappCommands.getWappManifest('acme.demo')).resolves.toBe(manifest);
  });

  it('wappStorageGet returns the raw storage value without wrapping', async () => {
    const value = { nested: [1, 'a'] };
    invokeMock.mockResolvedValueOnce(value);
    await expect(wappCommands.wappStorageGet('acme.demo', 'k')).resolves.toBe(value);
    expect(invokeMock).toHaveBeenLastCalledWith('wapp_storage_get', {
      wappId: 'acme.demo',
      key: 'k',
    });
  });
});
