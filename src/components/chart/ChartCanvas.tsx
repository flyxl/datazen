import { ResponsiveContainer } from 'recharts';
import { BarChartRenderer } from './renderers/BarChartRenderer';
import { LineChartRenderer } from './renderers/LineChartRenderer';
import { PieChartRenderer } from './renderers/PieChartRenderer';
import { ScatterChartRenderer } from './renderers/ScatterChartRenderer';
import { AreaChartRenderer } from './renderers/AreaChartRenderer';
import type { ChartConfig, ChartDataPoint, ChartType } from '../../types/chart';

interface ChartCanvasProps {
  data: ChartDataPoint[];
  config: ChartConfig;
  onDataPointClick?: (rowIndex: number) => void;
}

const RENDERERS: Record<ChartType, React.ComponentType<{ data: ChartDataPoint[]; config: ChartConfig; onDataPointClick?: (rowIndex: number) => void }>> = {
  bar: BarChartRenderer,
  line: LineChartRenderer,
  pie: PieChartRenderer,
  scatter: ScatterChartRenderer,
  area: AreaChartRenderer,
};

export function ChartCanvas({ data, config, onDataPointClick }: ChartCanvasProps) {
  const Renderer = RENDERERS[config.chartType];

  return (
    <div className="absolute inset-0 p-4">
      <ResponsiveContainer width="100%" height="100%">
        <Renderer data={data} config={config} onDataPointClick={onDataPointClick} />
      </ResponsiveContainer>
    </div>
  );
}
