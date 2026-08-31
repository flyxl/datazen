import { BarChart3, TableProperties } from 'lucide-react';
import { useCallback } from 'react';
import { ChartView } from '../../../components/chart/ChartView';
import { useI18n } from '../../../hooks/useI18n';
import { tid } from '../../../lib/tid';
import type { StatementResult } from '../../../types';
import type { ChartConfig } from '../../../types/chart';
import { cn } from '../../../lib/cn';
import { ResultTableView } from './ResultTableView';
import { resolveResultWorkspaceView, type ResultWorkspaceView } from './resultWorkspaceHelpers';

export interface ResultWorkspaceProps {
  result?: StatementResult | null;
  view: ResultWorkspaceView;
  chartConfig?: ChartConfig | null;
  rowDetailIndex?: number | null;
  error?: string | null;
  onViewChange?: (view: ResultWorkspaceView) => void;
  onChartConfigChange?: (config: ChartConfig) => void;
  onRowDetail?: (rowIndex: number) => void;
  className?: string;
}

export function ResultWorkspace({
  result,
  view,
  chartConfig,
  rowDetailIndex = null,
  error,
  onViewChange,
  onChartConfigChange,
  onRowDetail,
  className,
}: ResultWorkspaceProps) {
  const { t } = useI18n();
  const resolution = resolveResultWorkspaceView(result, view, chartConfig);
  const handleChartDataPointClick = useCallback(
    (rowIndex: number) => {
      onViewChange?.('table');
      onRowDetail?.(rowIndex);
    },
    [onRowDetail, onViewChange],
  );
  if (error) {
    return <div className={cn('flex min-h-0 flex-1 items-center justify-center px-4', className)} role="alert" {...tid('result-workspace-error')}><p className="max-w-2xl text-sm text-red-400">{error}</p></div>;
  }
  if (!result) {
    return <div className={cn('flex min-h-0 flex-1 items-center justify-center text-sm text-fg-muted', className)} role="status" {...tid('result-workspace-empty')}>{t('sqlFile.noResults')}</div>;
  }
  return (
    <div className={cn('flex min-h-0 flex-1 flex-col overflow-hidden', className)} {...tid('result-workspace')}>
      <div className="flex shrink-0 items-center border-b border-edge bg-surface-alt px-2">
        <div className="my-1 flex items-center gap-0.5 rounded-md bg-surface p-0.5">
          <button type="button" {...tid('result-workspace-view-table')} className={cn('flex items-center gap-1 rounded px-2 py-0.5 text-xs transition-colors', resolution.view === 'table' ? 'bg-accent/20 font-medium text-accent' : 'text-fg-muted hover:text-fg-secondary')} aria-pressed={resolution.view === 'table'} onClick={() => onViewChange?.('table')}>
            <TableProperties className="h-3 w-3" />{t('chart.viewTable')}
          </button>
          <button type="button" {...tid('result-workspace-view-chart')} className={cn('flex items-center gap-1 rounded px-2 py-0.5 text-xs transition-colors', resolution.view === 'chart' ? 'bg-accent/20 font-medium text-accent' : 'text-fg-muted hover:text-fg-secondary')} aria-pressed={resolution.view === 'chart'} disabled={!resolution.chartAvailable} onClick={() => onViewChange?.('chart')}>
            <BarChart3 className="h-3 w-3" />{t('chart.viewChart')}
          </button>
        </div>
      </div>
      {resolution.view === 'table' ? (
        <ResultTableView result={result} rowDetailIndex={rowDetailIndex} onRowDetail={onRowDetail} />
      ) : (
        <ChartView result={result} savedConfig={chartConfig ?? undefined} onConfigChange={onChartConfigChange} onDataPointClick={handleChartDataPointClick} />
      )}
    </div>
  );
}
