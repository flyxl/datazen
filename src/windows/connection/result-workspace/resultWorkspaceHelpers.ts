import type { StatementResult } from '../../../types';
import type { ChartConfig } from '../../../types/chart';
import { isChartableResult } from '../../../lib/chart/fieldInference';

export type ResultWorkspaceView = 'table' | 'chart';

export type ResultWorkspaceFallbackReason =
  | 'empty-result'
  | 'not-chartable';

export interface ResultWorkspaceViewResolution {
  view: ResultWorkspaceView;
  chartAvailable: boolean;
  fallbackReason?: ResultWorkspaceFallbackReason;
}

/**
 * A result is chart-capable when ChartView can infer at least one numeric
 * field. ChartView owns deriving its initial config when the caller has not
 * persisted one yet.
 */
export function canRenderResultChart(
  result: StatementResult | null | undefined,
  _chartConfig: ChartConfig | null | undefined,
): boolean {
  return result != null && isChartableResult(result);
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

  if (!isChartableResult(result)) {
    return { view: 'table', chartAvailable, fallbackReason: 'not-chartable' };
  }

  return { view: 'chart', chartAvailable };
}
