import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import { invoke } from '@tauri-apps/api/core';
import type { ConnectionConfig } from '../../../types';
import { useSchemaDiffEndpoints } from '../useSchemaDiffEndpoints';
import { ensureDedicatedSession, listDatabasesDedicated } from '../../../lib/dedicatedDbSession';
import { databaseCommands } from '../../../commands/database';

const { stableT } = vi.hoisted(() => ({
  stableT: (key: string) => key,
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT, language: 'en' }),
}));

vi.mock('@tauri-apps/api/core', () => ({
  invoke: vi.fn(),
}));

vi.mock('../../../lib/crossWindowBus', () => ({
  listenCrossWindow: vi.fn().mockResolvedValue(() => {}),
}));

const urlParamMock = vi.fn<(name: string) => string | null>();

vi.mock('../../../lib/windowKind', () => ({
  getUrlParam: (name: string) => urlParamMock(name),
}));

vi.mock('../../../lib/dedicatedDbSession', () => ({
  listDatabasesDedicated: vi.fn(),
  ensureDedicatedSession: vi.fn(),
  releaseDedicatedSession: vi.fn().mockResolvedValue(undefined),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    getTables: vi.fn().mockResolvedValue([]),
  },
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    pingConnection: vi.fn().mockResolvedValue(true),
  },
}));

function pgConn(id: string, name: string, database?: string): ConnectionConfig {
  return {
    id,
    name,
    databaseType: 'postgresql',
    host: '127.0.0.1',
    port: 5432,
    sslMode: 'prefer',
    database,
  };
}

const MOCK_CONNECTIONS: ConnectionConfig[] = [
  pgConn('pg-src', 'PG Src', 'datazen_sync_src'),
  pgConn('pg-tgt', 'PG Tgt', 'datazen_sync_tgt'),
  {
    id: 'mysql-tgt',
    name: 'MySQL Tgt',
    databaseType: 'mysql',
    host: '127.0.0.1',
    port: 3306,
    sslMode: 'prefer',
    database: 'datazen_sync_mysql_tgt',
  },
];

