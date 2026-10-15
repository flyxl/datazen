import { afterEach, describe, expect, it, vi } from 'vitest';
import { emptyQueryExecState, type QueryExecState } from '../queryExecActions';
import { paneKey } from '../paneKeys';
import type { Panel } from '../panelTypes';
import {
  RUNNING_CLOSE_TIMEOUT_MS,
  askRunningExecutionGate,
  createRunningCloseGuard,
  runningPanesOf,
  setRunningExecutionGate,
  setRunningCloseTimeoutMs,
  settleRunningPanes,
  type RunningExecutionRequest,
  type RunningPane,
} from '../runningExecutionClose';

/**
 * Closing a running tab is a three-step contract — ask, stop, wait — and each
 * step has a failure mode that must not silently degrade into "drop the tab and
 * hope": a declined prompt must change nothing, an uncancellable driver must not
 * wait for a confirmation that can never come, and an unconfirmed cancel must not
 * let the tab disappear.
 */

function queryPanel(id: string): Panel {
  return {
    id,
    connectionId: 'cfg-1',
    dbSessionId: 'sess-1',
    connectionName: 'local',
    databaseType: 'postgresql',
    type: 'query',
    title: 'SQL',
    database: 'app',
    schema: 'public',
  } as Panel;
}

function running(executionId = 'exec-1'): QueryExecState {
  return { ...emptyQueryExecState(), running: true, executionId };
}

function idle(): QueryExecState {
  return { ...emptyQueryExecState(), running: false };
}

function request(panelId: string): RunningExecutionRequest {
  return { panelId, title: 'SQL', paneId: 'main', sql: 'SELECT 1', runningPanes: 1 };
}

describe('runningPanesOf', () => {
  it('reports only the panes whose statement is still in flight', () => {
    const panel = queryPanel('panel-1');
    const exec = new Map<string, QueryExecState>([
      [paneKey('panel-1', 'main'), running()],
      [paneKey('panel-1', 'split'), idle()],
      [paneKey('panel-1', 'second'), running('exec-2')],
    ]);

    expect(runningPanesOf(panel, exec)).toEqual([
      { paneId: 'main', exec: expect.objectContaining({ executionId: 'exec-1' }) },
      { paneId: 'second', exec: expect.objectContaining({ executionId: 'exec-2' }) },
    ]);
  });

  it('never reports a pane of a non-query panel', () => {
    const panel = { ...queryPanel('panel-1'), type: 'table' } as unknown as Panel;
    expect(runningPanesOf(panel, new Map([[paneKey('panel-1'), running()]]))).toEqual([]);
  });

  it('reports nothing when every pane is idle', () => {
    const exec = new Map([[paneKey('panel-1'), idle()]]);
    expect(runningPanesOf(queryPanel('panel-1'), exec)).toEqual([]);
  });
});

