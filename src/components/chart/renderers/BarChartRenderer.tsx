import { Bar, BarChart, CartesianGrid, LabelList, Legend, Tooltip, XAxis, YAxis } from 'recharts';
import { getColorPalette } from '../../../lib/chart/colors';
import { formatAxisTick, formatNumber } from '../../../lib/chart/format';
import type { ChartConfig, ChartDataPoint } from '../../../types/chart';

interface BarChartRendererProps {
  data: ChartDataPoint[];
  config: ChartConfig;
  onDataPointClick?: (rowIndex: number) => void;
}

export function BarChartRenderer({ data, config, onDataPointClick }: BarChartRendererProps) {
  const colors = getColorPalette(config.colorScheme);
  return (
    <BarChart data={data}>
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
        tickFormatter={(v: number) => formatNumber(v)}
      />
      <Tooltip
        contentStyle={{
          background: 'var(--c-surface-alt, #1e1e2e)',
          border: '1px solid var(--c-edge, #333)',
          borderRadius: 6,
          color: 'var(--c-fg, #eee)',
          fontSize: 12,
        }}
      />
      {config.showLegend && config.yAxes.length > 1 && (
        <Legend wrapperStyle={{ fontSize: 12, color: 'var(--c-fg, #eee)' }} />
      )}
      {config.yAxes.map((yKey, i) => (
        <Bar
          key={yKey}
          dataKey={yKey}
          fill={colors[i % colors.length]}
          radius={[3, 3, 0, 0]}
          cursor={onDataPointClick ? 'pointer' : undefined}
          onClick={(_: unknown, index: number) => onDataPointClick?.(index)}
        >
          {config.showValues && <LabelList dataKey={yKey} position="top" fontSize={11} fill="var(--c-fg-secondary, #999)" />}
        </Bar>
      ))}
    </BarChart>
  );
}
