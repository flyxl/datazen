import { useCallback, useMemo, useState } from 'react';
import { CartesianGrid, LabelList, Legend, Line, LineChart, Tooltip, XAxis, YAxis } from 'recharts';
import { getColorPalette, STROKE_DASH_PATTERNS } from '../../../lib/chart/colors';
import { formatAxisTick, formatCompact } from '../../../lib/chart/format';
import { computeLogScaleHint, mapToLogScale } from '../../../lib/chart/transform';
import type { ChartConfig, ChartDataPoint } from '../../../types/chart';
import { LogScaleTooltip } from '../LogScaleTooltip';

interface LineChartRendererProps {
  data: ChartDataPoint[];
  config: ChartConfig;
  onDataPointClick?: (rowIndex: number) => void;
}

const logTickFormatter = (v: number) => formatCompact(Math.pow(10, v));

export function LineChartRenderer({ data, config, onDataPointClick }: LineChartRendererProps) {
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
    <LineChart
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
          onClick={(e: { dataKey?: string }) => e.dataKey && toggleSeries(e.dataKey)}
          formatter={(value: string, entry: { dataKey?: string }) => (
            <span style={{ opacity: hiddenSeries.has(entry.dataKey ?? '') ? 0.35 : 1 }}>
              {value}
            </span>
          )}
        />
      )}
      {config.yAxes.map((yKey, i) => (
        <Line
          key={yKey}
          type="monotone"
          dataKey={yKey}
          stroke={colors[i % colors.length]}
          strokeWidth={2}
          strokeDasharray={
            multiSeries ? STROKE_DASH_PATTERNS[i % STROKE_DASH_PATTERNS.length] : undefined
          }
          hide={hiddenSeries.has(yKey)}
          dot={{ r: 3, fill: colors[i % colors.length] }}
          activeDot={{ r: 5, cursor: onDataPointClick ? 'pointer' : undefined }}
        >
          {config.showValues && (
            <LabelList
              dataKey={yKey}
              position="top"
              fontSize={11}
              fill="var(--c-fg-secondary, #999)"
              formatter={logHint.use ? logTickFormatter : undefined}
            />
          )}
        </Line>
      ))}
    </LineChart>
  );
}
