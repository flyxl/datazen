import { useCallback, useMemo, useState } from 'react';
import { Bar, BarChart, CartesianGrid, LabelList, Legend, Tooltip, XAxis, YAxis } from 'recharts';
import { getColorPalette } from '../../../lib/chart/colors';
import { formatAxisTick, formatCompact } from '../../../lib/chart/format';
import { computeLogScaleHint, mapToLogScale } from '../../../lib/chart/transform';
import type { ChartConfig, ChartDataPoint } from '../../../types/chart';
import { LogScaleTooltip } from '../LogScaleTooltip';

interface BarChartRendererProps {
  data: ChartDataPoint[];
  config: ChartConfig;
  onDataPointClick?: (rowIndex: number) => void;
}

const logTickFormatter = (v: number) => formatCompact(Math.pow(10, v));

export function BarChartRenderer({ data, config, onDataPointClick }: BarChartRendererProps) {
  const colors = getColorPalette(config.colorScheme);
  const logHint = useMemo(() => computeLogScaleHint(data, config.yAxes), [data, config.yAxes]);
  const chartData = useMemo(
    () => (logHint.use ? mapToLogScale(data, config.yAxes, logHint.domainMin) : data),
    [data, config.yAxes, logHint],
  );

  const [hiddenSeries, setHiddenSeries] = useState<Set<string>>(new Set());
  const toggleSeries = useCallback((dataKey: string) => {
    setHiddenSeries((prev) => {
      const next = new Set(prev);
      if (next.has(dataKey)) next.delete(dataKey);
      else next.add(dataKey);
      return next;
    });
  }, []);

  const multiSeries = config.yAxes.length > 1;

  return (
    <BarChart data={chartData}>
      <CartesianGrid
        strokeDasharray="3 3"
        stroke="var(--c-edge, #333)"
        opacity={config.showGrid ? 0.3 : 0}
      />
      <XAxis
        dataKey={config.xAxis ?? '__index'}
        tick={{ fontSize: 12, fill: 'var(--c-fg-secondary, #999)' }}
        stroke="var(--c-edge, #333)"
        tickFormatter={formatAxisTick}
      />
      <YAxis
        tick={{ fontSize: 12, fill: 'var(--c-fg-secondary, #999)' }}
        stroke="var(--c-edge, #333)"
        tickFormatter={logHint.use ? logTickFormatter : (v: number) => formatCompact(v)}
      />
      {logHint.use ? (
        <LogScaleTooltip />
      ) : (
        <Tooltip
          contentStyle={{
            background: 'var(--c-surface-alt, #1e1e2e)',
            border: '1px solid var(--c-edge, #333)',
            borderRadius: 6,
            color: 'var(--c-fg, #eee)',
            fontSize: 12,
          }}
        />
      )}
      {multiSeries && (
        <Legend
          wrapperStyle={{ fontSize: 12, color: 'var(--c-fg, #eee)', cursor: 'pointer' }}
          onClick={(data) => {
            const key = data.dataKey;
            if (typeof key === 'string' && key) toggleSeries(key);
          }}
          formatter={(value, entry) => (
            <span style={{ opacity: hiddenSeries.has(String(entry.dataKey ?? '')) ? 0.35 : 1 }}>
              {value}
            </span>
          )}
        />
      )}
      {config.yAxes.map((yKey, i) => (
        <Bar
          key={yKey}
          dataKey={yKey}
          fill={colors[i % colors.length]}
          radius={[3, 3, 0, 0]}
          hide={hiddenSeries.has(yKey)}
          isAnimationActive={false}
          cursor={onDataPointClick ? 'pointer' : undefined}
          onClick={(_: unknown, index: number) => onDataPointClick?.(index)}
        >
          {config.showValues && (
            <LabelList
              dataKey={yKey}
              position="top"
              fontSize={11}
              fill="var(--c-fg-secondary, #999)"
              formatter={logHint.use ? (v) => logTickFormatter(Number(v)) : undefined}
            />
          )}
        </Bar>
      ))}
    </BarChart>
  );
}