describe('askRunningExecutionGate', () => {
  afterEach(() => setRunningExecutionGate(null));

  it('allows the close when no host is listening', async () => {
    await expect(askRunningExecutionGate(request('panel-1'))).resolves.toBe(true);
  });

  it('asks one tab at a time and returns the answers in order', async () => {
    const asked: string[] = [];
    let releaseFirst!: (value: boolean) => void;
    const firstAnswer = new Promise<boolean>((resolve) => {
      releaseFirst = resolve;
    });
    setRunningExecutionGate((pending) => {
      asked.push(pending.panelId);
      return pending.panelId === 'panel-1' ? firstAnswer : Promise.resolve(true);
    });

    const first = askRunningExecutionGate(request('panel-1'));
    const second = askRunningExecutionGate(request('panel-2'));
    await new Promise((resolve) => setTimeout(resolve, 0));

    // The second prompt must not overwrite the one still on screen.
    expect(asked).toEqual(['panel-1']);

    releaseFirst(true);
    await expect(first).resolves.toBe(true);
    await expect(second).resolves.toBe(true);
    expect(asked).toEqual(['panel-1', 'panel-2']);
  });

  it('keeps the tab and the queue when the prompt host fails', async () => {
    let calls = 0;
    setRunningExecutionGate(async () => {
      calls += 1;
      if (calls === 1) throw new Error('dialog host is gone');
      return true;
    });

    // A broken prompt is not consent to destroy a running statement.
    await expect(askRunningExecutionGate(request('panel-1'))).resolves.toBe(false);
    await expect(askRunningExecutionGate(request('panel-2'))).resolves.toBe(true);
    expect(calls).toBe(2);
  });

  it('releases every open prompt when the host is withdrawn', async () => {
    setRunningExecutionGate(() => new Promise<boolean>(() => {}));
    const first = askRunningExecutionGate(request('panel-1'));
    const queued = askRunningExecutionGate(request('panel-2'));
    await new Promise((resolve) => setTimeout(resolve, 0));

    // Nobody can answer any more: neither the prompt on screen nor the one
    // waiting its turn may hang, and neither may be read as a refusal.
    setRunningExecutionGate(null);
    await expect(first).resolves.toBe(true);
    await expect(queued).resolves.toBe(true);
    await expect(askRunningExecutionGate(request('panel-3'))).resolves.toBe(true);
  });

  it('asks the replacement host for a prompt that was queued behind the old one', async () => {
    setRunningExecutionGate(() => new Promise<boolean>(() => {}));
    const abandoned = askRunningExecutionGate(request('panel-1'));
    const queued = askRunningExecutionGate(request('panel-2'));
    await new Promise((resolve) => setTimeout(resolve, 0));

    const asked: string[] = [];
    setRunningExecutionGate(async (pending) => {
      asked.push(pending.panelId);
      return false;
    });
    // The prompt the old host was showing dies with it (nobody is left to answer,
    // which is the no-host answer); the queued one is put to the host that is
    // actually on screen now, so its refusal is the one that counts.
    await expect(abandoned).resolves.toBe(true);
    await expect(queued).resolves.toBe(false);
    expect(asked).toEqual(['panel-2']);
  });
});

describe('settleRunningPanes', () => {
  const panel = queryPanel('panel-1');
  const panes: RunningPane[] = [{ paneId: 'main', exec: running() }];

  it('requests a cancel and settles as soon as the stream says it is over', async () => {
    const exec = new Map([[paneKey('panel-1'), running()]]);
    const requested: string[] = [];
    const settlement = await settleRunningPanes(panel, panes, {
      readExec: (key) => exec.get(key),
      requestCancel: async (paneId) => {
        requested.push(paneId);
        exec.set(paneKey('panel-1'), idle());
        return true;
      },
      timeoutMs: 50,
      pollMs: 1,
    });

    expect(requested).toEqual(['main']);
    expect(settlement).toEqual({ settled: true, pending: [] });
  });

  it('reports the pane as pending when the cancel is never confirmed', async () => {
    const exec = new Map([[paneKey('panel-1'), running()]]);
    const settlement = await settleRunningPanes(panel, panes, {
      readExec: (key) => exec.get(key),
      requestCancel: async () => true,
      timeoutMs: 10,
      pollMs: 1,
    });

    expect(settlement).toEqual({ settled: false, pending: ['main'] });
  });

  it('does not wait when no request could be delivered: the session close is the cleanup', async () => {
    const requestCancel = vi.fn(async () => false);
    const settlement = await settleRunningPanes(panel, panes, {
      readExec: () => running(),
      requestCancel,
      timeoutMs: 10,
      pollMs: 1,
    });

    expect(requestCancel).toHaveBeenCalledWith('main');
    expect(settlement).toEqual({ settled: true, pending: [] });
  });

  it('treats a rejected cancel request as nothing to wait for', async () => {
    const settlement = await settleRunningPanes(panel, panes, {
      readExec: () => running(),
      requestCancel: async () => {
        throw new Error('runtime refused');
      },
      timeoutMs: 10,
      pollMs: 1,
    });

    expect(settlement).toEqual({ settled: true, pending: [] });
  });

  it('treats a vanished exec entry as finished', async () => {
    const settlement = await settleRunningPanes(panel, panes, {
      readExec: () => undefined,
      requestCancel: async () => true,
      timeoutMs: 10,
      pollMs: 1,
    });

    expect(settlement).toEqual({ settled: true, pending: [] });
  });

  it('has nothing to settle without running panes', async () => {
    const requestCancel = vi.fn(async () => true);
    const settlement = await settleRunningPanes(panel, [], {
      readExec: () => running(),
      requestCancel,
    });

    expect(requestCancel).not.toHaveBeenCalled();
    expect(settlement).toEqual({ settled: true, pending: [] });
  });
});

