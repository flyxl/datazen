import { useCallback, useMemo, useState } from 'react';
import { Area, AreaChart, CartesianGrid, Legend, Tooltip, XAxis, YAxis } from 'recharts';
import { getColorPalette, STROKE_DASH_PATTERNS } from '../../../lib/chart/colors';
import { formatAxisTick, formatCompact } from '../../../lib/chart/format';
import { computeLogScaleHint, mapToLogScale } from '../../../lib/chart/transform';
import type { ChartConfig, ChartDataPoint } from '../../../types/chart';
import { LogScaleTooltip } from '../LogScaleTooltip';

interface AreaChartRendererProps {
  data: ChartDataPoint[];
  config: ChartConfig;
  onDataPointClick?: (rowIndex: number) => void;
}

const logTickFormatter = (v: number) => formatCompact(Math.pow(10, v));
const labelFormatter = (value: unknown) =>
  typeof value === 'number' ? logTickFormatter(value) : String(value ?? '');

export function AreaChartRenderer({ data, config, onDataPointClick }: AreaChartRendererProps) {
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
    <AreaChart
      data={chartData}
      onClick={
        onDataPointClick
          ? (state: unknown) => {
              const s = state as { activeTooltipIndex?: number | null };
              if (typeof s?.activeTooltipIndex === 'number') onDataPointClick(s.activeTooltipIndex);
            }
          : undefined
      }
    >
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
          onClick={(entry) => {
            const dataKey = entry.dataKey;
            if (typeof dataKey === 'string') toggleSeries(dataKey);
          }}
          formatter={(value, entry) => {
            const dataKey = entry?.dataKey;
            return (
              <span style={{ opacity: typeof dataKey === 'string' && hiddenSeries.has(dataKey) ? 0.35 : 1 }}>
                {value}
              </span>
            );
          }}
        />
      )}
      {config.yAxes.map((yKey, i) => (
        <Area
          key={yKey}
          type="monotone"
          dataKey={yKey}
          stroke={colors[i % colors.length]}
          fill={colors[i % colors.length]}
          fillOpacity={0.15}
          strokeWidth={2}
          strokeDasharray={
            multiSeries ? STROKE_DASH_PATTERNS[i % STROKE_DASH_PATTERNS.length] : undefined
          }
          hide={hiddenSeries.has(yKey)}
          activeDot={{ r: 5, cursor: onDataPointClick ? 'pointer' : undefined }}
        />
      ))}
    </AreaChart>
  );
}
