import { useMemo } from 'react';
import { AlertTriangle, History, Loader2, Pencil, RefreshCw, Trash2 } from 'lucide-react';
import { ChartCanvas } from '../../components/chart/ChartCanvas';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { Button } from '../../components/ui/Button';
import { hasRenderableChart, widgetRunToChartData } from '../../lib/dashboard/runToChart';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';
import type { DashboardWidget, ViewMode, WidgetRun } from '../../types/dashboard';

export interface ChartWidgetTileProps {
  widget: DashboardWidget;
  run: WidgetRun | null;
  busy?: boolean;
  onEdit: () => void;
  onDelete?: () => void;
  onHistory: () => void;
  onRefresh: () => void;
  onViewModeChange?: (mode: ViewMode) => void;
}

export function ChartWidgetTile({
  widget,
  run,
  busy,
  onEdit,
  onDelete,
  onHistory,
  onRefresh,
  onViewModeChange,
}: Readonly<ChartWidgetTileProps>) {
  const { t } = useI18n();
  const viewMode = widget.viewMode;

  const chartData = useMemo(
    () =>
      run && viewMode === 'chart' && widget.chartConfig
        ? widgetRunToChartData(run, widget.chartConfig)
        : null,
    [run, widget.chartConfig, viewMode],
  );

  const tableColumns = useMemo<ColumnDef[]>(
    () =>
      run?.columns.map((name) => ({
        id: name,
        name,
        type: 'unknown',
      })) ?? [],
    [run?.columns],
  );

  const hasChart = widget.chartConfig ? hasRenderableChart(chartData, widget.chartConfig) : false;

  return (
    <div
      className={cn(
        'flex min-h-0 flex-col overflow-hidden rounded-lg border border-edge bg-surface-raised',
        !widget.enabled && 'opacity-60',
      )}
      data-testid="dashboard-tile"
      data-widget-id={widget.id}
      style={{
        gridColumn: `${widget.layout.x + 1} / span ${widget.layout.w}`,
        gridRow: `${widget.layout.y + 1} / span ${widget.layout.h}`,
      }}
    >
      <div
        className="flex shrink-0 items-center gap-1 border-b border-edge px-2 py-1.5"
        data-no-drag
      >
        <span className="min-w-0 flex-1 truncate text-xs font-medium text-fg">{widget.title}</span>
        {run?.alertFired && (
          <span title={t('dashboard.alertFired')}>
            <AlertTriangle className="h-3.5 w-3.5 shrink-0 text-amber-500" />
          </span>
        )}
        {onViewModeChange && (
          <div
            className="flex rounded border border-edge text-[10px]"
            data-testid="dashboard-view-toggle"
          >
            {(['chart', 'table'] as const).map((mode) => (
              <button
                key={mode}
                type="button"
                data-testid={mode === 'chart' ? 'widget-view-chart' : 'widget-view-table'}
                className={cn(
                  'px-1.5 py-0.5 capitalize',
                  viewMode === mode ? 'bg-accent/20 text-accent' : 'text-fg-muted hover:text-fg',
                )}
                onClick={() => onViewModeChange(mode)}
              >
                {mode === 'chart' ? t('dashboard.viewChart') : t('dashboard.viewTable')}
              </button>
            ))}
          </div>
        )}
        <Button
          variant="ghost"
          className="h-6 w-6 px-0"
          data-testid="dashboard-tile-refresh"
          title={t('dashboard.refreshWidget')}
          disabled={busy}
          onClick={onRefresh}
        >
          {busy ? (
            <Loader2 className="h-3.5 w-3.5 animate-spin" />
          ) : (
            <RefreshCw className="h-3.5 w-3.5" />
          )}
        </Button>
        <Button
          variant="ghost"
          className="h-6 w-6 px-0"
          data-testid="dashboard-tile-history"
          title={t('dashboard.history')}
          onClick={onHistory}
        >
          <History className="h-3.5 w-3.5" />
        </Button>
        <Button
          variant="ghost"
          className="h-6 w-6 px-0"
          data-testid="dashboard-tile-edit"
          title={t('dashboard.editWidget')}
          onClick={onEdit}
        >
          <Pencil className="h-3.5 w-3.5" />
        </Button>
        {onDelete && (
          <Button
            variant="ghost"
            className="h-6 w-6 px-0 text-red-400 hover:text-red-300"
            data-testid="dashboard-tile-delete"
            title={t('dashboard.deleteWidget')}
            onClick={onDelete}
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        )}
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
        {run?.status === 'ok' && viewMode === 'chart' && !hasChart && (
          <div className="absolute inset-0 flex items-center justify-center px-3 text-center text-xs text-fg-muted">
            {run.rowCount === 0 ? t('dashboard.emptyResult') : t('dashboard.chartNotConfigured')}
          </div>
        )}
        {hasChart && chartData && viewMode === 'chart' && (
          <div className="h-full min-h-0" data-testid="dashboard-tile-chart">
            <ChartCanvas data={chartData.data} config={widget.chartConfig!} compact />
          </div>
        )}
        {run?.status === 'ok' && viewMode === 'table' && run.rows.length > 0 && (
          <div className="h-full min-h-0" data-testid="dashboard-tile-table">
            <DataTable
              columns={tableColumns}
              rows={run.rows}
              rowHeight={28}
              exportTableName={widget.title}
            />
          </div>
        )}
        {run?.status === 'ok' && viewMode === 'table' && run.rows.length === 0 && (
          <div className="absolute inset-0 flex items-center justify-center px-3 text-center text-xs text-fg-muted">
            {t('dashboard.emptyResult')}
          </div>
        )}
      </div>
    </div>
  );
}
