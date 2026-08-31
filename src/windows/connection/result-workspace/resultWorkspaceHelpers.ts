import type { StatementResult } from '../../../types';
import type { ChartConfig } from '../../../types/chart';
import { isChartableResult } from '../../../lib/chart/fieldInference';

export type ResultWorkspaceView = 'table' | 'chart';

export type ResultWorkspaceFallbackReason =
  | 'empty-result'
  | 'missing-chart-config'
  | 'not-chartable';

export interface ResultWorkspaceViewResolution {
  view: ResultWorkspaceView;
  chartAvailable: boolean;
  fallbackReason?: ResultWorkspaceFallbackReason;
}

/**
 * Chart rendering is opt-in for a result workspace. The caller owns the
 * config, so a missing config must not cause ChartView to invent state while a
 * result is being switched between views.
 */
export function canRenderResultChart(
  result: StatementResult | null | undefined,
  chartConfig: ChartConfig | null | undefined,
): boolean {
  return result != null && chartConfig != null && isChartableResult(result);
}

/**
 * Resolve the view that can actually be rendered without changing caller
 * state. In particular, this function does not invoke onViewChange: a parent
 * can decide whether to persist the fallback after rendering it.
 */
export function resolveResultWorkspaceView(
  result: StatementResult | null | undefined,
  requestedView: ResultWorkspaceView,
  chartConfig: ChartConfig | null | undefined,
): ResultWorkspaceViewResolution {
  const chartAvailable = canRenderResultChart(result, chartConfig);

  if (requestedView === 'table') {
    return { view: 'table', chartAvailable };
  }

  if (result == null || result.rows.length === 0 || result.columns.length === 0) {
    return { view: 'table', chartAvailable, fallbackReason: 'empty-result' };
  }

  if (chartConfig == null) {
    return { view: 'table', chartAvailable, fallbackReason: 'missing-chart-config' };
  }

  if (!isChartableResult(result)) {
    return { view: 'table', chartAvailable, fallbackReason: 'not-chartable' };
  }

  return { view: 'chart', chartAvailable };
}
