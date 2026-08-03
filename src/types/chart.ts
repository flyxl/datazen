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
