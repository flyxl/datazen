import { describe, expect, it } from 'vitest';
import { DEFAULT_CHART_CONFIG } from '../../../types/chart';
import type { WidgetRun } from '../../../types/dashboard';
import { hasRenderableChart, widgetRunToChartData } from '../runToChart';

const okRun: WidgetRun = {
  id: 'run-1',
  dashboardId: 'dash-1',
  widgetId: 'widget-1',
  startedAt: '2026-08-09T00:00:00.000Z',
  finishedAt: '2026-08-09T00:00:01.000Z',
  status: 'ok',
  rowCount: 2,
  columns: ['x', 'v'],
  rows: [['a', 1], ['b', 2]],
};

describe('widgetRunToChartData', () => {
  it('returns chart data for ok runs', () => {
    const chartConfig = { ...DEFAULT_CHART_CONFIG, xAxis: 'x', yAxes: ['v'] };
    const result = widgetRunToChartData(okRun, chartConfig);
    expect(result).not.toBeNull();
    expect(result!.data).toHaveLength(2);
  });

  it('returns null for error runs', () => {
    const result = widgetRunToChartData({ ...okRun, status: 'error' }, DEFAULT_CHART_CONFIG);
    expect(result).toBeNull();
  });

  it('returns null when rowCount is zero', () => {
    const result = widgetRunToChartData({ ...okRun, rowCount: 0, rows: [] }, DEFAULT_CHART_CONFIG);
    expect(result).toBeNull();
  });
});

describe('hasRenderableChart', () => {
  it('requires data points and y axes', () => {
    const chartConfig = { ...DEFAULT_CHART_CONFIG, yAxes: ['v'] };
    const data = widgetRunToChartData(okRun, chartConfig);
    expect(hasRenderableChart(data, chartConfig)).toBe(true);
    expect(hasRenderableChart(data, { ...chartConfig, yAxes: [] })).toBe(false);
    expect(hasRenderableChart(null, chartConfig)).toBe(false);
  });
});
