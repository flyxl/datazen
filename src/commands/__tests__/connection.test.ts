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
    });
    const payload = invokeMock.mock.calls[0]?.[1] as { config: ConnectionConfig };
    expect(() => JSON.stringify(payload.config)).not.toThrow();
  });
});
