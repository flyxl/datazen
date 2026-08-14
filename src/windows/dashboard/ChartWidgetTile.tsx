import { useCallback, useMemo, useState } from 'react';
import { createPortal } from 'react-dom';
import {
  AlertTriangle,
  BarChart3,
  History,
  LineChart as LineChartIcon,
  Loader2,
  Maximize2,
  Minimize2,
  Pencil,
  PieChart as PieChartIcon,
  RefreshCw,
  ScatterChart as ScatterChartIcon,
  Trash2,
  TrendingUp,
} from 'lucide-react';
import { ChartCanvas } from '../../components/chart/ChartCanvas';
import { AxisConfigurator } from '../../components/chart/AxisConfigurator';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { Button } from '../../components/ui/Button';
import { hasRenderableChart, widgetRunToChartData } from '../../lib/dashboard/runToChart';
import { widgetRunToStatementResult } from '../../lib/dashboard/runToResult';
import { inferAllFields } from '../../lib/chart/fieldInference';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';
import type { DashboardWidget, ViewMode, WidgetRun } from '../../types/dashboard';
import type { ChartConfig, ChartType } from '../../types/chart';

const CHART_TYPE_ICONS: { type: ChartType; icon: React.ElementType; labelKey: string }[] = [
  { type: 'bar', icon: BarChart3, labelKey: 'chart.type.bar' },
  { type: 'line', icon: LineChartIcon, labelKey: 'chart.type.line' },
  { type: 'pie', icon: PieChartIcon, labelKey: 'chart.type.pie' },
  { type: 'scatter', icon: ScatterChartIcon, labelKey: 'chart.type.scatter' },
  { type: 'area', icon: TrendingUp, labelKey: 'chart.type.area' },
];

export interface ChartWidgetTileProps {
  widget: DashboardWidget;
  run: WidgetRun | null;
  busy?: boolean;
  onEdit: () => void;
  onDelete?: () => void;
  onHistory: () => void;
  onRefresh: () => void;
  onViewModeChange?: (mode: ViewMode) => void;
  onChartTypeChange?: (type: ChartType) => void;
  onChartConfigChange?: (config: ChartConfig) => void;
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
  onChartTypeChange,
  onChartConfigChange,
}: Readonly<ChartWidgetTileProps>) {
  const { t } = useI18n();
  const viewMode = widget.viewMode;
  const [expanded, setExpanded] = useState(false);

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
        'flex h-80 flex-col overflow-hidden rounded-lg border border-edge bg-surface-raised',
        !widget.enabled && 'opacity-60',
      )}
      data-testid="dashboard-tile"
      data-widget-id={widget.id}
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
        {onChartTypeChange && viewMode === 'chart' && (
          <div
            className="flex items-center gap-0.5 rounded border border-edge p-0.5"
            data-testid="dashboard-chart-type-toggle"
          >
            {CHART_TYPE_ICONS.map(({ type, icon: Icon, labelKey }) => (
              <button
                key={type}
                type="button"
                title={t(labelKey as never)}
                className={cn(
                  'rounded p-0.5 transition-colors',
                  widget.chartConfig?.chartType === type
                    ? 'bg-accent/20 text-accent'
                    : 'text-fg-muted hover:text-fg-secondary',
                )}
                onClick={() => onChartTypeChange(type)}
              >
                <Icon className="h-3 w-3" />
              </button>
            ))}
          </div>
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
        {run?.status === 'ok' && (
          <Button
            variant="ghost"
            className="h-6 w-6 px-0"
            data-testid="dashboard-tile-expand"
            title={t('chart.expand')}
            onClick={() => setExpanded(true)}
          >
            <Maximize2 className="h-3.5 w-3.5" />
          </Button>
        )}
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
          <div className="absolute inset-0 flex flex-col items-center justify-center gap-2 px-3 text-center text-xs text-fg-muted">
            <span>
              {run.rowCount === 0 ? t('dashboard.emptyResult') : t('dashboard.chartNotConfigured')}
            </span>
            {run.rowCount > 0 && (
              <Button
                variant="ghost"
                className="h-7 gap-1 px-2 text-xs text-accent"
                onClick={onEdit}
              >
                <Pencil className="h-3 w-3" />
                {t('dashboard.editWidget')}
              </Button>
            )}
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

      {expanded &&
        run?.status === 'ok' &&
        createPortal(
          <TileExpandOverlay
            widget={widget}
            run={run}
            chartData={chartData}
            tableColumns={tableColumns}
            hasChart={hasChart}
            viewMode={viewMode}
            t={t}
            onClose={() => setExpanded(false)}
            onChartConfigChange={onChartConfigChange}
          />,
          document.body,
        )}
    </div>
  );
}

