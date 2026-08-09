import { useCallback, useEffect, useState } from 'react';
import { X } from 'lucide-react';
import { Button } from '../../components/ui/Button';
import { Input } from '../../components/ui/Input';
import { Select } from '../../components/ui/Select';
import { useI18n } from '../../hooks/useI18n';
import { useConnectionStore } from '../../stores/connectionStore';
import { clampRefreshSec, MIN_REFRESH_SEC } from '../../types/dashboard';
import type { AlertMetricAgg, AlertOperator, DashboardWidget } from '../../types/dashboard';
import type { ChartType } from '../../types/chart';
import { DEFAULT_CHART_CONFIG } from '../../types/chart';

export interface WidgetEditorDrawerProps {
  open: boolean;
  widget: DashboardWidget | null;
  isNew?: boolean;
  onClose: () => void;
  onSave: (widget: DashboardWidget) => void;
}

const CHART_TYPES: ChartType[] = ['bar', 'line', 'pie', 'scatter', 'area'];

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
    configId: '',
    sql: 'SELECT 1 AS v',
    chartConfig: {
      ...DEFAULT_CHART_CONFIG,
      yAxes: ['v'],
    },
    layout: { x: 0, y: 0, w: 6, h: 4 },
    refreshSec: 60,
    enabled: true,
  };
}

export function WidgetEditorDrawer({
  open,
  widget,
  isNew,
  onClose,
  onSave,
}: Readonly<WidgetEditorDrawerProps>) {
  const { t } = useI18n();
  const connections = useConnectionStore((s) => s.connections);
  const fetchConnections = useConnectionStore((s) => s.fetchConnections);

  const [draft, setDraft] = useState<DashboardWidget>(() => widget ?? emptyDraft());

  useEffect(() => {
    if (open) {
      setDraft(widget ?? emptyDraft());
      void fetchConnections();
    }
  }, [open, widget, fetchConnections]);

  const handleSave = useCallback(() => {
    if (!draft.title.trim() || !draft.configId || !draft.sql.trim()) return;
    onSave({
      ...draft,
      title: draft.title.trim(),
      refreshSec: clampRefreshSec(draft.refreshSec),
    });
  }, [draft, onSave]);

  if (!open) return null;

  const connOptions = connections.map((c) => ({ value: c.id, label: c.name }));

  return (
    <div className="fixed inset-0 z-50 flex justify-end" data-no-drag>
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

          <label className="block space-y-1">
            <span className="text-xs text-fg-muted">{t('dashboard.connection')}</span>
            <Select
              value={draft.configId}
              onChange={(v) => setDraft((d) => ({ ...d, configId: v }))}
              options={connOptions}
              placeholder={t('dashboard.selectConnection')}
            />
          </label>

          <label className="block space-y-1">
            <span className="text-xs text-fg-muted">{t('dashboard.sql')}</span>
            <textarea
              value={draft.sql}
              onChange={(e) => setDraft((d) => ({ ...d, sql: e.target.value }))}
              rows={6}
              className="w-full resize-y rounded-md border border-edge bg-surface px-3 py-2 font-mono text-xs text-fg focus:outline-none focus:ring-1 focus:ring-accent"
              spellCheck={false}
            />
          </label>

          <label className="block space-y-1">
            <span className="text-xs text-fg-muted">{t('dashboard.refreshSec')}</span>
            <Input
              type="number"
              min={MIN_REFRESH_SEC}
              value={draft.refreshSec}
              onChange={(e) =>
                setDraft((d) => ({ ...d, refreshSec: Number(e.target.value) || MIN_REFRESH_SEC }))
              }
            />
            <span className="text-[11px] text-fg-muted">{t('dashboard.refreshSecHint', { min: MIN_REFRESH_SEC })}</span>
          </label>

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
            <span className="text-xs text-fg-muted">{t('dashboard.chartType')}</span>
            <Select
              value={draft.chartConfig.chartType}
              onChange={(v) =>
                setDraft((d) => ({
                  ...d,
                  chartConfig: { ...d.chartConfig, chartType: v as ChartType },
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
                    ...d.chartConfig,
                    yAxes: e.target.value.split(',').map((s) => s.trim()).filter(Boolean),
                  },
                }))
              }
              placeholder="v, count"
            />
          </label>

          <label className="block space-y-1">
            <span className="text-xs text-fg-muted">{t('dashboard.xAxis')}</span>
            <Input
              value={draft.chartConfig.xAxis ?? ''}
              onChange={(e) =>
                setDraft((d) => ({
                  ...d,
                  chartConfig: { ...d.chartConfig, xAxis: e.target.value || null },
                }))
              }
              placeholder={t('dashboard.xAxisOptional')}
            />
          </label>

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
                  <span className="text-xs text-fg-muted">{t('dashboard.alertMetricKind')}</span>
                  <Select
                    value={draft.alert.metric.kind}
                    onChange={(v) =>
                      setDraft((d) => ({
                        ...d,
                        alert: {
                          ...d.alert!,
                          metric: {
                            ...d.alert!.metric,
                            kind: v as 'column' | 'aggregation',
                            agg: v === 'aggregation' ? (d.alert!.metric.agg ?? 'last') : undefined,
                          },
                        },
                      }))
                    }
                    options={[
                      { value: 'column', label: t('dashboard.alertKindColumn') },
                      { value: 'aggregation', label: t('dashboard.alertKindAggregation') },
                    ]}
                  />
                </label>

                {draft.alert.metric.kind === 'aggregation' && (
                  <label className="block space-y-1">
                    <span className="text-xs text-fg-muted">{t('dashboard.alertAggregation')}</span>
                    <Select
                      value={draft.alert.metric.agg ?? 'last'}
                      onChange={(v) =>
                        setDraft((d) => ({
                          ...d,
                          alert: {
                            ...d.alert!,
                            metric: { ...d.alert!.metric, agg: v as AlertMetricAgg },
                          },
                        }))
                      }
                      options={ALERT_AGGS.map((a) => ({ value: a, label: a }))}
                    />
                  </label>
                )}

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
                        alert: { ...d.alert!, threshold: Number(e.target.value) || 0 },
                      }))
                    }
                  />
                </label>

                <label className="block space-y-1">
                  <span className="text-xs text-fg-muted">{t('dashboard.alertCooldown')}</span>
                  <Input
                    type="number"
                    min={0}
                    value={draft.alert.cooldownSec}
                    onChange={(e) =>
                      setDraft((d) => ({
                        ...d,
                        alert: { ...d.alert!, cooldownSec: Number(e.target.value) || 0 },
                      }))
                    }
                  />
                </label>
              </div>
            )}
          </div>
        </div>

        <div className="flex shrink-0 justify-end gap-2 border-t border-edge px-4 py-3">
          <Button variant="ghost" onClick={onClose}>{t('common.cancel')}</Button>
          <Button
            onClick={handleSave}
            disabled={!draft.title.trim() || !draft.configId || !draft.sql.trim()}
          >
            {t('common.save')}
          </Button>
        </div>
      </aside>
    </div>
  );
}
