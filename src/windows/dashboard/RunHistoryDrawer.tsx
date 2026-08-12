import { useCallback, useEffect, useMemo, useState } from 'react';
import { AlertTriangle, Loader2, X } from 'lucide-react';
import { ChartCanvas } from '../../components/chart/ChartCanvas';
import { DataTable } from '../../components/DataTable/DataTable';
import type { ColumnDef } from '../../components/DataTable/TableHeader';
import { Button } from '../../components/ui/Button';
import { dashboardCommands } from '../../commands/dashboard';
import { formatLastConnected } from '../../lib/formatters';
import { hasRenderableChart, widgetRunToChartData } from '../../lib/dashboard/runToChart';
import { cn } from '../../lib/cn';
import { useI18n } from '../../hooks/useI18n';
import type { DashboardWidget, RunIndexEntry, ViewMode, WidgetRun } from '../../types/dashboard';

const HISTORY_LIMIT = 50;

export interface RunHistoryDrawerProps {
  open: boolean;
  dashboardId: string;
  widget: DashboardWidget | null;
  onClose: () => void;
}

function statusClass(status: RunIndexEntry['status']): string {
  switch (status) {
    case 'ok':
      return 'text-emerald-500';
    case 'error':
      return 'text-red-400';
    case 'timeout':
      return 'text-amber-400';
  }
}

