import { describe, expect, it } from 'vitest';
import type { DriverCapabilities } from '../../types';
import { EMPTY_QUERY_EXEC, type QueryExecState } from '../../stores/queryExecActions';
import {
  getCancelCapability,
  isCancellationError,
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
    ).toBe('cancelled');
  });
});
