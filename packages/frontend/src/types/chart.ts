export type ChartType = 'bar' | 'line' | 'pie' | 'scatter' | 'area';

export type AggregationType = 'none' | 'sum' | 'avg' | 'count' | 'min' | 'max' | 'distinct_count';

export type InferredFieldType = 'numeric' | 'datetime' | 'categorical' | 'boolean' | 'unknown';

export interface ChartField {
  name: string;
  dataType: string;
  inferredType: InferredFieldType;
  distinctCount?: number;
  sampleValues?: unknown[];
}

export interface ChartConfig {
  chartType: ChartType;
  xAxis: string | null;
  yAxes: string[];
  groupBy: string | null;
  aggregation: AggregationType;
  sortBy: 'x_asc' | 'x_desc' | 'y_asc' | 'y_desc' | 'none';
  showLegend: boolean;
  showGrid: boolean;
  showValues: boolean;
  colorScheme: string;
  /**
   * 可选：把 X 轴当作「数值时间轴」渲染（`HH:mm:ss`）。
   * - `timeDomain`：固定显示的数据范围 `[startMs, endMs]`（滚动窗口两端）。
   * - `timeTicks`：显式刻度（与 timeDomain 同单位，ms）。
   * 设置后图表采用 `type="number" scale="time" domain`，且不绘制数据点、关闭动画。
   */
  timeDomain?: [number, number];
  timeTicks?: number[];
}

export interface ChartRecommendation {
  chartType: ChartType;
  xAxis: string | null;
  yAxes: string[];
  groupBy: string | null;
  aggregation: AggregationType;
  confidence: number;
  reason: string;
}

export interface ChartDataPoint {
  [key: string]: string | number | null;
}

export const DEFAULT_CHART_CONFIG: ChartConfig = {
  chartType: 'bar',
  xAxis: null,
  yAxes: [],
  groupBy: null,
  aggregation: 'none',
  sortBy: 'none',
  showLegend: true,
  showGrid: true,
  showValues: false,
  colorScheme: 'default',
};
