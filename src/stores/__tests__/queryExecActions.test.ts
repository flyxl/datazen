import { beforeEach, describe, expect, it, vi } from 'vitest';
import { ApiError } from '@datazen/backend-client';
import { t } from '../../locales/t';
import {
  emptyQueryExecState,
  runBoundQuery as runBoundQueryImpl,
  runStreamingQuery as runStreamingQueryImpl,
} from '../queryExecActions';
import { useTableDataStore } from '../tableDataStore';

const mockRunEditorQuery = vi.fn();
const mockEmitCrossWindow = vi.fn();
const mockGetTableData = vi.fn();

vi.mock('../../commands/database', () => ({
  databaseCommands: {
    getTableData: (...args: unknown[]) => mockGetTableData(...args),
  },
}));

vi.mock('../../lib/session/runEditorQuery', () => ({
  runEditorQuery: (...args: unknown[]) => mockRunEditorQuery(...args),
}));

vi.mock('../../lib/crossWindowBus', () => ({
  emitCrossWindow: (...args: unknown[]) => mockEmitCrossWindow(...args),
}));

function runBoundQuery(
  paneKey: string,
  dbSessionId: string,
  sql: string,
  params: Record<string, string | number | boolean | null>,
  getExec: () => Map<string, ReturnType<typeof emptyQueryExecState>>,
  setExec: (exec: Map<string, ReturnType<typeof emptyQueryExecState>>) => void,
  database?: string | null,
  schema?: string | null,
) {
  return runBoundQueryImpl(
    paneKey,
    dbSessionId,
    sql,
    params,
    getExec,
    setExec,
    database,
    schema,
    'cfg-1',
  );
}

function runStreamingQuery(
  paneKey: string,
  dbSessionId: string,
  sql: string,
  getExec: () => Map<string, ReturnType<typeof emptyQueryExecState>>,
  setExec: (exec: Map<string, ReturnType<typeof emptyQueryExecState>>) => void,
  database?: string | null,
  schema?: string | null,
  params?: Record<string, string | number | boolean | null>,
) {
  return runStreamingQueryImpl(
    paneKey,
    dbSessionId,
    sql,
    getExec,
    setExec,
    database,
    schema,
    params,
    'cfg-1',
  );
}

