import { transformData, type TransformResult } from '../chart/transform';
import type { ChartConfig } from '../../types/chart';
import type { WidgetRun } from '../../types/dashboard';
import { widgetRunToStatementResult } from './runToResult';

/** Build chart transform output from a persisted run snapshot (no SQL re-execution). */
export function widgetRunToChartData(
  run: WidgetRun,
  chartConfig: ChartConfig,
): TransformResult | null {
  if (run.status !== 'ok' || run.rowCount === 0) return null;
  try {
    const sr = widgetRunToStatementResult(run);
    return transformData(sr, chartConfig);
  } catch {
    return null;
  }
}

export function hasRenderableChart(
  chartData: TransformResult | null,
  chartConfig: ChartConfig,
): boolean {
  return !!(chartData && chartData.data.length > 0 && chartConfig.yAxes.length > 0);
}
