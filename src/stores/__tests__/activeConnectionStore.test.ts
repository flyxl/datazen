import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { ConnectionConfig } from '../../types';

const mockConnectionCommands = {
  connect: vi.fn(),
  testConnection: vi.fn(),
  disconnect: vi.fn(),
};

vi.mock('../../commands/connection', () => ({
  connectionCommands: mockConnectionCommands,
}));

vi.mock('../../lib/crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
}));

function makeConfig(overrides: Partial<ConnectionConfig> = {}): ConnectionConfig {
  return {
    id: 'cfg-1',
    name: 'Test',
    databaseType: 'postgresql',
    database: 'mydb',
    ...overrides,
  };
}

describe('activeConnectionStore', () => {
  let useActiveConnectionStore: typeof import('../activeConnectionStore').useActiveConnectionStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const mod = await import('../activeConnectionStore');
    useActiveConnectionStore = mod.useActiveConnectionStore;
    useActiveConnectionStore.getState().reset();
  });

  it('connect succeeds and marks connected', async () => {
    mockConnectionCommands.connect.mockResolvedValueOnce('pool-abc');
    mockConnectionCommands.testConnection.mockResolvedValueOnce({ version: '16' });

    await useActiveConnectionStore.getState().connect(makeConfig());

    const entry = useActiveConnectionStore.getState().connections['cfg-1'];
    expect(entry.status).toBe('connected');
    expect(entry.connectionId).toBe('pool-abc');
    expect(entry.serverInfo).toEqual({ version: '16' });
    expect(entry.currentDatabase).toBe('mydb');
  });

  it('connect failure marks error', async () => {
    mockConnectionCommands.connect.mockRejectedValueOnce('connection refused');

    await useActiveConnectionStore.getState().connect(makeConfig());

    const entry = useActiveConnectionStore.getState().connections['cfg-1'];
    expect(entry.status).toBe('error');
    expect(entry.error).toBe('connection refused');
  });

  it('markConnecting / markConnected / markError', () => {
    useActiveConnectionStore.getState().markConnecting('cfg-2', 'db2');
    expect(useActiveConnectionStore.getState().connections['cfg-2'].status).toBe('connecting');

    useActiveConnectionStore.getState().markConnected('cfg-2', 'pool-xyz');
    expect(useActiveConnectionStore.getState().connections['cfg-2'].status).toBe('connected');
    expect(useActiveConnectionStore.getState().connections['cfg-2'].connectionId).toBe('pool-xyz');

    useActiveConnectionStore.getState().markError('cfg-2', 'timeout');
    expect(useActiveConnectionStore.getState().connections['cfg-2'].status).toBe('error');
    expect(useActiveConnectionStore.getState().connections['cfg-2'].error).toBe('timeout');
  });

  it('disconnect removes entry when no connectionId', async () => {
    useActiveConnectionStore.getState().markConnecting('cfg-3', null);
    await useActiveConnectionStore.getState().disconnect('cfg-3');
    expect(useActiveConnectionStore.getState().connections['cfg-3']).toBeUndefined();
  });

  it('disconnect calls backend and removes entry', async () => {
    mockConnectionCommands.connect.mockResolvedValueOnce('pool-1');
    mockConnectionCommands.testConnection.mockResolvedValueOnce({});
    await useActiveConnectionStore.getState().connect(makeConfig({ id: 'cfg-4' }));

    mockConnectionCommands.disconnect.mockResolvedValueOnce(undefined);
    await useActiveConnectionStore.getState().disconnect('cfg-4');
    expect(mockConnectionCommands.disconnect).toHaveBeenCalledWith('pool-1');
    expect(useActiveConnectionStore.getState().connections['cfg-4']).toBeUndefined();
  });

  it('disconnect still removes entry when backend fails', async () => {
    useActiveConnectionStore.setState({
      connections: {
        'cfg-5': {
          connectionId: 'pool-5',
          configId: 'cfg-5',
          status: 'connected',
          serverInfo: null,
          currentDatabase: null,
          error: null,
        },
      },
    });
    mockConnectionCommands.disconnect.mockRejectedValueOnce(new Error('fail'));
    await useActiveConnectionStore.getState().disconnect('cfg-5');
    expect(useActiveConnectionStore.getState().connections['cfg-5']).toBeUndefined();
  });

  it('removeByConnectionId removes matching entries', () => {
    useActiveConnectionStore.setState({
      connections: {
        a: { connectionId: 'pool-a', configId: 'a', status: 'connected', serverInfo: null, currentDatabase: null, error: null },
        b: { connectionId: 'pool-b', configId: 'b', status: 'connected', serverInfo: null, currentDatabase: null, error: null },
      },
    });
    useActiveConnectionStore.getState().removeByConnectionId('pool-a');
    expect(useActiveConnectionStore.getState().connections['a']).toBeUndefined();
    expect(useActiveConnectionStore.getState().connections['b']).toBeDefined();
  });

  it('reset clears all connections', () => {
    useActiveConnectionStore.getState().markConnecting('x', null);
    useActiveConnectionStore.getState().reset();
    expect(useActiveConnectionStore.getState().connections).toEqual({});
  });
});
