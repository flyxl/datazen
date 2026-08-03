import { useCallback, useMemo, useRef, useState } from 'react';
import { inferAllFields } from '../../lib/chart/fieldInference';
import { recommendChart } from '../../lib/chart/recommend';
import { transformData } from '../../lib/chart/transform';
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
  const fields = useMemo(() => inferAllFields(result), [result]);
  const recommendation = useMemo(() => recommendChart(fields, result.rows.length), [fields, result.rows.length]);

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

  const data = useMemo(() => transformData(chartResult, config), [chartResult, config]);

  if (result.rows.length === 0) {
    return <ChartEmptyState reason="noData" />;
  }

  if (fields.filter((f) => f.inferredType === 'numeric').length === 0) {
    return <ChartEmptyState reason="noNumericField" />;
  }

  const showEmptyCanvas = config.yAxes.length === 0;

  return (
    <div className="flex flex-1 flex-col min-h-0">
      <ChartToolbar config={config} onChange={setConfig} chartRef={chartRef} fields={fields} />
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
            <ChartCanvas data={data} config={config} onDataPointClick={onDataPointClick} />
          )}
        </div>
      </div>
    </div>
  );
}
