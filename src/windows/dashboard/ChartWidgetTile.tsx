import { useMemo } from 'react';
import { AlertTriangle, History, Loader2, Pencil, RefreshCw } from 'lucide-react';
import { ChartCanvas } from '../../components/chart/ChartCanvas';
import { Button } from '../../components/ui/Button';
import { hasRenderableChart, widgetRunToChartData } from '../../lib/dashboard/runToChart';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';
import type { DashboardWidget, WidgetRun } from '../../types/dashboard';

export interface ChartWidgetTileProps {
  widget: DashboardWidget;
  run: WidgetRun | null;
  busy?: boolean;
  onEdit: () => void;
  onHistory: () => void;
  onRefresh: () => void;
}

export function ChartWidgetTile({
  widget,
  run,
  busy,
  onEdit,
  onHistory,
  onRefresh,
}: Readonly<ChartWidgetTileProps>) {
  const { t } = useI18n();

  const chartData = useMemo(
    () => (run ? widgetRunToChartData(run, widget.chartConfig) : null),
    [run, widget.chartConfig],
  );

  const hasChart = hasRenderableChart(chartData, widget.chartConfig);

  return (
    <div
      className={cn(
        'flex min-h-0 flex-col overflow-hidden rounded-lg border border-edge bg-surface-raised',
        !widget.enabled && 'opacity-60',
      )}
      style={{
        gridColumn: `${widget.layout.x + 1} / span ${widget.layout.w}`,
        gridRow: `${widget.layout.y + 1} / span ${widget.layout.h}`,
      }}
    >
      <div className="flex shrink-0 items-center gap-1 border-b border-edge px-2 py-1.5" data-no-drag>
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-fg">{widget.title}</span>
        {run?.alertFired && (
          <span title={t('dashboard.alertFired')}>
            <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-amber-500" />
          </span>
        )}
        <Button
          variant="ghost"
          className="h-6 w-6 px-0"
          title={t('dashboard.refreshWidget')}
          disabled={busy}
          onClick={onRefresh}
        >
          {busy ? <Loader2 className="h-3.5 w-3.5 animate-spin" /> : <RefreshCw className="h-3.5 w-3.5" />}
        </Button>
        <Button variant="ghost" className="h-6 w-6 px-0" title={t('dashboard.history')} onClick={onHistory}>
          <History className="h-3.5 w-3.5" />
        </Button>
        <Button variant="ghost" className="h-6 w-6 px-0" title={t('dashboard.editWidget')} onClick={onEdit}>
          <Pencil className="h-3.5 w-3.5" />
        </Button>
      </div>

      <div className="relative min-h-0 flex-1">
        {busy && !run && (
          <div className="absolute inset-0 flex items-center justify-center text-xs text-fg-muted">
            <Loader2 className="mr-1.5 h-4 w-4 animate-spin" />
            {t('dashboard.running')}
          </div>
        )}
        {!busy && !run && (
          <div className="absolute inset-0 flex items-center justify-center px-3 text-center text-xs text-fg-muted">
            {t('dashboard.noRunYet')}
          </div>
        )}
        {run?.status === 'error' && (
          <div className="absolute inset-0 flex items-center justify-center px-3 text-center text-xs text-red-400">
            {run.error ?? t('dashboard.runError')}
          </div>
        )}
        {run?.status === 'timeout' && (
          <div className="absolute inset-0 flex items-center justify-center px-3 text-center text-xs text-amber-400">
            {t('dashboard.runTimeout')}
          </div>
        )}
        {run?.status === 'ok' && !hasChart && (
          <div className="absolute inset-0 flex items-center justify-center px-3 text-center text-xs text-fg-muted">
            {run.rowCount === 0 ? t('dashboard.emptyResult') : t('dashboard.chartNotConfigured')}
          </div>
        )}
        {hasChart && chartData && (
          <ChartCanvas data={chartData.data} config={widget.chartConfig} compact />
        )}
      </div>
    </div>
  );
}
