import { CartesianGrid, LabelList, Legend, Line, LineChart, Tooltip, XAxis, YAxis } from 'recharts';
import { getColorPalette } from '../../../lib/chart/colors';
import { formatAxisTick, formatNumber } from '../../../lib/chart/format';
import type { ChartConfig, ChartDataPoint } from '../../../types/chart';

interface LineChartRendererProps {
  data: ChartDataPoint[];
  config: ChartConfig;
  onDataPointClick?: (rowIndex: number) => void;
}

export function LineChartRenderer({ data, config, onDataPointClick }: LineChartRendererProps) {
  const colors = getColorPalette(config.colorScheme);
  return (
    <LineChart data={data} onClick={onDataPointClick ? (state: unknown) => {
      const s = state as { activeTooltipIndex?: number | null };
      if (typeof s?.activeTooltipIndex === 'number') onDataPointClick(s.activeTooltipIndex);
    } : undefined}>
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
        <Line
          key={yKey}
          type="monotone"
          dataKey={yKey}
          stroke={colors[i % colors.length]}
          strokeWidth={2}
          dot={{ r: 3, fill: colors[i % colors.length] }}
          activeDot={{ r: 5, cursor: onDataPointClick ? 'pointer' : undefined }}
        >
          {config.showValues && <LabelList dataKey={yKey} position="top" fontSize={11} fill="var(--c-fg-secondary, #999)" />}
        </Line>
      ))}
    </LineChart>
  );
}