describe('useSchemaDiffEndpoints', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    urlParamMock.mockReset();
    urlParamMock.mockReturnValue(null);
    vi.mocked(invoke).mockResolvedValue(MOCK_CONNECTIONS);
    vi.mocked(listDatabasesDedicated).mockResolvedValue({
      databases: ['datazen_sync_src', 'datazen_sync_tgt'],
    });
    vi.mocked(ensureDedicatedSession).mockImplementation(
      async (_current, connectionId, database) =>
        connectionId && database
          ? { connectionId, database, dbSessionId: `session-${connectionId}` }
          : null,
    );
  });

  it('loads connections on mount', async () => {
    const { result } = renderHook(() => useSchemaDiffEndpoints());

    await waitFor(() => {
      expect(result.current.connections).toHaveLength(3);
    });
    expect(invoke).toHaveBeenCalledWith('get_connections');
  });

  it('reports cross-dialect when source and target types differ', async () => {
    const { result } = renderHook(() => useSchemaDiffEndpoints());

    await waitFor(() => expect(result.current.connections).toHaveLength(3));

    act(() => {
      result.current.setSourceId('pg-src');
      result.current.setTargetId('mysql-tgt');
    });

    await waitFor(() => {
      expect(result.current.isCrossDialect).toBe(true);
    });
  });

  it('validateEndpoints rejects missing endpoints and reports errors', async () => {
    const onError = vi.fn();
    const { result } = renderHook(() => useSchemaDiffEndpoints({ onError }));

    await waitFor(() => expect(result.current.connections).toHaveLength(3));

    expect(result.current.validateEndpoints()).toBe(false);
    expect(onError).toHaveBeenCalledWith('sync.selectBoth');

    act(() => {
      result.current.setSourceId('pg-src');
      result.current.setTargetId('pg-src');
      result.current.setSourceDatabase('datazen_sync_src');
      result.current.setTargetDatabase('datazen_sync_tgt');
    });

    expect(result.current.validateEndpoints()).toBe(true);

    act(() => {
      result.current.setTargetDatabase('datazen_sync_src');
    });

    expect(result.current.validateEndpoints()).toBe(false);
    expect(onError).toHaveBeenCalledWith('sync.cannotSameDb');

    act(() => {
      result.current.setTargetId('pg-tgt');
      result.current.setTargetDatabase('datazen_sync_tgt');
    });

    expect(result.current.validateEndpoints()).toBe(true);

    act(() => {
      result.current.setTargetDatabase('');
    });

    expect(result.current.validateEndpoints()).toBe(false);
    expect(onError).toHaveBeenCalledWith('sync.selectDbRequired');
  });

  it('validateEndpoints passes when endpoints differ', async () => {
    const { result } = renderHook(() => useSchemaDiffEndpoints());

    await waitFor(() => expect(result.current.connections).toHaveLength(3));

    act(() => {
      result.current.setSourceId('pg-src');
      result.current.setTargetId('pg-tgt');
      result.current.setSourceDatabase('datazen_sync_src');
      result.current.setTargetDatabase('datazen_sync_tgt');
    });

    expect(result.current.validateEndpoints()).toBe(true);
  });

  it('handleSwap exchanges source and target endpoint state', async () => {
    const { result } = renderHook(() => useSchemaDiffEndpoints());

    await waitFor(() => expect(result.current.connections).toHaveLength(3));

    act(() => {
      result.current.setSourceId('pg-src');
      result.current.setTargetId('pg-tgt');
      result.current.setSourceDatabase('datazen_sync_src');
      result.current.setTargetDatabase('datazen_sync_tgt');
    });

    await waitFor(() => {
      expect(result.current.sourceSession?.dbSessionId).toBe('session-pg-src');
      expect(result.current.targetSession?.dbSessionId).toBe('session-pg-tgt');
    });

    act(() => {
      result.current.handleSwap();
    });

    expect(result.current.sourceId).toBe('pg-tgt');
    expect(result.current.targetId).toBe('pg-src');
    expect(result.current.sourceDatabase).toBe('datazen_sync_tgt');
    expect(result.current.targetDatabase).toBe('datazen_sync_src');
    expect(result.current.sourceSession?.connectionId).toBe('pg-tgt');
    expect(result.current.targetSession?.connectionId).toBe('pg-src');
  });

  it('ensureConnected reuses a live dedicated session', async () => {
    const { result } = renderHook(() => useSchemaDiffEndpoints());

    await waitFor(() => expect(result.current.connections).toHaveLength(3));

    act(() => {
      result.current.setSourceId('pg-src');
      result.current.setSourceDatabase('datazen_sync_src');
    });

    await waitFor(() => {
      expect(result.current.sourceSession?.dbSessionId).toBe('session-pg-src');
    });

    let sessionId: string | null = null;
    await act(async () => {
      sessionId = await result.current.ensureConnected('source');
    });

    expect(sessionId).toBe('session-pg-src');
    const { connectionCommands } = await import('../../../commands/connection');
    expect(connectionCommands.pingConnection).toHaveBeenCalledWith('session-pg-src');
  });

  it('[tester] applies URL prefill for source and target connection ids', async () => {
    urlParamMock.mockImplementation((name) => {
      if (name === 'sourceId') return 'pg-src';
      if (name === 'targetId') return 'pg-tgt';
      return null;
    });
    const { result } = renderHook(() => useSchemaDiffEndpoints());

    await waitFor(() => {
      expect(result.current.sourceId).toBe('pg-src');
      expect(result.current.targetId).toBe('pg-tgt');
    });
  });

  it('discovers schemas through table metadata for non-PostgreSQL SQL drivers', async () => {
    vi.mocked(databaseCommands.getTables).mockResolvedValue([
      { name: 'users', schema: 'public', tableType: 'table' },
      { name: 'users', schema: 'app', tableType: 'table' },
    ]);
    const { result } = renderHook(() => useSchemaDiffEndpoints());

    await waitFor(() => expect(result.current.connections).toHaveLength(3));

    act(() => {
      result.current.setSourceId('mysql-tgt');
      result.current.setSourceDatabase('datazen_sync_mysql_tgt');
    });

    await waitFor(() => {
      expect(result.current.sourceSchemas).toEqual(['app', 'public']);
    });
    expect(databaseCommands.getTables).toHaveBeenCalledWith(
      'session-mysql-tgt',
      'datazen_sync_src',
    );
  });
});
