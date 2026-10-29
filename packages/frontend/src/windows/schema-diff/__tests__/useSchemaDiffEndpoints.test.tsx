import { describe, it, expect, vi, beforeEach } from 'vitest';
import { renderHook, act, waitFor } from '@testing-library/react';
import type { ConnectionConfig } from '../../../types';
import { useSchemaDiffEndpoints } from '../useSchemaDiffEndpoints';
import {
  ensureDedicatedSession,
  listDatabasesDedicated,
  releaseDedicatedSession,
  type DedicatedSideSession,
} from '../../../lib/dedicatedDbSession';
import { databaseCommands } from '../../../commands/database';

const { stableT } = vi.hoisted(() => ({
  stableT: (key: string) => key,
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT, language: 'en' }),
}));

const invokeMock = vi.hoisted(() => vi.fn());
import { setBackendClient } from '@datazen/backend-client';
// form-split R6: the invoke wrappers ride the bound backend transport now
// (same binding pattern as file.test.ts — the mock keeps its invoke shape).
setBackendClient('test', {
  transport: { call: (c: string, p?: unknown) => (p === undefined ? invokeMock(c) : invokeMock(c, p)) },
} as never);
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
  ensureDedicatedSessionForEffect: vi.fn(
    async (
      current: DedicatedSideSession | null,
      connectionId: string,
      database: string,
      isCurrent: () => boolean,
      databaseOverride: string | null = database,
    ) => {
      const next = await ensureDedicatedSession(current, connectionId, database, databaseOverride);
      if (isCurrent()) return next;
      if (next?.dbSessionId && next.dbSessionId !== current?.dbSessionId) {
        await releaseDedicatedSession(next.dbSessionId);
      }
      return null;
    },
  ),
}));

vi.mock('../../../commands/database', () => ({
  databaseCommands: {
    listCatalog: vi.fn().mockResolvedValue({ database: '', schemas: [], relations: [] }),
  },
}));

vi.mock('../../../commands/connection', () => ({
  connectionCommands: {
    pingConnection: vi.fn().mockResolvedValue(true),
  },
}));

function pgConn(id: string, name: string, database?: string, schema?: string): ConnectionConfig {
  return {
    id,
    name,
    databaseType: 'postgresql',
    host: '127.0.0.1',
    port: 5432,
    sslMode: 'prefer',
    database,
    schema,
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
    vi.mocked(databaseCommands.listCatalog).mockResolvedValue({
      database: '',
      schemas: [],
      relations: [],
    });
    urlParamMock.mockReset();
    urlParamMock.mockReturnValue(null);
    invokeMock.mockResolvedValue(MOCK_CONNECTIONS);
    vi.mocked(listDatabasesDedicated).mockResolvedValue({
      databases: ['datazen_sync_src', 'datazen_sync_tgt'],
      // Matches the real helper, which releases the dedicated session before
      // returning and therefore reports `dbSessionId: null`.
      dbSessionId: null,
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
    expect(invokeMock).toHaveBeenCalledWith('get_connections');
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
    expect(onError).toHaveBeenCalledWith('schemaDiff.selectBoth');

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
    expect(onError).toHaveBeenCalledWith('schemaDiff.cannotSameDb');

    act(() => {
      result.current.setTargetId('pg-tgt');
      result.current.setTargetDatabase('datazen_sync_tgt');
    });

    expect(result.current.validateEndpoints()).toBe(true);

    act(() => {
      result.current.setTargetDatabase('');
    });

    expect(result.current.validateEndpoints()).toBe(false);
    expect(onError).toHaveBeenCalledWith('schemaDiff.selectDbRequired');
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

  it('keeps SQLite main as the catalog without overriding the configured file path', async () => {
    invokeMock.mockResolvedValue([
      { id: 'sqlite-src', name: 'SQLite Src', databaseType: 'sqlite', database: '/tmp/source.db' },
      { id: 'sqlite-tgt', name: 'SQLite Tgt', databaseType: 'sqlite', database: '/tmp/target.db' },
    ]);
    vi.mocked(listDatabasesDedicated).mockResolvedValue({ databases: ['main'], dbSessionId: null });

    const { result } = renderHook(() => useSchemaDiffEndpoints());
    await waitFor(() => expect(result.current.connections).toHaveLength(2));
    act(() => {
      result.current.setSourceId('sqlite-src');
      result.current.setTargetId('sqlite-tgt');
    });

    await waitFor(() => {
      expect(result.current.sourceDatabase).toBe('main');
      expect(result.current.targetDatabase).toBe('main');
      expect(result.current.sourceSession?.dbSessionId).toBe('session-sqlite-src');
      expect(result.current.targetSession?.dbSessionId).toBe('session-sqlite-tgt');
    });
    expect(listDatabasesDedicated).toHaveBeenCalledWith('sqlite-src', '/tmp/source.db');
    expect(listDatabasesDedicated).toHaveBeenCalledWith('sqlite-tgt', '/tmp/target.db');
    expect(ensureDedicatedSession).toHaveBeenCalledWith(null, 'sqlite-src', 'main', null);
    expect(ensureDedicatedSession).toHaveBeenCalledWith(null, 'sqlite-tgt', 'main', null);

    await act(async () => {
      await result.current.refreshEndpointSessions();
    });
    expect(ensureDedicatedSession).toHaveBeenLastCalledWith(
      expect.objectContaining({ connectionId: 'sqlite-tgt', database: 'main' }),
      'sqlite-tgt',
      'main',
      null,
    );
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

  it('discovers schemas from the catalog for non-PostgreSQL SQL drivers', async () => {
    vi.mocked(databaseCommands.listCatalog).mockResolvedValue({
      database: 'datazen_sync_src',
      schemas: ['app', 'public'],
      relations: [],
    });
    const { result } = renderHook(() => useSchemaDiffEndpoints());

    await waitFor(() => expect(result.current.connections).toHaveLength(3));

    act(() => {
      result.current.setSourceId('mysql-tgt');
      result.current.setSourceDatabase('datazen_sync_mysql_tgt');
    });

    await waitFor(() => {
      expect(result.current.sourceSchemas).toEqual(['app', 'public']);
    });
    expect(databaseCommands.listCatalog).toHaveBeenCalledWith(
      'session-mysql-tgt',
      'datazen_sync_src',
      null,
      'mysql-tgt',
    );
  });

  it('prefers the configured connection schema when the catalog has empty schemas', async () => {
    invokeMock.mockResolvedValue([
      pgConn('pg-src', 'PG Src', 'worker_db', 'e2e_worker_0'),
      pgConn('pg-tgt', 'PG Tgt', 'datazen_sync_tgt'),
    ]);
    vi.mocked(databaseCommands.listCatalog).mockResolvedValue({
      database: 'worker_db',
      schemas: ['e2e_worker_0', 'public'],
      relations: [],
    });

    const { result } = renderHook(() => useSchemaDiffEndpoints());
    await waitFor(() => expect(result.current.connections).toHaveLength(2));

    act(() => {
      result.current.setSourceId('pg-src');
      result.current.setSourceDatabase('worker_db');
    });

    await waitFor(() => {
      expect(result.current.sourceSchemas).toEqual(['e2e_worker_0', 'public']);
      expect(result.current.sourceSchema).toBe('e2e_worker_0');
    });
  });
});
