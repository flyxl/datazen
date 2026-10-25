/**
 * `useColumnStats` (DB-12): the lifecycle of one column's server-side
 * aggregates.
 *
 * The streamer is injected, so the tests decide what the "server" answers:
 * the contract under lock is the request shape (dialect-quoted SQL, no
 * history recording, target routing) and the state machine (loading →
 * ready/error, last request wins, dismiss closes and forgets).
 */
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act, renderHook } from '@testing-library/react';
import type { QueryStreamEvent, Value } from '../../types';
import { useColumnStats, type ColumnStatsRequest } from '../useColumnStats';

const REQUEST: ColumnStatsRequest = {
  dbSessionId: 'session-1',
  database: 'shop',
  databaseType: 'postgresql',
  schema: 'public',
  tableName: 'orders',
};

type StreamCall = {
  dbSessionId: string;
  sql: string;
  onEvent: (event: QueryStreamEvent) => void;
  options?: {
    applyResultLimit?: boolean;
    recordHistory?: boolean;
    database?: string | null;
    schema?: string | null;
  };
};

function fakeStreamer(
  calls: StreamCall[],
  behavior: {
    rows?: (Value | null)[][];
    error?: Error;
    delay?: () => Promise<void>;
  },
) {
  return async (
    dbSessionId: string,
    sql: string,
    onEvent: (event: QueryStreamEvent) => void,
    options?: StreamCall['options'],
  ): Promise<void> => {
    calls.push({ dbSessionId, sql, onEvent, options });
    if (behavior.delay) await behavior.delay();
    if (behavior.error) throw behavior.error;
    if (behavior.rows) onEvent({ type: 'rows', index: 0, rows: behavior.rows });
  };
}

describe('useColumnStats (DB-12)', () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it('streams the quoted aggregate without recording history, then reports ready', async () => {
    const calls: StreamCall[] = [];
    let releaseStream!: () => void;
    const streamGate = new Promise<void>((resolve) => {
      releaseStream = resolve;
    });
    const streamQuery = fakeStreamer(calls, {
      rows: [[1_000_000, 999_994, 7, 'a', 'z', 19.5]],
      delay: () => streamGate,
    });
    const { result } = renderHook(() => useColumnStats({ streamQuery }));

    expect(result.current.state.status).toBe('idle');
    act(() => {
      result.current.open(REQUEST, 'price', 'numeric(10,2)');
    });

    // The stream is gated open, so this is what the user sees while the
    // server aggregates.
    expect(result.current.state.status).toBe('loading');
    expect(result.current.state.column).toBe('price');

    // One round trip: dialect-quoted, schema-qualified, AVG included for a
    // numeric column, and never written into the user's query history.
    expect(calls).toHaveLength(1);
    expect(calls[0].sql).toBe(
      'SELECT COUNT(*) AS row_count, COUNT("price") AS non_null_count, ' +
        'COUNT(DISTINCT "price") AS distinct_count, MIN("price") AS min_value, ' +
        'MAX("price") AS max_value, AVG("price") AS avg_value FROM "public"."orders"',
    );
    expect(calls[0].dbSessionId).toBe('session-1');
    expect(calls[0].options).toMatchObject({
      applyResultLimit: false,
      recordHistory: false,
      database: 'shop',
      schema: 'public',
    });

    act(() => {
      releaseStream();
    });
    await vi.waitFor(() => {
      expect(result.current.state.status).toBe('ready');
    });
    expect(result.current.state.stats).toEqual({
      rowCount: 1_000_000,
      nonNullCount: 999_994,
      nullCount: 6,
      distinctCount: 7,
      min: 'a',
      max: 'z',
      avg: 19.5,
      supportsAvg: true,
    });
  });

  it('omits the AVG projection for a text column', () => {
    const calls: StreamCall[] = [];
    const streamQuery = fakeStreamer(calls, { rows: [[10, 10, 1, 'x', 'x']] });
    const { result } = renderHook(() => useColumnStats({ streamQuery }));
    act(() => {
      result.current.open(REQUEST, 'note', 'text');
    });
    expect(calls[0].sql).not.toContain('AVG');
    expect(calls[0].sql).toContain('COUNT(DISTINCT "note")');
  });

  it('a rejected stream reports the error and keeps the column', async () => {
    const calls: StreamCall[] = [];
    const streamQuery = fakeStreamer(calls, { error: new Error('avg(text) does not exist') });
    const { result } = renderHook(() => useColumnStats({ streamQuery }));
    act(() => {
      result.current.open(REQUEST, 'note', 'integer');
    });
    await vi.waitFor(() => {
      expect(result.current.state.status).toBe('error');
    });
    expect(result.current.state.error).toContain('avg(text)');
    expect(result.current.state.column).toBe('note');
    expect(result.current.state.stats).toBeNull();
  });

  it('a late answer for a superseded request lands nowhere', async () => {
    const calls: StreamCall[] = [];
    let releaseFirst!: () => void;
    const firstGate = new Promise<void>((resolve) => {
      releaseFirst = resolve;
    });
    const streamQuery = async (
      dbSessionId: string,
      sql: string,
      onEvent: (event: QueryStreamEvent) => void,
      options?: StreamCall['options'],
    ) => {
      const isPrice = sql.includes('"price"');
      calls.push({ dbSessionId, sql, onEvent, options });
      if (isPrice) await firstGate;
      onEvent({ type: 'rows', index: 0, rows: [[5, 5, 1, 'x', 'x']] });
    };
    const { result } = renderHook(() => useColumnStats({ streamQuery }));

    act(() => {
      result.current.open(REQUEST, 'price', 'integer');
    });
    act(() => {
      result.current.open(REQUEST, 'note', 'text');
    });
    await vi.waitFor(() => {
      expect(result.current.state.status).toBe('ready');
    });
    expect(result.current.state.column).toBe('note');

    // The first column finally answers — over a request the user already
    // replaced. It must not overwrite the second column's numbers.
    act(() => {
      releaseFirst();
    });
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.state.column).toBe('note');
    expect(result.current.state.status).toBe('ready');
  });

  it('dismiss returns to idle and a late answer reopens nothing', async () => {
    const calls: StreamCall[] = [];
    let release!: () => void;
    const gate = new Promise<void>((resolve) => {
      release = resolve;
    });
    const streamQuery = async (
      dbSessionId: string,
      sql: string,
      onEvent: (event: QueryStreamEvent) => void,
      options?: StreamCall['options'],
    ) => {
      calls.push({ dbSessionId, sql, onEvent, options });
      await gate;
      onEvent({ type: 'rows', index: 0, rows: [[5, 5, 1, 'x', 'x']] });
    };
    const { result } = renderHook(() => useColumnStats({ streamQuery }));

    act(() => {
      result.current.open(REQUEST, 'price', 'integer');
    });
    act(() => {
      result.current.dismiss();
    });
    expect(result.current.state.status).toBe('idle');

    act(() => {
      release();
    });
    await act(async () => {
      await Promise.resolve();
    });
    expect(result.current.state.status).toBe('idle');
  });

  it('a stream that ends without a row is an error, never zeros', async () => {
    const calls: StreamCall[] = [];
    const streamQuery = fakeStreamer(calls, { rows: [] });
    const { result } = renderHook(() => useColumnStats({ streamQuery }));
    act(() => {
      result.current.open(REQUEST, 'price', 'integer');
    });
    await vi.waitFor(() => {
      expect(result.current.state.status).toBe('error');
    });
    expect(result.current.state.error).toContain('empty');
  });
});
