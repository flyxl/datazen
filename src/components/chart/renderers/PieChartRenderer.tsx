import { Cell, Legend, Pie, PieChart, Tooltip } from 'recharts';
import { getColorPalette } from '../../../lib/chart/colors';
import type { ChartConfig, ChartDataPoint } from '../../../types/chart';

interface PieChartRendererProps {
  data: ChartDataPoint[];
  config: ChartConfig;
  onDataPointClick?: (rowIndex: number) => void;
}

export function PieChartRenderer({ data, config, onDataPointClick }: PieChartRendererProps) {
  const colors = getColorPalette(config.colorScheme);
  const yKey = config.yAxes[0];
  const nameKey = config.xAxis ?? '__index';

  return (
    <PieChart>
      <Pie
        data={data}
        dataKey={yKey}
        nameKey={nameKey}
        cx="50%"
        cy="50%"
        outerRadius="70%"
        isAnimationActive={false}
        label={
          config.showValues
            ? (props: { name?: string; percent?: number }) =>
                `${props.name ?? ''}: ${((props.percent ?? 0) * 100).toFixed(0)}%`
            : false
        }
        labelLine={config.showValues}
      >
        {data.map((_, idx) => (
          <Cell
            key={idx}
            fill={colors[idx % colors.length]}
            cursor={onDataPointClick ? 'pointer' : undefined}
            onClick={() => onDataPointClick?.(idx)}
          />
        ))}
      </Pie>
      <Tooltip
        contentStyle={{
          background: 'var(--c-surface-alt, #1e1e2e)',
          border: '1px solid var(--c-edge, #333)',
          borderRadius: 6,
          color: 'var(--c-fg, #eee)',
          fontSize: 12,
        }}
      />
      {config.showLegend && <Legend wrapperStyle={{ fontSize: 12, color: 'var(--c-fg, #eee)' }} />}
    </PieChart>
  );
}
