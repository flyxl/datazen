import { describe, expect, it } from 'vitest';
import { widgetRunToStatementResult } from '../runToResult';
import type { WidgetRun } from '../../../types/dashboard';

const sampleRun: WidgetRun = {
  id: 'run-1',
  dashboardId: 'dash-1',
  widgetId: 'widget-1',
  startedAt: '2026-08-09T00:00:00.000Z',
  finishedAt: '2026-08-09T00:00:01.000Z',
  status: 'ok',
  rowCount: 1,
  columns: ['v'],
  rows: [[42]],
};

describe('widgetRunToStatementResult', () => {
  it('maps columns/rows into StatementResult shape', () => {
    const sr = widgetRunToStatementResult(sampleRun);
    expect(sr.columns.map((c) => c.name)).toEqual(['v']);
    expect(sr.rows).toHaveLength(1);
    expect(sr.rows[0]).toEqual([42]);
  });

  it('uses empty sql and zero execution time', () => {
    const sr = widgetRunToStatementResult(sampleRun);
    expect(sr.sql).toBe('');
    expect(sr.executionTimeMs).toBe(0);
  });
});
