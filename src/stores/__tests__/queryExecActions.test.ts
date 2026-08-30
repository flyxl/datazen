import { beforeEach, describe, expect, it, vi } from 'vitest';
import { emptyQueryExecState, runBoundQuery, runStreamingQuery } from '../queryExecActions';

const mockExecuteQuery = vi.fn();
const mockExecuteQueryStream = vi.fn();
const mockEmitCrossWindow = vi.fn();

vi.mock('../../commands/query', () => ({
  queryCommands: {
    executeQuery: (...args: unknown[]) => mockExecuteQuery(...args),
    executeQueryStream: (...args: unknown[]) => mockExecuteQueryStream(...args),
  },
}));

vi.mock('../../lib/crossWindowBus', () => ({
  emitCrossWindow: (...args: unknown[]) => mockEmitCrossWindow(...args),
}));

describe('queryExecActions schema refresh', () => {
  beforeEach(() => {
    mockExecuteQuery.mockReset();
    mockExecuteQueryStream.mockReset();
    mockEmitCrossWindow.mockReset();
    mockEmitCrossWindow.mockResolvedValue(undefined);
  });

  it('emits refresh-connection after successful bound DDL query', async () => {
    mockExecuteQuery.mockResolvedValueOnce({ results: [], totalTimeMs: 5 });
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
    mockExecuteQuery.mockRejectedValueOnce(new Error('permission denied'));
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
    mockExecuteQuery.mockResolvedValueOnce({ results: [], totalTimeMs: 5 });
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    const getExec = () => exec;
    const setExec = (next: Map<string, ReturnType<typeof emptyQueryExecState>>) => {
      exec = next;
    };

    await runBoundQuery('panel-1', 'conn-1', 'SELECT 1', {}, getExec, setExec);

    expect(mockEmitCrossWindow).not.toHaveBeenCalled();
  });

  it('emits refresh after successful streaming DDL query', async () => {
    mockExecuteQueryStream.mockImplementationOnce(
      async (_connId: string, _sql: string, onEvent: (event: unknown) => void) => {
        onEvent({ type: 'done', totalTimeMs: 5 });
      },
    );
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
    mockExecuteQueryStream.mockRejectedValueOnce(new Error('syntax error'));
    let exec = new Map([['panel-1', emptyQueryExecState()]]);
    const getExec = () => exec;
    const setExec = (next: Map<string, ReturnType<typeof emptyQueryExecState>>) => {
      exec = next;
    };

    await runStreamingQuery('panel-1', 'conn-1', 'CREATE TABLE t (id int)', getExec, setExec);

    expect(mockEmitCrossWindow).not.toHaveBeenCalled();
    expect(exec.get('panel-1')?.error).toBe('syntax error');
  });
});

describe('queryExecActions carries the panel database (F1 BUG-001)', () => {
  beforeEach(() => {
    mockExecuteQuery.mockReset();
    mockExecuteQueryStream.mockReset();
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

  it('runBoundQuery forwards database to executeQuery', async () => {
    mockExecuteQuery.mockResolvedValueOnce({ results: [], totalTimeMs: 5 });
    const { getExec, setExec } = makeExecContext();

    await runBoundQuery(
      'panel-1',
      'conn-1',
      'SELECT * FROM users',
      {},
      getExec,
      setExec,
      'db_b',
      'sales',
    );

    expect(mockExecuteQuery).toHaveBeenCalledWith('conn-1', 'SELECT * FROM users', {}, 'db_b', 'sales');
  });

  it('runBoundQuery normalizes a missing database to null', async () => {
    mockExecuteQuery.mockResolvedValueOnce({ results: [], totalTimeMs: 5 });
    const { getExec, setExec } = makeExecContext();

    await runBoundQuery('panel-1', 'conn-1', 'SELECT 1', {}, getExec, setExec);

    expect(mockExecuteQuery).toHaveBeenCalledWith('conn-1', 'SELECT 1', {}, null, null);
  });

  it('runStreamingQuery forwards database via stream options', async () => {
    mockExecuteQueryStream.mockImplementationOnce(
      async (_connId: string, _sql: string, onEvent: (event: unknown) => void) => {
        onEvent({ type: 'done', totalTimeMs: 5 });
      },
    );
    const { getExec, setExec } = makeExecContext();

    await runStreamingQuery('panel-1', 'conn-1', 'SELECT 1', getExec, setExec, 'analytics');

    expect(mockExecuteQueryStream).toHaveBeenCalledWith(
      'conn-1',
      'SELECT 1',
      expect.any(Function),
      { database: 'analytics', schema: null },
    );
  });

  it('runStreamingQuery normalizes a missing database to null', async () => {
    mockExecuteQueryStream.mockImplementationOnce(
      async (_connId: string, _sql: string, onEvent: (event: unknown) => void) => {
        onEvent({ type: 'done', totalTimeMs: 5 });
      },
    );
    const { getExec, setExec } = makeExecContext();

    await runStreamingQuery('panel-1', 'conn-1', 'SELECT 1', getExec, setExec);

    expect(mockExecuteQueryStream).toHaveBeenCalledWith(
      'conn-1',
      'SELECT 1',
      expect.any(Function),
      { database: null, schema: null },
    );
  });

  it('runStreamingQuery forwards the F7 schema target via stream options', async () => {
    mockExecuteQueryStream.mockImplementationOnce(
      async (_connId: string, _sql: string, onEvent: (event: unknown) => void) => {
        onEvent({ type: 'done', totalTimeMs: 5 });
      },
    );
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

    expect(mockExecuteQueryStream).toHaveBeenCalledWith(
      'conn-1',
      'SELECT 1',
      expect.any(Function),
      { database: 'analytics', schema: 'sales' },
    );
  });
});

describe('queryExecActions cancellation terminal semantics', () => {
  beforeEach(() => {
    mockExecuteQuery.mockReset();
    mockExecuteQueryStream.mockReset();
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
        exec = new Map([
          ['panel-1', { ...current, cancelState: 'requested' as const }],
        ]);
      },
    };
  }

  it('waits for a streaming promise before reporting cancellation', async () => {
    const gate = deferred();
    mockExecuteQueryStream.mockImplementationOnce(async () => gate.promise);
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

  it('reports success when a requested cancellation does not terminate the query', async () => {
    const gate = deferred();
    mockExecuteQueryStream.mockImplementationOnce(async () => gate.promise);
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
    mockExecuteQuery.mockImplementationOnce(async () => gate.promise);
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
