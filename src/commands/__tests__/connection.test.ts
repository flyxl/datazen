import { describe, it, expect, vi, beforeEach } from 'vitest';
import type { ConnectionConfig } from '../../types';

const invokeMock = vi.fn();

vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...args: unknown[]) => invokeMock(...args),
}));

import { connectionCommands } from '../connection';

describe('connectionCommands IPC boundary', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue({ serverVersion: '16.0', serverType: 'postgresql' });
  });

  it('testConnection passes a plain cloned config to invoke', async () => {
    const config: ConnectionConfig = {
      id: 'c1',
      name: 'Local PG',
      databaseType: 'postgresql',
      host: '127.0.0.1',
      port: 5432,
      database: 'postgres',
      sslMode: 'prefer',
      group: 'preset:development',
    };

    await connectionCommands.testConnection(config);

    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith('test_connection', {
      config: expect.objectContaining({
        id: 'c1',
        name: 'Local PG',
        group: 'preset:development',
      }),
    });
    const payload = invokeMock.mock.calls[0]?.[1] as { config: ConnectionConfig };
    expect(() => JSON.stringify(payload.config)).not.toThrow();
    expect(payload.config).not.toBe(config);
  });

  it('testConnection rejects cyclic group payloads before invoke', async () => {
    const cyclicGroup: Record<string, unknown> = {};
    cyclicGroup.self = cyclicGroup;
    const bad = {
      id: 'c1',
      name: 'Bad',
      databaseType: 'postgresql',
      sslMode: 'prefer',
      group: cyclicGroup,
    } as unknown as ConnectionConfig;

    expect(() => connectionCommands.testConnection(bad)).toThrow(/cycle/);
    expect(invokeMock).not.toHaveBeenCalled();
  });

  it('saveConnection passes a plain cloned config to invoke', async () => {
    const config: ConnectionConfig = {
      id: 'c2',
      name: 'Save me',
      databaseType: 'mysql',
      host: '127.0.0.1',
      port: 3306,
      sslMode: 'disable',
    };

    await connectionCommands.saveConnection(config);

    expect(invokeMock).toHaveBeenCalledWith('save_connection', {
      config: expect.objectContaining({ id: 'c2', databaseType: 'mysql' }),
      clearMissingCredentials: false,
    });
    const payload = invokeMock.mock.calls[0]?.[1] as { config: ConnectionConfig };
    expect(() => JSON.stringify(payload.config)).not.toThrow();
  });

  it('forwards the explicit request to clear blank saved credentials', async () => {
    const config: ConnectionConfig = {
      id: 'c3',
      name: 'Clear credentials',
      databaseType: 'postgresql',
      sslMode: 'prefer',
    };

    await connectionCommands.saveConnection(config, true);

    expect(invokeMock).toHaveBeenCalledWith('save_connection', {
      config: expect.objectContaining({ id: 'c3' }),
      clearMissingCredentials: true,
    });
  });

  it('duplicates a connection through the backend without returning its credentials', async () => {
    await connectionCommands.duplicateConnection('source', 'copy', 'Copy');
    expect(invokeMock).toHaveBeenCalledWith('duplicate_connection', {
      sourceId: 'source',
      newId: 'copy',
      newName: 'Copy',
    });
  });
});

describe('connectionCommands.connect in-flight de-duplication', () => {
  beforeEach(() => {
    invokeMock.mockReset();
  });

  it('joins concurrent callers for the same connectionId into one invoke', async () => {
    // Left pending on purpose: the resolution happens only when the tab path
    // and the store path are both already in flight.
    let release!: (value: { connectionId: string; dbSessionId: string }) => void;
    invokeMock.mockImplementation(
      () =>
        new Promise((resolve) => {
          release = resolve;
        }),
    );

    // The right-click "open" race: selection opens a tab (connectTab → direct
    // IPC) while the menu handler also fires store.connect → same command.
    const tabPath = connectionCommands.connect('cfg-race');
    const storePath = connectionCommands.connect('cfg-race');

    // The command awaits its E2E-delay prologue first, so the actual invoke
    // lands in a microtask — wait for it before resolving the backend.
    await vi.waitFor(() => expect(invokeMock).toHaveBeenCalledTimes(1));

    release({ connectionId: 'cfg-race', dbSessionId: 'sess-one' });

    const [viaTab, viaStore] = await Promise.all([tabPath, storePath]);
    expect(invokeMock).toHaveBeenCalledTimes(1);
    expect(invokeMock).toHaveBeenCalledWith('connect', { connectionId: 'cfg-race' });
    expect(viaTab.dbSessionId).toBe('sess-one');
    expect(viaStore.dbSessionId).toBe('sess-one');
  });

  it('frees the slot after settling so a retry issues a fresh invoke', async () => {
    invokeMock.mockResolvedValueOnce({ connectionId: 'cfg-retry', dbSessionId: 'sess-1' });
    invokeMock.mockResolvedValueOnce({ connectionId: 'cfg-retry', dbSessionId: 'sess-2' });

    await connectionCommands.connect('cfg-retry');
    const second = await connectionCommands.connect('cfg-retry');

    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(second.dbSessionId).toBe('sess-2');
  });

  it('shares one rejection with all joiners and still allows a later retry', async () => {
    invokeMock.mockRejectedValueOnce(new Error('backend unreachable'));
    invokeMock.mockResolvedValueOnce({ connectionId: 'cfg-fail', dbSessionId: 'sess-ok' });

    const first = connectionCommands.connect('cfg-fail');
    const joiner = connectionCommands.connect('cfg-fail');

    await expect(first).rejects.toThrow('backend unreachable');
    await expect(joiner).rejects.toThrow('backend unreachable');
    expect(invokeMock).toHaveBeenCalledTimes(1);

    const retry = await connectionCommands.connect('cfg-fail');
    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(retry.dbSessionId).toBe('sess-ok');
  });

  it('never de-duplicates distinct connectionIds', async () => {
    invokeMock.mockResolvedValue({ connectionId: 'x', dbSessionId: 'sess' });

    await Promise.all([connectionCommands.connect('cfg-a'), connectionCommands.connect('cfg-b')]);

    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(invokeMock).toHaveBeenNthCalledWith(1, 'connect', { connectionId: 'cfg-a' });
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'connect', { connectionId: 'cfg-b' });
  });

  it('leaves connectDedicated untouched: sub-windows keep isolated sessions', async () => {
    invokeMock.mockResolvedValue({ connectionId: 'cfg-sub', dbSessionId: 'sess-dedicated' });

    await Promise.all([
      connectionCommands.connectDedicated('cfg-sub'),
      connectionCommands.connectDedicated('cfg-sub'),
    ]);

    expect(invokeMock).toHaveBeenCalledTimes(2);
    expect(invokeMock).toHaveBeenNthCalledWith(1, 'connect_dedicated', {
      connectionId: 'cfg-sub',
      database: null,
    });
    expect(invokeMock).toHaveBeenNthCalledWith(2, 'connect_dedicated', {
      connectionId: 'cfg-sub',
      database: null,
    });
  });
});
