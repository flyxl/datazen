import { ChevronDown, ChevronRight, Lightbulb, Plus, X } from 'lucide-react';
import { useState } from 'react';
import { useI18n } from '../../hooks/useI18n';
import { Select } from '../ui/Select';
import type { SelectOption } from '../ui/Select';
import type { AggregationType, ChartConfig, ChartField, ChartRecommendation } from '../../types/chart';
import { COLOR_PALETTES } from '../../lib/chart/colors';

interface AxisConfiguratorProps {
  fields: ChartField[];
  config: ChartConfig;
  onChange: (config: ChartConfig) => void;
  recommendation: ChartRecommendation | null;
}

export function AxisConfigurator({ fields, config, onChange, recommendation }: AxisConfiguratorProps) {
  const { t } = useI18n();
  const [collapsed, setCollapsed] = useState(false);

  const numericFields = fields.filter((f) => f.inferredType === 'numeric');
  const categoricalFields = fields.filter((f) => f.inferredType === 'categorical' || f.inferredType === 'datetime');
  const unusedYFields = numericFields.filter((f) => !config.yAxes.includes(f.name));

  const allFieldOptions: SelectOption[] = [
    { value: '', label: t('chart.autoIndex') },
    ...fields.map((f) => ({ value: f.name, label: `${f.name} (${f.inferredType})` })),
  ];

  const numericFieldOptions: SelectOption[] = numericFields.map((f) => ({
    value: f.name,
    label: f.name,
  }));

  const groupByOptions: SelectOption[] = [
    { value: '', label: t('chart.noGroupBy') },
    ...categoricalFields.map((f) => ({ value: f.name, label: f.name })),
  ];

  const aggregationOptions: SelectOption[] = [
    { value: 'none', label: t('chart.agg.none') },
    { value: 'sum', label: t('chart.agg.sum') },
    { value: 'avg', label: t('chart.agg.avg') },
    { value: 'count', label: t('chart.agg.count') },
    { value: 'min', label: t('chart.agg.min') },
    { value: 'max', label: t('chart.agg.max') },
  ];

  const sortOptions: SelectOption[] = [
    { value: 'none', label: t('chart.sort.none') },
    { value: 'x_asc', label: t('chart.sort.xAsc') },
    { value: 'x_desc', label: t('chart.sort.xDesc') },
    { value: 'y_asc', label: t('chart.sort.yAsc') },
    { value: 'y_desc', label: t('chart.sort.yDesc') },
  ];

  if (collapsed) {
    return (
      <button
        type="button"
        className="shrink-0 border-r border-edge bg-surface-alt px-1 py-2 text-fg-muted hover:text-fg-secondary"
        onClick={() => setCollapsed(false)}
        title={t('chart.configPanel')}
      >
        <ChevronRight className="h-4 w-4" />
      </button>
    );
  }

  return (
    <div className="flex w-52 shrink-0 flex-col border-r border-edge bg-surface-alt overflow-y-auto">
      {/* Header */}
      <div className="flex items-center justify-between border-b border-edge px-3 py-1.5">
        <span className="text-xs font-medium text-fg-secondary">{t('chart.configPanel')}</span>
        <button
          type="button"
          className="text-fg-muted hover:text-fg-secondary"
          onClick={() => setCollapsed(true)}
        >
          <ChevronDown className="h-3.5 w-3.5 rotate-90" />
        </button>
      </div>

      <div className="flex flex-col gap-3 p-3 text-xs">
        {/* X Axis */}
        <div>
          <label className="mb-1 block text-fg-muted">{t('chart.xAxis')}</label>
          <Select
            value={config.xAxis ?? ''}
            options={allFieldOptions}
            onChange={(v) => onChange({ ...config, xAxis: v || null })}
            className="!h-7 !text-xs"
          />
        </div>

        {/* Y Axes */}
        <div>
          <label className="mb-1 block text-fg-muted">{t('chart.yAxis')}</label>
          {config.yAxes.map((yKey, idx) => (
            <div key={yKey} className="mb-1 flex items-center gap-1">
              <Select
                value={yKey}
                options={numericFieldOptions}
                onChange={(v) => {
                  const next = [...config.yAxes];
                  next[idx] = v;
                  onChange({ ...config, yAxes: next });
                }}
                className="!h-7 !text-xs flex-1"
              />
              {config.yAxes.length > 1 && (
                <button
                  type="button"
                  className="text-fg-muted hover:text-red-400"
                  onClick={() => onChange({ ...config, yAxes: config.yAxes.filter((_, i) => i !== idx) })}
                >
                  <X className="h-3 w-3" />
                </button>
              )}
            </div>
          ))}
          {unusedYFields.length > 0 && config.chartType !== 'pie' && config.chartType !== 'scatter' && (
            <button
              type="button"
              className="flex items-center gap-1 text-accent hover:text-accent/80 mt-1"
              onClick={() => onChange({ ...config, yAxes: [...config.yAxes, unusedYFields[0].name] })}
            >
              <Plus className="h-3 w-3" />
              {t('chart.addSeries')}
            </button>
          )}
        </div>

        {/* Group By */}
        {config.chartType !== 'pie' && config.chartType !== 'scatter' && categoricalFields.length > 0 && (
          <div>
            <label className="mb-1 block text-fg-muted">{t('chart.groupBy')}</label>
            <Select
              value={config.groupBy ?? ''}
              options={groupByOptions}
              onChange={(v) => onChange({ ...config, groupBy: v || null })}
              className="!h-7 !text-xs"
            />
          </div>
        )}

        {/* Aggregation */}
        <div>
          <label className="mb-1 block text-fg-muted">{t('chart.aggregation')}</label>
          <Select
            value={config.aggregation}
            options={aggregationOptions}
            onChange={(v) => onChange({ ...config, aggregation: v as AggregationType })}
            className="!h-7 !text-xs"
          />
        </div>

        {/* Sort */}
        <div>
          <label className="mb-1 block text-fg-muted">{t('chart.sortBy')}</label>
          <Select
            value={config.sortBy}
            options={sortOptions}
            onChange={(v) => onChange({ ...config, sortBy: v as ChartConfig['sortBy'] })}
            className="!h-7 !text-xs"
          />
        </div>

        {/* Color Scheme */}
        <div>
          <label className="mb-1 block text-fg-muted">{t('chart.colorScheme')}</label>
          <div className="flex gap-1.5">
            {Object.entries(COLOR_PALETTES).map(([name, palette]) => (
              <button
                key={name}
                type="button"
                title={name}
                aria-label={name}
                className={`flex gap-0.5 rounded p-1 ${config.colorScheme === name ? 'ring-1 ring-accent' : 'hover:bg-surface'}`}
                onClick={() => onChange({ ...config, colorScheme: name })}
              >
                {palette.slice(0, 4).map((c, i) => (
                  <span key={i} className="block h-3 w-3 rounded-sm" style={{ backgroundColor: c }} />
                ))}
              </button>
            ))}
          </div>
        </div>

        {/* Recommendation hint */}
        {recommendation && (
          <div className="rounded border border-accent/20 bg-accent/5 p-2">
            <div className="flex items-center gap-1 text-accent mb-1">
              <Lightbulb className="h-3 w-3" />
              <span className="font-medium">{t('chart.recommended')}</span>
            </div>
            <p className="text-fg-muted text-[11px]">
              {t(recommendation.reason as never)}
            </p>
          </div>
        )}
      </div>
    </div>
  );
}
