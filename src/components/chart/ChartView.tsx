import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Minimize2 } from 'lucide-react';
import { inferAllFields } from '../../lib/chart/fieldInference';
import { recommendChart } from '../../lib/chart/recommend';
import { transformData, type TransformResult } from '../../lib/chart/transform';
import { exportChartAsPng, exportChartAsSvg } from '../../lib/chart/export';
import { useI18n } from '../../hooks/useI18n';
import type { StatementResult } from '../../types';
import type { ChartConfig, ChartRecommendation } from '../../types/chart';
import { DEFAULT_CHART_CONFIG } from '../../types/chart';
import { ChartToolbar } from './ChartToolbar';
import { AxisConfigurator } from './AxisConfigurator';
import { ChartCanvas } from './ChartCanvas';
import { ChartEmptyState } from './ChartEmptyState';

interface ChartViewProps {
  result: StatementResult;
  onDataPointClick?: (rowIndex: number) => void;
  savedConfig?: ChartConfig;
  onConfigChange?: (config: ChartConfig) => void;
}

function recommendationToConfig(rec: ChartRecommendation): ChartConfig {
  return {
    ...DEFAULT_CHART_CONFIG,
    chartType: rec.chartType,
    xAxis: rec.xAxis,
    yAxes: rec.yAxes,
    groupBy: rec.groupBy,
    aggregation: rec.aggregation,
  };
}

function defaultConfig(fields: { name: string; inferredType: string }[]): ChartConfig {
  const numerics = fields.filter((f) => f.inferredType === 'numeric');
  return {
    ...DEFAULT_CHART_CONFIG,
    yAxes: numerics.length > 0 ? [numerics[0].name] : [],
  };
}

const MAX_CHART_ROWS = 1000;

export function ChartView({ result, onDataPointClick, savedConfig, onConfigChange }: ChartViewProps) {
  const chartRef = useRef<HTMLDivElement>(null);
  const expandedChartRef = useRef<HTMLDivElement>(null);
  const { t } = useI18n();
  const fields = useMemo(() => inferAllFields(result), [result]);
  const recommendation = useMemo(() => recommendChart(fields, result.rows.length), [fields, result.rows.length]);
  const [expanded, setExpanded] = useState(false);

  const [config, setConfigState] = useState<ChartConfig>(() =>
    savedConfig ?? (recommendation ? recommendationToConfig(recommendation) : defaultConfig(fields)),
  );

  const setConfig = useCallback((newConfig: ChartConfig) => {
    setConfigState(newConfig);
    onConfigChange?.(newConfig);
  }, [onConfigChange]);

  const chartResult = useMemo(() => {
    if (result.rows.length > MAX_CHART_ROWS) {
      return {
        ...result,
        rows: result.rows.slice(0, MAX_CHART_ROWS),
      };
    }
    return result;
  }, [result]);

  const { data, seriesKeys } = useMemo<TransformResult>(
    () => transformData(chartResult, config),
    [chartResult, config],
  );

  const renderConfig = useMemo<ChartConfig>(() => {
    if (!config.groupBy) return config;
    return { ...config, yAxes: seriesKeys, groupBy: null };
  }, [config, seriesKeys]);

  if (result.rows.length === 0) {
    return <ChartEmptyState reason="noData" />;
  }

  if (fields.filter((f) => f.inferredType === 'numeric').length === 0) {
    return <ChartEmptyState reason="noNumericField" />;
  }

  const showEmptyCanvas = config.yAxes.length === 0;

  return (
    <div className="flex flex-1 flex-col min-h-0">
      <ChartToolbar config={config} onChange={setConfig} chartRef={chartRef} fields={fields} onExpand={() => setExpanded(true)} />
      <div className="flex flex-1 min-h-0">
        <AxisConfigurator
          fields={fields}
          config={config}
          onChange={setConfig}
          recommendation={recommendation}
        />
        <div ref={chartRef} className="relative flex-1 min-h-0">
          {showEmptyCanvas ? (
            <ChartEmptyState reason="noConfig" />
          ) : (
            <ChartCanvas data={data} config={renderConfig} onDataPointClick={onDataPointClick} />
          )}
        </div>
      </div>

      {expanded && createPortal(
        <ChartExpandOverlay
          data={data}
          config={config}
          renderConfig={renderConfig}
          fields={fields}
          recommendation={recommendation}
          chartRef={expandedChartRef}
          t={t}
          onConfigChange={setConfig}
          onClose={() => setExpanded(false)}
        />,
        document.body,
      )}
    </div>
  );
}

function ChartExpandOverlay({
  data,
  config,
  renderConfig,
  fields,
  recommendation,
  chartRef,
  t,
  onConfigChange,
  onClose,
}: {
  data: ReturnType<typeof transformData>['data'];
  config: ChartConfig;
  renderConfig: ChartConfig;
  fields: ReturnType<typeof inferAllFields>;
  recommendation: ChartRecommendation | null;
  chartRef: React.RefObject<HTMLDivElement | null>;
  t: ReturnType<typeof useI18n>['t'];
  onConfigChange: (c: ChartConfig) => void;
  onClose: () => void;
}) {
  useEffect(() => {
    function onKeyDown(e: KeyboardEvent) {
      if (e.key === 'Escape') onClose();
    }
    window.addEventListener('keydown', onKeyDown);
    return () => window.removeEventListener('keydown', onKeyDown);
  });

  const handleExport = async (format: 'png' | 'svg') => {
    const el = chartRef.current;
    if (!el) return;
    const filename = `chart-${Date.now()}`;
    if (format === 'png') await exportChartAsPng(el, filename);
    else await exportChartAsSvg(el, filename);
  };

  return (
    <div className="fixed inset-0 z-50 flex flex-col bg-surface/95 backdrop-blur-sm">
      {/* Toolbar */}
      <div className="flex shrink-0 items-center gap-2 border-b border-edge px-4 py-2">
        <span className="text-sm font-medium text-fg">{t('chart.expandTitle')}</span>
        <div className="flex-1" />
        <button
          type="button"
          className="rounded px-2 py-1 text-xs text-fg-muted hover:text-fg-secondary hover:bg-surface-alt transition-colors"
          onClick={() => void handleExport('png')}
        >
          PNG
        </button>
        <button
          type="button"
          className="rounded px-2 py-1 text-xs text-fg-muted hover:text-fg-secondary hover:bg-surface-alt transition-colors"
          onClick={() => void handleExport('svg')}
        >
          SVG
        </button>
        <span className="mx-1 h-4 w-px bg-edge" />
        <button
          type="button"
          className="flex items-center gap-1 rounded px-2 py-1 text-xs text-fg-muted hover:text-fg-secondary hover:bg-surface-alt transition-colors"
          onClick={onClose}
          title={t('chart.collapse')}
        >
          <Minimize2 className="h-3.5 w-3.5" />
        </button>
      </div>

      {/* Content */}
      <div className="flex flex-1 min-h-0">
        <AxisConfigurator
          fields={fields}
          config={config}
          onChange={onConfigChange}
          recommendation={recommendation}
        />
        <div ref={chartRef as React.RefObject<HTMLDivElement>} className="relative flex-1 min-h-0">
          <ChartCanvas data={data} config={renderConfig} />
        </div>
      </div>
    </div>
  );
}
