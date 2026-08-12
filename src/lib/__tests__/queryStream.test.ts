import { describe, expect, it } from 'vitest';
import { applyQueryStreamEvent } from '../queryStream';
import type { StatementResult } from '../../types';

function tab(results: StatementResult[] = []) {
  return {
    results,
    running: true,
    error: null as string | null,
    executionTimeMs: null as number | null,
  };
}

describe('applyQueryStreamEvent', () => {
  it('appends row chunks without treating batch size as a SQL limit', () => {
    let state = applyQueryStreamEvent(tab(), {
      type: 'statementStart',
      index: 0,
      sql: 'SELECT * FROM t',
      columns: [{ name: 'id', dataType: 'int', nullable: true }],
    });
    state = applyQueryStreamEvent(state, {
      type: 'rows',
      index: 0,
      rows: [[1], [2], [3]],
    });
    state = applyQueryStreamEvent(state, {
      type: 'rows',
      index: 0,
      rows: [[4], [5]],
    });
    expect(state.results[0].rows).toHaveLength(5);
    expect(state.results[0].truncated).toBe(false);

    state = applyQueryStreamEvent(state, {
      type: 'statementEnd',
      index: 0,
      rowsAffected: 5,
      executionTimeMs: 12,
      truncated: false,
    });
    expect(state.results[0].truncated).toBe(false);
    expect(state.results[0].executionTimeMs).toBe(12);
  });

  it('records SQL-limit truncation only from statementEnd', () => {
    let state = applyQueryStreamEvent(tab(), {
      type: 'statementStart',
      index: 0,
      sql: 'SELECT * FROM t',
      columns: [{ name: 'id', dataType: 'int', nullable: true }],
    });
    state = applyQueryStreamEvent(state, {
      type: 'rows',
      index: 0,
      rows: Array.from({ length: 500 }, (_, i) => [i]),
    });
    state = applyQueryStreamEvent(state, {
      type: 'statementEnd',
      index: 0,
      executionTimeMs: 9,
      truncated: true,
    });
    expect(state.results[0].rows).toHaveLength(500);
    expect(state.results[0].truncated).toBe(true);
  });

  it('done marks the tab idle', () => {
    const state = applyQueryStreamEvent(tab(), { type: 'done', totalTimeMs: 40 });
    expect(state.running).toBe(false);
    expect(state.executionTimeMs).toBe(40);
  });

  it('pads missing statement slots for later indexes', () => {
    const state = applyQueryStreamEvent(tab(), {
      type: 'statementStart',
      index: 1,
      sql: 'SELECT 2',
      columns: [],
    });
    expect(state.results).toHaveLength(2);
    expect(state.results[0].sql).toBe('');
    expect(state.results[1].sql).toBe('SELECT 2');
  });

  it('ignores row chunks for unknown indexes', () => {
    const state = applyQueryStreamEvent(tab(), {
      type: 'rows',
      index: 0,
      rows: [[1]],
    });
    expect(state.results).toHaveLength(0);
  });

  it('keeps an unknown event as a no-op', () => {
    const before = tab();
    const state = applyQueryStreamEvent(before, { type: 'noop' } as never);
    expect(state).toEqual(before);
  });
});
