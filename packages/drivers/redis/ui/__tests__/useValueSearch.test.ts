/**
 * useValueSearch state-machine journey (R6).
 *
 * Fake timers drive the setTimeout(0) poll loop deterministically:
 *   input → start → incremental batches → cancel → restart → done.
 */
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';

const scanValues = vi.fn();
const scanAbort = vi.fn();

vi.mock('../redisInvoke', () => ({
  redisCommandInvoke: vi.fn(),
  invokeScanValues: (...a: unknown[]) => scanValues(...a),
  invokeScanAbort: (...a: unknown[]) => scanAbort(...a),
}));

import { useValueSearch } from '../useValueSearch';

beforeEach(() => {
  vi.useFakeTimers();
  scanValues.mockReset();
  scanAbort.mockReset();
  scanAbort.mockResolvedValue(undefined);
});

afterEach(() => {
  vi.useRealTimers();
});

function batch(
  over: Partial<{
    taskId: string;
    done: boolean;
    cancelled: boolean;
    limitHit: boolean;
    scannedKeys: number;
    cursor: number;
    matched: unknown[];
  }>,
) {
  return {
    taskId: 'task-1',
    done: false,
    cancelled: false,
    limitHit: false,
    scannedKeys: 0,
    cursor: 100,
    matched: [],
    ...over,
  };
}

/** Flush scheduled setTimeout(0) work until the loop settles. */
async function drain(maxTicks = 50) {
  for (let i = 0; i < maxTicks; i++) {
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });
  }
}

describe('useValueSearch', () => {
  it('accumulates incremental batches until done', async () => {
    scanValues
      .mockResolvedValueOnce(
        batch({
          scannedKeys: 500,
          cursor: 100,
          matched: [{ key: 'a', matchedIn: 'value', preview: 'aa' }],
        }),
      )
      .mockResolvedValueOnce(
        batch({
          scannedKeys: 1000,
          cursor: 0,
          done: true,
          matched: [{ key: 'b', matchedIn: 'key', preview: 'b' }],
        }),
      );

    const { result } = renderHook(() => useValueSearch({ dbSessionId: 's', dbIndex: 0 }));

    act(() => {
      result.current.start({ mode: 'value', query: 'a', pattern: '*' });
    });
    await drain();

    expect(result.current.state.status).toBe('done');
    expect(result.current.state.scanned).toBe(1000);
    expect(result.current.state.hits.map((h) => h.key)).toEqual(['a', 'b']);
    expect(scanValues).toHaveBeenCalledTimes(2);
    // first call starts at cursor 0, second resumes with cursor 100 + taskId
    expect(scanValues.mock.calls[0][2]).toMatchObject({ cursor: 0, mode: 'value' });
    expect(scanValues.mock.calls[1][2]).toMatchObject({ cursor: 100, taskId: 'task-1' });
  });

  it('cancel stops the loop and fires scan_abort with the current task id', async () => {
    let resolveFirst: (v: unknown) => void = () => {};
    scanValues.mockImplementationOnce(
      () =>
        new Promise((r) => {
          resolveFirst = r;
        }),
    );
    scanValues.mockResolvedValue(batch({ scannedKeys: 800, cursor: 50 })); // subsequent

    const { result } = renderHook(() => useValueSearch({ dbSessionId: 's', dbIndex: 0 }));

    act(() => {
      result.current.start({ mode: 'all', query: 'x', pattern: '*', maxKeys: 50_000 });
    });
    // let the first batch resolve so taskId is captured
    await act(async () => {
      resolveFirst(
        batch({
          scannedKeys: 300,
          cursor: 42,
          matched: [{ key: 'k', matchedIn: 'value', preview: 'p' }],
        }),
      );
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(0);
    });

    act(() => {
      result.current.cancel();
    });
    expect(result.current.state.status).toBe('cancelled');
    expect(scanAbort).toHaveBeenCalledWith('s', 'task-1', expect.any(Function));

    // no further batches after cancel
    const callsAtCancel = scanValues.mock.calls.length;
    await drain(5);
    expect(scanValues.mock.calls.length).toBe(callsAtCancel);
  });

  it('restart supersedes an in-flight run (seq guard, no cross-contamination)', async () => {
    let release: (v: unknown) => void = () => {};
    scanValues.mockImplementationOnce(
      () =>
        new Promise((r) => {
          release = r;
        }),
    );
    scanValues.mockResolvedValue(
      batch({
        taskId: 'task-2',
        scannedKeys: 10,
        cursor: 0,
        done: true,
        matched: [{ key: 'new', matchedIn: 'key', preview: 'n' }],
      }),
    );

    const { result } = renderHook(() => useValueSearch({ dbSessionId: 's', dbIndex: 0 }));

    act(() => {
      result.current.start({ mode: 'value', query: 'old', pattern: '*' });
    });
    // start a fresh search while the first batch is still pending
    act(() => {
      result.current.start({ mode: 'value', query: 'new', pattern: '*' });
    });
    // resolve the stale first request late — must be discarded
    await act(async () => {
      release(
        batch({
          scannedKeys: 999,
          cursor: 999,
          matched: [{ key: 'stale', matchedIn: 'value', preview: 'S' }],
        }),
      );
    });
    await drain();

    expect(result.current.state.status).toBe('done');
    expect(result.current.state.hits.map((h) => h.key)).toEqual(['new']);
    expect(result.current.state.scanned).toBe(10);
  });

  it('reports limitHit and stops without scheduling more', async () => {
    scanValues.mockResolvedValueOnce(
      batch({
        scannedKeys: 50_000,
        cursor: 77,
        limitHit: true,
        matched: [{ key: 'z', matchedIn: 'value', preview: 'zz' }],
      }),
    );

    const { result } = renderHook(() => useValueSearch({ dbSessionId: 's', dbIndex: 0 }));
    act(() => {
      result.current.start({ mode: 'value', query: 'z', pattern: '*', maxKeys: 50_000 });
    });
    await drain(3);

    expect(result.current.state.status).toBe('done');
    expect(result.current.state.limitHit).toBe(true);
    const calls = scanValues.mock.calls.length;
    await drain(5);
    expect(scanValues.mock.calls.length).toBe(calls);
  });
});
