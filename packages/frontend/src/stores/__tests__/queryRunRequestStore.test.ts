import { afterEach, describe, expect, it, vi } from 'vitest';
import { registerQueryRunner, requestQueryRun } from '../queryRunRequestStore';

const PANEL_ID = 'panel-run-test';

afterEach(() => {
  // The registry is process-wide; clear the panel used below so a queued
  // statement cannot leak into another test.
  registerQueryRunner(PANEL_ID, () => undefined)();
});

describe('query run request store', () => {
  it('runs immediately when the panel is mounted', () => {
    const runner = vi.fn();
    const unregister = registerQueryRunner(PANEL_ID, runner);

    expect(requestQueryRun(PANEL_ID, 'SELECT 1')).toBe(true);
    expect(runner).toHaveBeenCalledWith('SELECT 1');

    unregister();
  });

  it('queues a request until the query panel registers, then flushes once', () => {
    expect(requestQueryRun(PANEL_ID, 'SELECT 2')).toBe(false);

    const runner = vi.fn();
    const unregister = registerQueryRunner(PANEL_ID, runner);
    expect(runner).toHaveBeenCalledTimes(1);
    expect(runner).toHaveBeenCalledWith('SELECT 2');

    expect(requestQueryRun(PANEL_ID, 'SELECT 3')).toBe(true);
    expect(runner).toHaveBeenCalledTimes(2);
    unregister();
  });

  it('queues consecutive requests in order instead of keeping only the last', () => {
    expect(requestQueryRun(PANEL_ID, 'SELECT 2a')).toBe(false);
    expect(requestQueryRun(PANEL_ID, 'SELECT 2b')).toBe(false);

    const runner = vi.fn();
    const unregister = registerQueryRunner(PANEL_ID, runner);
    expect(runner.mock.calls).toEqual([['SELECT 2a'], ['SELECT 2b']]);
    unregister();
  });

  it('does not call a stale runner after unregister', () => {
    const runner = vi.fn();
    const unregister = registerQueryRunner(PANEL_ID, runner);
    unregister();

    expect(requestQueryRun(PANEL_ID, 'SELECT 4')).toBe(false);
    expect(runner).not.toHaveBeenCalled();
  });
});
