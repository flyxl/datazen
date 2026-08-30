import { describe, expect, it } from 'vitest';
import type { DriverCapabilities } from '../../types';
import { EMPTY_QUERY_EXEC, type QueryExecState } from '../../stores/queryExecActions';
import {
  getCancelCapability,
  getCancelActionState,
  isCancellationError,
  reduceQueryExecutionState,
  toQueryExecutionViewModel,
} from '../queryExecutionViewModel';

const supported: DriverCapabilities = {
  supportsCancelQuery: true,
  supportsExplain: true,
  supportsStreamingResults: true,
};

const unsupported: DriverCapabilities = {
  ...supported,
  supportsCancelQuery: false,
};

function exec(overrides: Partial<QueryExecState> = {}): QueryExecState {
  return { ...EMPTY_QUERY_EXEC, ...overrides };
}

describe('queryExecutionViewModel', () => {
  it('distinguishes supported, unsupported, and unknown cancel capability', () => {
    expect(getCancelCapability(supported)).toBe('supported');
    expect(getCancelCapability(unsupported)).toBe('unsupported');
    expect(getCancelCapability(undefined)).toBe('unknown');
  });

  it('shows a running query as cancellable only when the driver supports it', () => {
    const running = exec({ running: true });

    expect(toQueryExecutionViewModel(running, supported)).toMatchObject({
      phase: 'running',
      cancelCapability: 'supported',
      cancelState: 'available',
    });
    expect(toQueryExecutionViewModel(running, unsupported)).toMatchObject({
      phase: 'running',
      cancelCapability: 'unsupported',
      cancelState: 'unavailable',
    });
    expect(toQueryExecutionViewModel(running, undefined)).toMatchObject({
      phase: 'running',
      cancelCapability: 'unknown',
      cancelState: 'unavailable',
    });
  });

  it('keeps a query in cancel_requested until execution reaches a terminal state', () => {
    expect(
      toQueryExecutionViewModel(exec({ running: true, cancelState: 'requested' }), supported),
    ).toMatchObject({
      phase: 'cancel_requested',
      cancelState: 'requested',
    });
  });

  it('reports cancel command failure without stopping the query', () => {
    expect(
      toQueryExecutionViewModel(
        exec({ running: true, cancelState: 'failed', cancelError: 'cancel failed' }),
        supported,
      ),
    ).toMatchObject({
      phase: 'running',
      cancelState: 'failed',
      error: 'cancel failed',
    });
  });

  it('reports cancellation only after the query has a cancellation terminal state', () => {
    expect(
      toQueryExecutionViewModel(exec({ terminalState: 'cancelled', error: null }), supported).phase,
    ).toBe('cancelled');
    expect(isCancellationError('canceling statement due to user request')).toBe(true);
    expect(
      toQueryExecutionViewModel(
        exec({ error: 'canceling statement due to user request' }),
        supported,
      ).phase,
    ).toBe('failed');
    expect(toQueryExecutionViewModel(exec({ terminalState: 'unknown' }), supported).phase).toBe(
      'outcome_unknown',
    );
  });

  it('derives the cancel action state from phase and capability', () => {
    const requested = toQueryExecutionViewModel(
      exec({ running: true, cancelState: 'requested' }),
      supported,
    );
    expect(getCancelActionState(requested)).toBe('requested');
    expect(
      getCancelActionState(
        toQueryExecutionViewModel(exec({ running: true }), unsupported),
      ),
    ).toBe('unavailable');
  });

  it('uses one reducer for request, failure, and terminal transitions', () => {
    const running = reduceQueryExecutionState(exec(), { type: 'start' });
    expect(running.running).toBe(true);

    const requested = reduceQueryExecutionState(running, { type: 'cancel_requested' });
    expect(requested).toMatchObject({ running: true, cancelState: 'requested' });

    const failed = reduceQueryExecutionState(requested, {
      type: 'cancel_failed',
      error: 'cancel failed',
    });
    expect(failed).toMatchObject({
      running: true,
      cancelState: 'failed',
      cancelError: 'cancel failed',
      terminalState: null,
    });

    const cancelled = reduceQueryExecutionState(requested, { type: 'cancelled' });
    expect(cancelled).toMatchObject({
      running: false,
      cancelState: 'idle',
      terminalState: 'cancelled',
    });
  });
});