describe('createRunningCloseGuard', () => {
  afterEach(() => setRunningCloseTimeoutMs(RUNNING_CLOSE_TIMEOUT_MS));

  const panel = queryPanel('panel-1');

  /** Wiring that records what the guard did, with one live pane by default. */
  function wiring(overrides: Partial<Parameters<typeof createRunningCloseGuard>[0]> = {}) {
    const exec = new Map([[paneKey('panel-1'), running()]]);
    const calls = { cancelled: [] as string[], blocked: [] as string[] };
    const base = {
      queryExec: () => exec,
      hasSession: () => true,
      canCancel: () => true,
      cancelPane: async (panelId: string, paneId: string) => {
        calls.cancelled.push(`${panelId}:${paneId}`);
        exec.set(paneKey(panelId, paneId), idle());
        return true;
      },
      markBlocked: (panelId: string, paneId: string) => {
        calls.blocked.push(`${panelId}:${paneId}`);
      },
      ...overrides,
    };
    return { guard: createRunningCloseGuard(base), exec, calls };
  }

  it('decides synchronously when nothing could be stopped', () => {
    const noSession = wiring({ hasSession: () => false });
    const noCancel = wiring({ canCancel: () => false });
    const idlePanel = wiring({ queryExec: () => new Map() });

    // Not a promise: callers that never had a statement to stop must not be
    // pushed into a later microtask by a feature they cannot reach.
    expect(noSession.guard({ panel, panelId: 'panel-1' })).toBe(true);
    expect(noCancel.guard({ panel, panelId: 'panel-1' })).toBe(true);
    expect(idlePanel.guard({ panel, panelId: 'panel-1' })).toBe(true);
    expect(noSession.calls.cancelled).toEqual([]);
  });

  it('stops the statement and allows the close once it is over', async () => {
    const { guard, calls } = wiring();
    await expect(guard({ panel, panelId: 'panel-1' })).resolves.toBe(true);
    expect(calls.cancelled).toEqual(['panel-1:main']);
    expect(calls.blocked).toEqual([]);
  });

  it('keeps the tab and marks the pane when the cancel is never confirmed', async () => {
    // A deadline of milliseconds: the point is the *policy* on the timeout, not
    // how long a real desktop waits before concluding the statement never stopped.
    setRunningCloseTimeoutMs(20);
    const { guard, calls } = wiring({ cancelPane: async () => true });

    // Refusing to close is the whole answer: the cancel was asked for, the
    // statement never reported a terminal state, so nobody may claim it stopped.
    await expect(guard({ panel, panelId: 'panel-1' })).resolves.toBe(false);
    expect(calls.blocked).toEqual(['panel-1:main']);
  });

  it('only asks about the pane it was given', async () => {
    const exec = new Map([
      [paneKey('panel-1', 'main'), running('exec-1')],
      [paneKey('panel-1', 'p2'), running('exec-2')],
    ]);
    const { guard, calls } = wiring({
      queryExec: () => exec,
      cancelPane: async (panelId: string, paneId: string) => {
        calls.cancelled.push(`${panelId}:${paneId}`);
        exec.set(paneKey(panelId, paneId), idle());
        return true;
      },
    });

    await expect(guard({ panel, panelId: 'panel-1', paneId: 'p2' })).resolves.toBe(true);
    // Exactly one prompt and one cancel: the pane that was not asked about is
    // left alone, still running, exactly as the user left it.
    expect(calls.cancelled).toEqual(['panel-1:p2']);
    expect(exec.get(paneKey('panel-1', 'main'))?.running).toBe(true);
  });
});
