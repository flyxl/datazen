import { describe, expect, it } from 'vitest';
import type { WidgetRun, WidgetRunStatus } from '../../../types/dashboard';

const STATUSES: WidgetRunStatus[] = ['ok', 'error', 'timeout'];

describe('WidgetRunStatus', () => {
  it('accepts timeout status in WidgetRun shape', () => {
    const run: WidgetRun = {
      id: 'run-1',
      dashboardId: 'd1',
      widgetId: 'w1',
      startedAt: '2026-08-09T00:00:00.000Z',
      finishedAt: '2026-08-09T00:01:00.000Z',
      status: 'timeout',
      error: 'Query timed out after 60s',
      rowCount: 0,
      columns: [],
      rows: [],
    };
    expect(run.status).toBe('timeout');
    expect(STATUSES).toContain(run.status);
  });
});