function TileExpandOverlay({
  widget,
  run,
  tableColumns,
  viewMode,
  t,
  onClose,
  onChartConfigChange,
}: {
  widget: DashboardWidget;
  run: WidgetRun;
  chartData: ReturnType<typeof widgetRunToChartData> | null;
  tableColumns: ColumnDef[];
  hasChart: boolean;
  viewMode: string;
  t: ReturnType<typeof useI18n>['t'];
  onClose: () => void;
  onChartConfigChange?: (config: ChartConfig) => void;
}) {
  const [localConfig, setLocalConfig] = useState<ChartConfig>(
    () =>
      widget.chartConfig ?? {
        chartType: 'bar',
        xAxis: null,
        yAxes: [],
        showLegend: true,
        showGrid: true,
        showValues: false,
        aggregation: 'none',
        groupBy: null,
        sortBy: 'none',
        colorScheme: 'default',
      },
  );

  const fields = useMemo(() => {
    try {
      const sr = widgetRunToStatementResult(run);
      return inferAllFields(sr);
    } catch {
      return [];
    }
  }, [run]);

  const chartData = useMemo(() => widgetRunToChartData(run, localConfig), [run, localConfig]);
  const hasChart = hasRenderableChart(chartData, localConfig);

  const handleConfigChange = useCallback(
    (cfg: ChartConfig) => {
      setLocalConfig(cfg);
      onChartConfigChange?.(cfg);
    },
    [onChartConfigChange],
  );

  return (
    <div
      className="fixed inset-0 top-10 z-50 flex flex-col bg-surface/95 backdrop-blur-sm"
      onKeyDown={(e) => {
        if (e.key === 'Escape') onClose();
      }}
    >
      <div className="flex shrink-0 items-center gap-2 border-b border-edge px-4 py-2">
        <span className="text-sm font-medium text-fg">{widget.title}</span>
        <div className="flex-1" />
        <button
          type="button"
          className="flex items-center gap-1 rounded px-2 py-1 text-xs text-fg-muted hover:bg-surface-alt hover:text-fg-secondary transition-colors"
          onClick={onClose}
          title={t('chart.collapse')}
        >
          <Minimize2 className="h-3.5 w-3.5" />
        </button>
      </div>
      <div className="flex min-h-0 flex-1">
        {viewMode === 'chart' && fields.length > 0 && (
          <AxisConfigurator
            fields={fields}
            config={localConfig}
            onChange={handleConfigChange}
            recommendation={null}
          />
        )}
        <div className="relative min-h-0 flex-1">
          {viewMode === 'chart' && hasChart && chartData && (
            <div className="h-full min-h-0">
              <ChartCanvas data={chartData.data} config={localConfig} />
            </div>
          )}
          {viewMode === 'chart' && !hasChart && (
            <div className="flex h-full items-center justify-center text-xs text-fg-muted">
              {run.rowCount === 0 ? t('dashboard.emptyResult') : t('dashboard.chartNotConfigured')}
            </div>
          )}
          {viewMode === 'table' && run.rows.length > 0 && (
            <div className="h-full min-h-0">
              <DataTable
                columns={tableColumns}
                rows={run.rows}
                rowHeight={28}
                exportTableName={widget.title}
              />
            </div>
          )}
          {viewMode === 'table' && run.rows.length === 0 && (
            <div className="flex h-full items-center justify-center text-xs text-fg-muted">
              {t('dashboard.emptyResult')}
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
