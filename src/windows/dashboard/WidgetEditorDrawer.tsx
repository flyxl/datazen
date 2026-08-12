import { useCallback, useEffect, useState } from 'react';
import { X } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { useConnectionStore } from '../../stores/connectionStore';
import {
  clampRefreshSec,
  DEFAULT_REFRESH,
  MIN_REFRESH_SEC,
  normalizeRefreshPolicy,
  REFRESH_WARN_BELOW_SEC,
  shouldWarnRefreshSec,
} from '../../types/dashboard';
import type {
  AlertMetricAgg,
  AlertOperator,
  DashboardWidget,
  RefreshMode,
} from '../../types/dashboard';
import type { ChartType } from '../../types/chart';
import { DEFAULT_CHART_CONFIG } from '../../types/chart';

export interface WidgetEditorDrawerProps {
  open: boolean;
  widget: DashboardWidget | null;
  isNew?: boolean;
  /** When editing a hidden SQL workflow widget */
  hiddenSql?: { configId: string; sql: string };
  onClose: () => void;
  onSave: (widget: DashboardWidget, hiddenSql?: { configId: string; sql: string }) => void;
}

const CHART_TYPES: ChartType[] = ['bar', 'line', 'pie', 'scatter', 'area'];
const REFRESH_MODES: RefreshMode[] = ['manual', 'onOpen', 'interval'];
const ALERT_OPS: AlertOperator[] = ['>', '>=', '<', '<=', '==', '!='];
const ALERT_AGGS: AlertMetricAgg[] = ['last', 'max', 'min', 'avg', 'sum'];

const DEFAULT_ALERT: NonNullable<DashboardWidget['alert']> = {
  metric: { kind: 'column', column: '' },
  op: '>',
  threshold: 0,
  cooldownSec: 300,
  channels: ['desktop'],
};

function emptyDraft(): DashboardWidget {
  return {
    id: crypto.randomUUID(),
    title: '',
    workflowId: '',
    viewMode: 'chart',
    chartConfig: { ...DEFAULT_CHART_CONFIG, yAxes: ['v'] },
    layout: { x: 0, y: 0, w: 6, h: 4 },
    refresh: { ...DEFAULT_REFRESH },
    enabled: true,
  };
}

