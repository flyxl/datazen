import type { AggregationType, ChartConfig, ChartField, ChartType } from '../../types/chart';

const CHART_TYPE_PATTERNS: { pattern: RegExp; type: ChartType }[] = [
  { pattern: /(?:换成|改成|切换(?:为|到)?|switch\s*(?:to)?|change\s*(?:to)?)\s*(?:柱状图|柱形图|bar)/i, type: 'bar' },
  { pattern: /(?:换成|改成|切换(?:为|到)?|switch\s*(?:to)?|change\s*(?:to)?)\s*(?:折线图|线图|line)/i, type: 'line' },
  { pattern: /(?:换成|改成|切换(?:为|到)?|switch\s*(?:to)?|change\s*(?:to)?)\s*(?:饼图|饼状图|pie)/i, type: 'pie' },
  { pattern: /(?:换成|改成|切换(?:为|到)?|switch\s*(?:to)?|change\s*(?:to)?)\s*(?:散点图|scatter)/i, type: 'scatter' },
  { pattern: /(?:换成|改成|切换(?:为|到)?|switch\s*(?:to)?|change\s*(?:to)?)\s*(?:面积图|area)/i, type: 'area' },
  { pattern: /^(?:柱状图|柱形图|bar\s*chart?)$/i, type: 'bar' },
  { pattern: /^(?:折线图|线图|line\s*chart?)$/i, type: 'line' },
  { pattern: /^(?:饼图|饼状图|pie\s*chart?)$/i, type: 'pie' },
  { pattern: /^(?:散点图|scatter\s*(?:plot|chart)?)$/i, type: 'scatter' },
  { pattern: /^(?:面积图|area\s*chart?)$/i, type: 'area' },
];

const AGG_PATTERNS: { pattern: RegExp; agg: AggregationType }[] = [
  { pattern: /(?:按|用|聚合方式?(?:改)?为?)\s*(?:求和|sum)/i, agg: 'sum' },
  { pattern: /(?:按|用|聚合方式?(?:改)?为?)\s*(?:平均|均值|avg|average)/i, agg: 'avg' },
  { pattern: /(?:按|用|聚合方式?(?:改)?为?)\s*(?:计数|count)/i, agg: 'count' },
  { pattern: /(?:按|用|聚合方式?(?:改)?为?)\s*(?:最[大小]值?|min|max)/i, agg: 'min' },
];

const SORT_PATTERNS: { pattern: RegExp; sort: ChartConfig['sortBy'] }[] = [
  { pattern: /(?:按|sort\s*by)\s*(?:x\s*轴?|横轴)\s*(?:升序|asc)/i, sort: 'x_asc' },
  { pattern: /(?:按|sort\s*by)\s*(?:x\s*轴?|横轴)\s*(?:降序|desc)/i, sort: 'x_desc' },
  { pattern: /(?:按|sort\s*by)\s*(?:y\s*轴?|纵轴|数值)\s*(?:升序|asc)/i, sort: 'y_asc' },
  { pattern: /(?:按|sort\s*by)\s*(?:y\s*轴?|纵轴|数值)\s*(?:降序|desc)/i, sort: 'y_desc' },
  { pattern: /(?:从[小大]到[大小]|升序|ascending)/i, sort: 'y_asc' },
  { pattern: /(?:从大到小|降序|descending)/i, sort: 'y_desc' },
];

const TOGGLE_PATTERNS: { pattern: RegExp; key: 'showLegend' | 'showGrid' | 'showValues'; value: boolean }[] = [
  { pattern: /(?:显示|打开|show)\s*(?:图例|legend)/i, key: 'showLegend', value: true },
  { pattern: /(?:隐藏|关闭|hide)\s*(?:图例|legend)/i, key: 'showLegend', value: false },
  { pattern: /(?:显示|打开|show)\s*(?:网格|grid)/i, key: 'showGrid', value: true },
  { pattern: /(?:隐藏|关闭|hide)\s*(?:网格|grid)/i, key: 'showGrid', value: false },
  { pattern: /(?:显示|打开|show)\s*(?:数值|values?)/i, key: 'showValues', value: true },
  { pattern: /(?:隐藏|关闭|hide)\s*(?:数值|values?)/i, key: 'showValues', value: false },
];

export interface NlConfigResult {
  config: Partial<ChartConfig>;
  matched: boolean;
  description: string;
}

export function parseNlChartConfig(
  input: string,
  _fields: ChartField[],
  currentConfig: ChartConfig,
): NlConfigResult {
  const trimmed = input.trim();
  if (!trimmed) return { config: {}, matched: false, description: '' };

  const changes: Partial<ChartConfig> = {};
  const descriptions: string[] = [];

  for (const { pattern, type } of CHART_TYPE_PATTERNS) {
    if (pattern.test(trimmed)) {
      changes.chartType = type;
      descriptions.push(`→ ${type}`);
      break;
    }
  }

  for (const { pattern, agg } of AGG_PATTERNS) {
    if (pattern.test(trimmed)) {
      changes.aggregation = agg;
      descriptions.push(`aggregation: ${agg}`);
      break;
    }
  }

  for (const { pattern, sort } of SORT_PATTERNS) {
    if (pattern.test(trimmed)) {
      changes.sortBy = sort;
      descriptions.push(`sort: ${sort}`);
      break;
    }
  }

  for (const { pattern, key, value } of TOGGLE_PATTERNS) {
    if (pattern.test(trimmed)) {
      changes[key] = value;
      descriptions.push(`${key}: ${value}`);
    }
  }

  const fieldMatch = trimmed.match(/(?:x\s*轴?|横轴)\s*(?:改为?|设为?|=|：|:)\s*(\S+)/i);
  if (fieldMatch) {
    changes.xAxis = fieldMatch[1];
    descriptions.push(`xAxis: ${fieldMatch[1]}`);
  }

  const yFieldMatch = trimmed.match(/(?:y\s*轴?|纵轴)\s*(?:改为?|设为?|=|：|:)\s*(\S+)/i);
  if (yFieldMatch) {
    changes.yAxes = [yFieldMatch[1]];
    descriptions.push(`yAxes: [${yFieldMatch[1]}]`);
  }

  const matched = descriptions.length > 0;
  return {
    config: matched ? { ...currentConfig, ...changes } : {},
    matched,
    description: descriptions.join(', '),
  };
}