describe('queryExecActions schema refresh', () => {
  beforeEach(() => {
    mockRunEditorQuery.mockReset();
    mockRunEditorQuery.mockImplementation(async (...args: unknown[]) => {
      const onEvent = args[6] as (event: unknown) => void;
      onEvent({ type: 'executionStarted', executionId: 'exec-1' });
      onEvent({ type: 'done', totalTimeMs: 5 });
    });
    mockEmitCrossWindow.mockReset();
    mockEmitCrossWindow.mockResolvedValue(undefined);
  });

  it('emits refresh-connection after successful bound DDL query', async () => {
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    const getExec = () => exec;
    const setExec = (next: Map<string, ReturnType<typeof emptyQueryExecState>>) => {
      exec = next;
    };

    await runBoundQuery('panel-1', 'conn-1', 'CREATE DATABASE app', {}, getExec, setExec);

    expect(mockEmitCrossWindow).toHaveBeenCalledWith('datazen:refresh-connection', {
      dbSessionId: 'conn-1',
    });
    expect(exec.get('panel-1')?.error).toBeNull();
  });

  it('does not emit refresh after failed bound query', async () => {
    mockRunEditorQuery.mockRejectedValueOnce(new Error('permission denied'));
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    const getExec = () => exec;
    const setExec = (next: Map<string, ReturnType<typeof emptyQueryExecState>>) => {
      exec = next;
    };

    await runBoundQuery('panel-1', 'conn-1', 'CREATE DATABASE app', {}, getExec, setExec);

    expect(mockEmitCrossWindow).not.toHaveBeenCalled();
    expect(exec.get('panel-1')?.error).toBe('permission denied');
  });

  it('does not emit refresh for non-DDL success', async () => {
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    const getExec = () => exec;
    const setExec = (next: Map<string, ReturnType<typeof emptyQueryExecState>>) => {
      exec = next;
    };

    await runBoundQuery('panel-1', 'conn-1', 'SELECT 1', {}, getExec, setExec);

    expect(mockEmitCrossWindow).not.toHaveBeenCalled();
  });

  it('emits refresh after successful streaming DDL query', async () => {
    mockRunEditorQuery.mockImplementationOnce(async (...args: unknown[]) => {
      const onEvent = args[6] as (event: unknown) => void;
      onEvent({ type: 'executionStarted', executionId: 'exec-1' });
      onEvent({ type: 'done', totalTimeMs: 5 });
    });
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    const getExec = () => exec;
    const setExec = (next: Map<string, ReturnType<typeof emptyQueryExecState>>) => {
      exec = next;
    };

    await runStreamingQuery('panel-1', 'conn-1', 'DROP TABLE users', getExec, setExec);

    expect(mockEmitCrossWindow).toHaveBeenCalledWith('datazen:refresh-connection', {
      dbSessionId: 'conn-1',
    });
    expect(exec.get('panel-1')?.running).toBe(false);
  });

  it('does not emit refresh when streaming throws', async () => {
    mockRunEditorQuery.mockRejectedValueOnce(new Error('syntax error'));
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    const getExec = () => exec;
    const setExec = (next: Map<string, ReturnType<typeof emptyQueryExecState>>) => {
      exec = next;
    };

    await runStreamingQuery('panel-1', 'conn-1', 'CREATE TABLE t (id int)', getExec, setExec);

    expect(mockEmitCrossWindow).not.toHaveBeenCalled();
    expect(exec.get('panel-1')?.error).toBe('syntax error');
  });

  it('turns an editor-session quota refusal into an actionable message', async () => {
    // 宿主把额度型拒绝收敛成一句不含细节的通用文案；只有 code 能区分它。
    // 用户需要看到「该怎么做」，而不是又一个 connection runtime operation failed。
    mockRunEditorQuery.mockRejectedValueOnce(
      new ApiError('SessionQuotaExceeded', 'connection runtime operation failed'),
    );
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    const getExec = () => exec;
    const setExec = (next: Map<string, ReturnType<typeof emptyQueryExecState>>) => {
      exec = next;
    };

    await runStreamingQuery('panel-1', 'conn-1', 'SELECT 1', getExec, setExec);

    expect(exec.get('panel-1')?.error).toBe(t('query.session.editorQuotaExceeded'));
    expect(exec.get('panel-1')?.error).not.toContain('connection runtime operation failed');
    expect(exec.get('panel-1')?.running).toBe(false);
  });

  async function seedCachedTableRows(dbSessionId: string) {
    mockGetTableData.mockReset();
    mockGetTableData.mockResolvedValue({
      columns: [
        { name: 'id', dataType: 'integer', isPrimaryKey: true },
        { name: 'name', dataType: 'text', isPrimaryKey: false },
      ],
      rows: [
        [1, 'Alice'],
        [2, 'Bob'],
      ],
      totalRows: 2,
      page: 0,
      pageSize: 50,
    });
    useTableDataStore.getState().reset();
    useTableDataStore
      .getState()
      .loadTableData({ panelId: 'table-panel', dbSessionId, table: 'users' })
      .catch(() => {});
    await vi.waitFor(() => {
      expect(useTableDataStore.getState().byPanel.get('table-panel')?.rows).toHaveLength(2);
    });
  }

  function cachedRows() {
    return useTableDataStore.getState().byPanel.get('table-panel')?.rows;
  }

  it('invalidates cached table data after a successful UPDATE', async () => {
    await seedCachedTableRows('s-write');
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    await runStreamingQuery(
      'panel-1',
      's-write',
      "UPDATE users SET name = 'x' WHERE id = 1",
      () => exec,
      (next) => {
        exec = next;
      },
    );
    expect(cachedRows()).toHaveLength(0);
  });

  it('keeps cached table data after a read-only query', async () => {
    await seedCachedTableRows('s-read');
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    await runStreamingQuery(
      'panel-1',
      's-read',
      'SELECT * FROM users',
      () => exec,
      (next) => {
        exec = next;
      },
    );
    expect(cachedRows()).toHaveLength(2);
  });

  it('does not invalidate cached table data when the write fails', async () => {
    await seedCachedTableRows('s-fail');
    mockRunEditorQuery.mockRejectedValueOnce(new Error('constraint violated'));
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    await runStreamingQuery(
      'panel-1',
      's-fail',
      'DELETE FROM users',
      () => exec,
      (next) => {
        exec = next;
      },
    );
    expect(cachedRows()).toHaveLength(2);
  });
});