export function WidgetEditorDrawer({
  open,
  widget,
  isNew,
  hiddenSql: hiddenSqlProp,
  onClose,
  onSave,
}: Readonly<WidgetEditorDrawerProps>) {
  const { t } = useI18n();
  const connections = useConnectionStore((s) => s.connections);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);

  const [draft, setDraft] = useState<DashboardWidget>(() => widget ?? emptyDraft());
  const [hiddenSql, setHiddenSql] = useState<{ configId: string; sql: string }>(
    hiddenSqlProp ?? { configId: '', sql: 'SELECT 1 AS v' },
  );

  useEffect(() => {
    if (open) {
      setDraft(widget ?? emptyDraft());
      setHiddenSql(hiddenSqlProp ?? { configId: '', sql: 'SELECT 1 AS v' });
      void fetchConnections();
    }
  }, [open, widget, hiddenSqlProp, fetchConnections]);

  const handleSave = useCallback(() => {
    if (!draft.title.trim()) return;
    if (hiddenSqlProp != null && (!hiddenSql.configId || !hiddenSql.sql.trim())) return;
    const normalized = {
      ...draft,
      title: draft.title.trim(),
      refresh: normalizeRefreshPolicy(draft.refresh),
    };
    onSave(normalized, hiddenSqlProp != null ? hiddenSql : undefined);
  }, [draft, hiddenSql, hiddenSqlProp, onSave]);

  if (!open) return null;

  const connOptions = connections.map((c) => ({ value: c.id, label: c.name }));
  const refreshWarn = shouldWarnRefreshSec(draft.refresh);

  return (
    <div
      className="fixed inset-0 z-50 flex justify-end"
      data-no-drag
      data-testid="widget-editor-drawer"
    >
      <button
        type="button"
        className="absolute inset-0 bg-black/40"
        aria-label={t('common.close')}
        onClick={onClose}
      />
      <aside className="relative flex h-full w-full max-w-md flex-col border-l border-edge bg-surface shadow-xl">
        <div className="flex shrink-0 items-center justify-between border-b border-edge px-4 py-3">
          <h2 className="text-sm font-semibold text-fg">
            {isNew ? t('dashboard.newWidget') : t('dashboard.editWidget')}
          </h2>
          <Button variant="ghost" className="h-7 w-7 px-0" onClick={onClose}>
            <X className="h-4 w-4" />
          </Button>
        </div>

        <div className="flex-1 space-y-4 overflow-auto p-4">
          <label className="block space-y-1">
            <span className="text-xs text-fg-muted">{t('dashboard.widgetTitle')}</span>
            <Input
              value={draft.title}
              onChange={(e) => setDraft((d) => ({ ...d, title: e.target.value }))}
              placeholder={t('dashboard.widgetTitlePlaceholder')}
            />
          </label>

          {hiddenSqlProp != null ? (
            <>
              <label className="block space-y-1">
                <span className="text-xs text-fg-muted">{t('dashboard.connection')}</span>
                <Select
                  value={hiddenSql.configId}
                  onChange={(v) => setHiddenSql((s) => ({ ...s, configId: v }))}
                  options={connOptions}
                  placeholder={t('dashboard.selectConnection')}
                />
              </label>
              <label className="block space-y-1">
                <span className="text-xs text-fg-muted">{t('dashboard.sql')}</span>
                <textarea
                  value={hiddenSql.sql}
                  onChange={(e) => setHiddenSql((s) => ({ ...s, sql: e.target.value }))}
                  rows={6}
                  className="w-full resize-y rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg focus:outline-none focus:ring-1 focus:ring-accent"
                  spellCheck={false}
                />
              </label>
            </>
          ) : (
            <label className="block space-y-1">
              <span className="text-xs text-fg-muted">{t('dashboard.workflowSource')}</span>
              <Input value={draft.workflowId} readOnly className="font-mono text-xs" />
            </label>
          )}

          <label className="block space-y-1">
            <span className="text-xs text-fg-muted">{t('dashboard.refreshMode')}</span>
            <Select
              value={draft.refresh.mode}
              onChange={(v) =>
                setDraft((d) => ({
                  ...d,
                  refresh: {
                    mode: v as RefreshMode,
                    refreshSec:
                      v === 'interval'
                        ? clampRefreshSec(d.refresh.refreshSec ?? MIN_REFRESH_SEC)
                        : undefined,
                  },
                }))
              }
              options={REFRESH_MODES.map((m) => ({
                value: m,
                label: t(`dashboard.refreshMode.${m}`),
              }))}
            />
          </label>

          {draft.refresh.mode === 'interval' && (
            <label className="block space-y-1">
              <span className="text-xs text-fg-muted">{t('dashboard.refreshSec')}</span>
              <Input
                type="number"
                min={MIN_REFRESH_SEC}
                value={draft.refresh.refreshSec ?? MIN_REFRESH_SEC}
                onChange={(e) =>
                  setDraft((d) => ({
                    ...d,
                    refresh: {
                      ...d.refresh,
                      refreshSec: clampRefreshSec(Number(e.target.value) || MIN_REFRESH_SEC),
                    },
                  }))
                }
              />
              <span className="text-[11px] text-fg-muted">
                {t('dashboard.refreshSecHint', { min: MIN_REFRESH_SEC })}
              </span>
              {refreshWarn && (
                <p className="text-[11px] text-amber-500" data-testid="refresh-sec-warn">
                  {t('dashboard.refreshSecWarn', { sec: REFRESH_WARN_BELOW_SEC })}
                </p>
              )}
            </label>
          )}

          <label className="flex items-center gap-2 text-xs text-fg">
            <input
              type="checkbox"
              checked={draft.enabled}
              onChange={(e) => setDraft((d) => ({ ...d, enabled: e.target.checked }))}
              className="rounded border-edge"
            />
            {t('dashboard.widgetEnabled')}
          </label>

          <label className="block space-y-1">
            <span className="text-xs text-fg-muted">{t('dashboard.defaultView')}</span>
            <Select
              value={draft.viewMode}
              onChange={(v) =>
                setDraft((d) => ({ ...d, viewMode: v as DashboardWidget['viewMode'] }))
              }
              options={[
                { value: 'chart', label: t('dashboard.viewChart') },
                { value: 'table', label: t('dashboard.viewTable') },
              ]}
            />
          </label>

          {draft.viewMode === 'chart' && draft.chartConfig && (
            <>
              <label className="block space-y-1">
                <span className="text-xs text-fg-muted">{t('dashboard.chartType')}</span>
                <Select
                  value={draft.chartConfig.chartType}
                  onChange={(v) =>
                    setDraft((d) => ({
                      ...d,
                      chartConfig: {
                        ...(d.chartConfig ?? DEFAULT_CHART_CONFIG),
                        chartType: v as ChartType,
                      },
                    }))
                  }
                  options={CHART_TYPES.map((ct) => ({ value: ct, label: ct }))}
                />
              </label>

              <label className="block space-y-1">
                <span className="text-xs text-fg-muted">{t('dashboard.yAxis')}</span>
                <Input
                  value={draft.chartConfig.yAxes.join(', ')}
                  onChange={(e) =>
                    setDraft((d) => ({
                      ...d,
                      chartConfig: {
                        ...(d.chartConfig ?? DEFAULT_CHART_CONFIG),
                        yAxes: e.target.value
                          .split(',')
                          .map((s) => s.trim())
                          .filter(Boolean),
                      },
                    }))
                  }
                  placeholder="v, count"
                />
              </label>
            </>
          )}

          <div className="space-y-3 rounded-md border border-edge p-3">
            <label className="flex items-center gap-2 text-xs text-fg">
              <input
                type="checkbox"
                checked={!!draft.alert}
                onChange={(e) =>
                  setDraft((d) => ({
                    ...d,
                    alert: e.target.checked ? (d.alert ?? { ...DEFAULT_ALERT }) : undefined,
                  }))
                }
                className="rounded border-edge"
              />
              {t('dashboard.alertEnabled')}
            </label>

            {draft.alert && (
              <div className="space-y-3">
                <label className="block space-y-1">
                  <span className="text-xs text-fg-muted">{t('dashboard.alertColumn')}</span>
                  <Input
                    value={draft.alert.metric.column}
                    onChange={(e) =>
                      setDraft((d) => ({
                        ...d,
                        alert: {
                          ...d.alert!,
                          metric: { ...d.alert!.metric, column: e.target.value },
                        },
                      }))
                    }
                    placeholder="v"
                  />
                </label>

                <label className="block space-y-1">
                  <span className="text-xs text-fg-muted">{t('dashboard.alertOperator')}</span>
                  <Select
                    value={draft.alert.op}
                    onChange={(v) =>
                      setDraft((d) => ({
                        ...d,
                        alert: { ...d.alert!, op: v as AlertOperator },
                      }))
                    }
                    options={ALERT_OPS.map((op) => ({ value: op, label: op }))}
                  />
                </label>

                <label className="block space-y-1">
                  <span className="text-xs text-fg-muted">{t('dashboard.alertThreshold')}</span>
                  <Input
                    type="number"
                    value={draft.alert.threshold}
                    onChange={(e) =>
                      setDraft((d) => ({
                        ...d,
                        alert: { ...d.alert!, threshold: Number(e.target.value) },
                      }))
                    }
                  />
                </label>
              </div>
            )}
          </div>
        </div>

        <div className="flex shrink-0 justify-end gap-2 border-t border-edge px-4 py-3">
          <Button variant="ghost" className="h-8 px-3 text-xs" onClick={onClose}>
            {t('common.cancel')}
          </Button>
          <Button
            className="h-8 px-3 text-xs"
            onClick={handleSave}
            data-testid="widget-editor-save"
          >
            {t('common.save')}
          </Button>
        </div>
      </aside>
    </div>
  );
}