export function RunHistoryDrawer({
  open,
  dashboardId,
  widget,
  onClose,
}: Readonly<RunHistoryDrawerProps>) {
  const { t } = useI18n();
  const [index, setIndex] = useState<RunIndexEntry[]>([]);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [loadedRun, setLoadedRun] = useState<WidgetRun | null>(null);
  const [loadingIndex, setLoadingIndex] = useState(false);
  const [loadingRun, setLoadingRun] = useState(false);
  const [indexError, setIndexError] = useState<string | null>(null);
  const [runError, setRunError] = useState<string | null>(null);
  const [viewMode, setViewMode] = useState<ViewMode>('chart');

  useEffect(() => {
    if (!open || !widget) return;
    let cancelled = false;
    setSelectedId(null);
    setLoadedRun(null);
    setIndexError(null);
    setRunError(null);
    setViewMode(widget.viewMode);
    setLoadingIndex(true);
    void dashboardCommands
      .listWidgetRuns(dashboardId, widget.id, HISTORY_LIMIT)
      .then((entries) => {
        if (cancelled) return;
        setIndex(entries);
        if (entries.length > 0) setSelectedId(entries[0].id);
      })
      .catch((e) => {
        if (cancelled) return;
        setIndexError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoadingIndex(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, dashboardId, widget]);

  useEffect(() => {
    if (!open || !widget || !selectedId) {
      setLoadedRun(null);
      return;
    }
    let cancelled = false;
    setLoadingRun(true);
    setRunError(null);
    void dashboardCommands
      .getWidgetRun(dashboardId, widget.id, selectedId)
      .then((run) => {
        if (cancelled) return;
        setLoadedRun(run);
      })
      .catch((e) => {
        if (cancelled) return;
        setLoadedRun(null);
        setRunError(String(e));
      })
      .finally(() => {
        if (!cancelled) setLoadingRun(false);
      });
    return () => {
      cancelled = true;
    };
  }, [open, dashboardId, widget, selectedId]);

  const chartConfig = widget?.chartConfig;

  const chartData = useMemo(() => {
    if (!loadedRun || !chartConfig) return null;
    return widgetRunToChartData(loadedRun, chartConfig);
  }, [loadedRun, chartConfig]);

  const hasChart = !!chartConfig && hasRenderableChart(chartData, chartConfig);

  const tableColumns = useMemo<ColumnDef[]>(
    () =>
      loadedRun?.columns.map((name) => ({
        id: name,
        name,
        type: 'unknown',
      })) ?? [],
    [loadedRun?.columns],
  );

  const handleSelect = useCallback((id: string) => setSelectedId(id), []);

  if (!open || !widget) return null;

  return (
    <div
      className="fixed inset-0 z-50 flex justify-end"
      data-no-drag
      data-testid="run-history-drawer"
    >
      <button
        type="button"
        className="absolute inset-0 bg-black/40"
        aria-label={t('common.close')}
        onClick={onClose}
      />
      <aside className="relative flex h-full w-full max-w-3xl flex-col border-l border-edge bg-surface shadow-xl">
        <div className="flex shrink-0 items-center justify-between border-b border-edge px-4 py-3">
          <div className="min-w-0">
            <h2 className="text-sm font-semibold text-fg">{t('dashboard.runHistory')}</h2>
            <p className="truncate text-xs text-fg-muted">{widget.title}</p>
          </div>
          <Button variant="ghost" className="h-7 w-7 px-0" onClick={onClose}>
            <X className="h-4 w-4" />
          </Button>
        </div>

        {indexError && (
          <div className="mx-4 mt-3 rounded-md border border-red-500/30 bg-red-500/10 px-3 py-2 text-xs text-red-400">
            {indexError}
          </div>
        )}

        <div className="flex min-h-0 flex-1">
          <div className="flex w-56 shrink-0 flex-col border-r border-edge">
            {loadingIndex && (
              <div className="flex flex-1 items-center justify-center text-xs text-fg-muted">
                <Loader2 className="mr-1.5 h-4 w-4 animate-spin" />
                {t('common.loading')}
              </div>
            )}
            {!loadingIndex && index.length === 0 && (
              <div className="flex flex-1 items-center justify-center px-3 text-center text-xs text-fg-muted">
                {t('dashboard.noHistory')}
              </div>
            )}
            {!loadingIndex && index.length > 0 && (
              <ul className="overflow-auto py-1">
                {index.map((entry) => (
                  <li key={entry.id}>
                    <button
                      type="button"
                      className={cn(
                        'flex w-full items-start gap-1.5 px-3 py-2 text-left text-xs hover:bg-surface-raised',
                        selectedId === entry.id && 'bg-surface-raised',
                      )}
                      onClick={() => handleSelect(entry.id)}
                    >
                      <span className="min-w-0 flex-1">
                        <span className="block truncate text-fg">
                          {formatLastConnected(entry.startedAt)}
                        </span>
                        <span className={cn('block capitalize', statusClass(entry.status))}>
                          {entry.status}
                        </span>
                      </span>
                      {entry.alertFired && (
                        <AlertTriangle className="mt-0.5 h-3.5 w-3.5 shrink-0 text-amber-500" />
                      )}
                    </button>
                  </li>
                ))}
              </ul>
            )}
          </div>

          <div className="relative min-h-0 min-w-0 flex-1 flex flex-col">
            {loadedRun?.status === 'ok' && (
              <div
                className="flex shrink-0 justify-end border-b border-edge px-3 py-1.5"
                data-testid="run-history-view-toggle"
              >
                <div className="flex rounded border border-edge text-[10px]">
                  {(['chart', 'table'] as const).map((mode) => (
                    <button
                      key={mode}
                      type="button"
                      className={cn(
                        'px-1.5 py-0.5 capitalize',
                        viewMode === mode
                          ? 'bg-accent/20 text-accent'
                          : 'text-fg-muted hover:text-fg',
                      )}
                      onClick={() => setViewMode(mode)}
                    >
                      {mode === 'chart' ? t('dashboard.viewChart') : t('dashboard.viewTable')}
                    </button>
                  ))}
                </div>
              </div>
            )}
            <div className="relative min-h-0 flex-1">
              {runError && (
                <div className="absolute inset-0 flex items-center justify-center px-4 text-center text-xs text-red-400">
                  {runError}
                </div>
              )}
              {!runError && !selectedId && !loadingIndex && (
                <div className="absolute inset-0 flex items-center justify-center px-4 text-center text-xs text-fg-muted">
                  {t('dashboard.selectRun')}
                </div>
              )}
              {loadingRun && (
                <div className="absolute inset-0 flex items-center justify-center text-xs text-fg-muted">
                  <Loader2 className="mr-1.5 h-4 w-4 animate-spin" />
                  {t('common.loading')}
                </div>
              )}
              {!loadingRun && loadedRun?.status === 'error' && (
                <div className="absolute inset-0 flex items-center justify-center px-4 text-center text-xs text-red-400">
                  {loadedRun.error ?? t('dashboard.runError')}
                </div>
              )}
              {!loadingRun && loadedRun?.status === 'timeout' && (
                <div className="absolute inset-0 flex items-center justify-center px-4 text-center text-xs text-amber-400">
                  {t('dashboard.runTimeout')}
                </div>
              )}
              {!loadingRun && loadedRun?.status === 'ok' && viewMode === 'chart' && !hasChart && (
                <div className="absolute inset-0 flex items-center justify-center px-4 text-center text-xs text-fg-muted">
                  {loadedRun.rowCount === 0
                    ? t('dashboard.emptyResult')
                    : t('dashboard.chartNotConfigured')}
                </div>
              )}
              {!loadingRun && hasChart && chartData && chartConfig && viewMode === 'chart' && (
                <ChartCanvas data={chartData.data} config={chartConfig} />
              )}
              {!loadingRun &&
                loadedRun?.status === 'ok' &&
                viewMode === 'table' &&
                loadedRun.rows.length > 0 && (
                  <DataTable
                    columns={tableColumns}
                    rows={loadedRun.rows}
                    rowHeight={28}
                    exportTableName={widget.title}
                  />
                )}
              {!loadingRun &&
                loadedRun?.status === 'ok' &&
                viewMode === 'table' &&
                loadedRun.rows.length === 0 && (
                  <div className="absolute inset-0 flex items-center justify-center px-4 text-center text-xs text-fg-muted">
                    {t('dashboard.emptyResult')}
                  </div>
                )}
            </div>
          </div>
        </div>
      </aside>
    </div>
  );
}