describe('queryExecActions carries the panel database', () => {
  beforeEach(() => {
    mockRunEditorQuery.mockReset();
    mockEmitCrossWindow.mockReset();
    mockEmitCrossWindow.mockResolvedValue(undefined);
  });

  function makeExecContext() {
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    return {
      getExec: () => exec,
      setExec: (next: Map<string, ReturnType<typeof emptyQueryExecState>>) => {
        exec = next;
      },
    };
  }

  it('runBoundQuery forwards owner, params and database to the editor session', async () => {
    const { getExec, setExec } = makeExecContext();

    await runBoundQuery(
      'panel-1',
      'conn-1',
      'SELECT * FROM users',
      { id: 7 },
      getExec,
      setExec,
      'db_b',
      'sales',
    );

    expect(mockRunEditorQuery).toHaveBeenCalledWith(
      'panel-1',
      'cfg-1',
      'SELECT * FROM users',
      'db_b',
      'sales',
      { id: 7 },
      expect.any(Function),
    );
  });

  it('runBoundQuery forwards an unpinned namespace to the editor session', async () => {
    const { getExec, setExec } = makeExecContext();

    await runBoundQuery('panel-1', 'conn-1', 'SELECT 1', {}, getExec, setExec);

    expect(mockRunEditorQuery).toHaveBeenCalledWith(
      'panel-1',
      'cfg-1',
      'SELECT 1',
      undefined,
      undefined,
      {},
      expect.any(Function),
    );
  });

  it('runStreamingQuery forwards database via stream options', async () => {
    mockRunEditorQuery.mockImplementationOnce(async (...args: unknown[]) => {
      const onEvent = args[6] as (event: unknown) => void;
      onEvent({ type: 'executionStarted', executionId: 'exec-1' });
      onEvent({ type: 'done', totalTimeMs: 5 });
    });
    const { getExec, setExec } = makeExecContext();

    await runStreamingQuery('panel-1', 'conn-1', 'SELECT 1', getExec, setExec, 'analytics');

    expect(mockRunEditorQuery).toHaveBeenCalledWith(
      'panel-1',
      'cfg-1',
      'SELECT 1',
      'analytics',
      undefined,
      undefined,
      expect.any(Function),
    );
  });

  it('runStreamingQuery forwards an unpinned namespace to the editor session', async () => {
    mockRunEditorQuery.mockImplementationOnce(async (...args: unknown[]) => {
      const onEvent = args[6] as (event: unknown) => void;
      onEvent({ type: 'executionStarted', executionId: 'exec-1' });
      onEvent({ type: 'done', totalTimeMs: 5 });
    });
    const { getExec, setExec } = makeExecContext();

    await runStreamingQuery('panel-1', 'conn-1', 'SELECT 1', getExec, setExec);

    expect(mockRunEditorQuery).toHaveBeenCalledWith(
      'panel-1',
      'cfg-1',
      'SELECT 1',
      undefined,
      undefined,
      undefined,
      expect.any(Function),
    );
  });

  it('runStreamingQuery forwards the schema target via stream options', async () => {
    mockRunEditorQuery.mockImplementationOnce(async (...args: unknown[]) => {
      const onEvent = args[6] as (event: unknown) => void;
      onEvent({ type: 'executionStarted', executionId: 'exec-1' });
      onEvent({ type: 'done', totalTimeMs: 5 });
    });
    const { getExec, setExec } = makeExecContext();

    await runStreamingQuery(
      'panel-1',
      'conn-1',
      'SELECT 1',
      getExec,
      setExec,
      'analytics',
      'sales',
    );

    expect(mockRunEditorQuery).toHaveBeenCalledWith(
      'panel-1',
      'cfg-1',
      'SELECT 1',
      'analytics',
      'sales',
      undefined,
      expect.any(Function),
    );
  });
});

