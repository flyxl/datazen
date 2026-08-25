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
