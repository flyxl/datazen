import { describe, expect, it, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { useConnectionTabs } from '../useConnectionTabs';
import { PENDING_CONNECTION_KEY } from '../../../lib/windowManager';

const { stableT, connectMock, pingMock, activeStore, crossWindowHandlers } = vi.hoisted(() => ({
  stableT: (key: string) => key,
  connectMock: vi.fn(),
  pingMock: vi.fn().mockResolvedValue(true),
  activeStore: {
    connections: {} as Record<
      string,
      { status: string; dbSessionId?: string; error?: string }
    >,
    markConnecting: vi.fn(),
    markConnected: vi.fn(),
    markError: vi.fn(),
  },
  crossWindowHandlers: {} as Record<string, (payload: unknown) => void>,
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT }),
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    connect: (...args: unknown[]) => connectMock(...args),
    pingConnection: (...args: unknown[]) => pingMock(...args),
  },
}));

vi.mock('../../../stores/activeConnectionStore', () => ({
  useActiveConnectionStore: { getState: () => activeStore },
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  emitCrossWindow: vi.fn().mockResolvedValue(undefined),
  listenCrossWindow: vi.fn(async (event: string, handler: (payload: unknown) => void) => {
    crossWindowHandlers[event] = handler;
    return () => {
      delete crossWindowHandlers[event];
    };
  }),
}));

describe('[tester] useConnectionTabs', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    localStorage.clear();
    activeStore.connections = {};
    for (const key of Object.keys(crossWindowHandlers)) delete crossWindowHandlers[key];
  });

  it('bootstraps tab from pending localStorage connection', () => {
    localStorage.setItem(
      PENDING_CONNECTION_KEY,
      JSON.stringify({
        connectionId: 'cfg-1',
        connectionName: 'PG',
        databaseType: 'postgresql',
        action: 'focus-query',
      }),
    );
    const { result } = renderHook(() => useConnectionTabs());
    expect(result.current.tabs).toHaveLength(1);
    expect(result.current.tabs[0].connectionId).toBe('cfg-1');
    expect(result.current.pendingActionRef.current).toBe('focus-query');
  });

  it('connects pending tabs and marks connected on success', async () => {
    connectMock.mockResolvedValue('sess-1');
    const { result } = renderHook(() => useConnectionTabs());

    act(() => {
      result.current.setTabs([
        {
          connectionId: 'cfg-1',
          dbSessionId: '',
          connectionName: 'PG',
          databaseType: 'postgresql',
          status: 'connecting',
        },
      ]);
    });

    await waitFor(() => {
      expect(connectMock).toHaveBeenCalledWith('cfg-1');
      expect(activeStore.markConnected).toHaveBeenCalledWith('cfg-1', 'sess-1');
    });

    await waitFor(() => {
      expect(result.current.tabs[0]?.status).toBe('connected');
      expect(result.current.tabs[0]?.dbSessionId).toBe('sess-1');
    });
  });

  it('marks tab error when connect fails', async () => {
    connectMock.mockRejectedValue(new Error('connect failed'));
    const { result } = renderHook(() => useConnectionTabs());

    act(() => {
      result.current.setTabs([
        {
          connectionId: 'cfg-1',
          dbSessionId: '',
          connectionName: 'PG',
          databaseType: 'postgresql',
          status: 'connecting',
        },
      ]);
    });

    await waitFor(() => {
      expect(result.current.tabs[0]?.status).toBe('error');
      expect(result.current.tabs[0]?.error).toBe('connect failed');
    });
  });

  it('reacts to cross-window connection-ready and disconnect events', async () => {
    const { result } = renderHook(() => useConnectionTabs());

    act(() => {
      result.current.setTabs([
        {
          connectionId: 'cfg-1',
          dbSessionId: '',
          connectionName: 'PG',
          databaseType: 'postgresql',
          status: 'connecting',
        },
      ]);
    });

    await waitFor(() => expect(crossWindowHandlers['datazen:connection-ready']).toBeDefined());

    act(() => {
      crossWindowHandlers['datazen:connection-ready']({
        connectionId: 'cfg-1',
        dbSessionId: 'sess-remote',
      });
    });
    expect(result.current.tabs[0]?.dbSessionId).toBe('sess-remote');

    act(() => {
      crossWindowHandlers['datazen:disconnect-requested']({ dbSessionId: 'sess-remote' });
    });
    expect(result.current.tabs).toHaveLength(0);
  });
});