describe('queryExecActions cancellation terminal semantics', () => {
  beforeEach(() => {
    mockRunEditorQuery.mockReset();
    mockEmitCrossWindow.mockReset();
    mockEmitCrossWindow.mockResolvedValue(undefined);
  });

  function deferred(): {
    promise: Promise<void>;
    resolve: () => void;
    reject: (error: unknown) => void;
  } {
    let resolve!: () => void;
    let reject!: (error: unknown) => void;
    const promise = new Promise<void>((res, rej) => {
      resolve = res;
      reject = rej;
    });
    return { promise, resolve, reject };
  }

  function makeExecContext() {
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    return {
      getExec: () => exec,
      setExec: (next: Map<string, ReturnType<typeof emptyQueryExecState>>) => {
        exec = next;
      },
      markCancelRequested: () => {
        const current = exec.get('panel-1')!;
        exec = new Map([['panel-1', { ...current, cancelState: 'requested' as const }]]);
      },
    };
  }

  it('waits for a streaming promise before reporting cancellation', async () => {
    const gate = deferred();
    mockRunEditorQuery.mockImplementationOnce(async () => gate.promise);
    const context = makeExecContext();
    const run = runStreamingQuery(
      'panel-1',
      'conn-1',
      'SELECT pg_sleep(10)',
      context.getExec,
      context.setExec,
    );

    await Promise.resolve();
    expect(context.getExec().get('panel-1')?.running).toBe(true);
    context.markCancelRequested();
    expect(context.getExec().get('panel-1')?.terminalState).toBeNull();

    gate.reject(new Error('canceling statement due to user request'));
    await run;

    expect(context.getExec().get('panel-1')).toMatchObject({
      running: false,
      terminalState: 'cancelled',
    });
  });

  it('stores executionId only for the active stream and clears it on completion', async () => {
    const gate = deferred();
    mockRunEditorQuery.mockImplementationOnce(async (...args: unknown[]) => {
      const onEvent = args[6] as (event: unknown) => void;
      onEvent({ type: 'executionStarted', executionId: 'exec-1' });
      await gate.promise;
      onEvent({ type: 'done', totalTimeMs: 5 });
    });
    const context = makeExecContext();
    const run = runStreamingQuery(
      'panel-1',
      'conn-1',
      'SELECT 1',
      context.getExec,
      context.setExec,
    );

    await Promise.resolve();
    expect(context.getExec().get('panel-1')?.executionId).toBe('exec-1');
    gate.resolve();
    await run;
    expect(context.getExec().get('panel-1')?.executionId).toBeNull();
  });

  it('reports success when a requested cancellation does not terminate the query', async () => {
    const gate = deferred();
    mockRunEditorQuery.mockImplementationOnce(async () => gate.promise);
    const context = makeExecContext();
    const run = runStreamingQuery(
      'panel-1',
      'conn-1',
      'SELECT 1',
      context.getExec,
      context.setExec,
    );

    await Promise.resolve();
    context.markCancelRequested();
    expect(context.getExec().get('panel-1')?.running).toBe(true);
    gate.resolve();
    await run;

    expect(context.getExec().get('panel-1')).toMatchObject({
      running: false,
      terminalState: 'succeeded',
    });
  });

  it('does not call a failed cancel request a confirmed cancellation', async () => {
    const gate = deferred();
    mockRunEditorQuery.mockImplementationOnce(async () => gate.promise);
    const context = makeExecContext();
    const run = runBoundQuery(
      'panel-1',
      'conn-1',
      'SELECT 1',
      {},
      context.getExec,
      context.setExec,
    );

    await Promise.resolve();
    const current = context.getExec().get('panel-1')!;
    context.setExec(
      new Map([['panel-1', { ...current, cancelState: 'failed', cancelError: 'cancel failed' }]]),
    );
    gate.reject(new Error('canceling statement due to user request'));
    await run;

    expect(context.getExec().get('panel-1')).toMatchObject({
      running: false,
      terminalState: 'unknown',
      error: 'canceling statement due to user request',
    });
  });
});
