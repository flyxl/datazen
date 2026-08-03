import { CartesianGrid, Legend, Scatter, ScatterChart, Tooltip, XAxis, YAxis, ZAxis } from 'recharts';
import { getColorPalette } from '../../../lib/chart/colors';
import { formatNumber } from '../../../lib/chart/format';
import type { ChartConfig, ChartDataPoint } from '../../../types/chart';

interface ScatterChartRendererProps {
  data: ChartDataPoint[];
  config: ChartConfig;
  onDataPointClick?: (rowIndex: number) => void;
}

export function ScatterChartRenderer({ data, config, onDataPointClick }: ScatterChartRendererProps) {
  const colors = getColorPalette(config.colorScheme);
  const xKey = config.xAxis ?? '__index';
  const yKey = config.yAxes[0];

  return (
    <ScatterChart>
      <CartesianGrid
        strokeDasharray="3 3"
        stroke="var(--border-edge, #333)"
        opacity={config.showGrid ? 0.3 : 0}
      />
      <XAxis
        dataKey={xKey}
        type="number"
        name={xKey}
        tick={{ fontSize: 12, fill: 'var(--text-secondary, #999)' }}
        stroke="var(--border-edge, #333)"
        tickFormatter={(v: number) => formatNumber(v)}
      />
      <YAxis
        dataKey={yKey}
        type="number"
        name={yKey}
        tick={{ fontSize: 12, fill: 'var(--text-secondary, #999)' }}
        stroke="var(--border-edge, #333)"
        tickFormatter={(v: number) => formatNumber(v)}
      />
      <ZAxis range={[40, 40]} />
      <Tooltip
        contentStyle={{
          background: 'var(--bg-surface, #1e1e2e)',
          border: '1px solid var(--border-edge, #333)',
          borderRadius: 6,
          color: 'var(--text-primary, #eee)',
          fontSize: 12,
        }}
        cursor={{ strokeDasharray: '3 3' }}
      />
      {config.showLegend && (
        <Legend wrapperStyle={{ fontSize: 12, color: 'var(--text-primary, #eee)' }} />
      )}
      <Scatter
        data={data}
        fill={colors[0]}
        cursor={onDataPointClick ? 'pointer' : undefined}
        onClick={(_: unknown, index: number) => onDataPointClick?.(index)}
      />
    </ScatterChart>
  );
}
