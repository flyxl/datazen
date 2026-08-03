import type { StatementResult } from '../../types';
import type { AggregationType, ChartConfig, ChartDataPoint } from '../../types/chart';

export function transformData(
  result: StatementResult,
  config: ChartConfig,
): ChartDataPoint[] {
  const records = result.rows.map((row) =>
    Object.fromEntries(result.columns.map((col, i) => [col.name, row[i]])),
  );

  const raw = config.aggregation === 'none'
    ? transformDirect(records, config)
    : transformAggregated(records, config);

  return sortData(raw, config);
}

function transformDirect(
  records: Record<string, unknown>[],
  config: ChartConfig,
): ChartDataPoint[] {
  return records.map((rec, idx) => {
    const point: ChartDataPoint = {};
    if (config.xAxis) {
      const xVal = rec[config.xAxis];
      if (xVal == null && config.chartType === 'scatter') {
        point[config.xAxis] = idx;
      } else {
        point[config.xAxis] = formatAxisValue(xVal);
      }
    } else {
      point['__index'] = idx;
    }
    for (const y of config.yAxes) {
      point[y] = toNumber(rec[y]);
    }
    if (config.groupBy) {
      point[config.groupBy] = String(rec[config.groupBy] ?? '');
    }
    return point;
  });
}

function transformAggregated(
  records: Record<string, unknown>[],
  config: ChartConfig,
): ChartDataPoint[] {
  if (!config.xAxis) return [];

  const groups = new Map<string, Record<string, unknown>[]>();
  for (const rec of records) {
    const key = String(rec[config.xAxis] ?? '__null__');
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(rec);
  }

  return Array.from(groups.entries()).map(([key, rows]) => {
    const point: ChartDataPoint = { [config.xAxis!]: key };
    for (const y of config.yAxes) {
      point[y] = aggregate(rows.map((r) => r[y]), config.aggregation);
    }
    return point;
  });
}

function aggregate(values: unknown[], type: AggregationType): number | null {
  const nums = values.map(toNumber).filter((n): n is number => n !== null);
  if (nums.length === 0) return null;
  switch (type) {
    case 'sum': return nums.reduce((a, b) => a + b, 0);
    case 'avg': return nums.reduce((a, b) => a + b, 0) / nums.length;
    case 'count': return values.length;
    case 'min': return Math.min(...nums);
    case 'max': return Math.max(...nums);
    case 'distinct_count': return new Set(values).size;
    default: return null;
  }
}

function toNumber(v: unknown): number | null {
  if (typeof v === 'number') return v;
  if (typeof v === 'string') {
    const n = Number(v);
    return isNaN(n) ? null : n;
  }
  return null;
}

function formatAxisValue(v: unknown): string | number {
  if (typeof v === 'number') return v;
  return String(v ?? '');
}

function sortData(data: ChartDataPoint[], config: ChartConfig): ChartDataPoint[] {
  if (config.sortBy === 'none') return data;
  const xKey = config.xAxis ?? '__index';
  const yKey = config.yAxes[0];
  if (!yKey) return data;

  return [...data].sort((a, b) => {
    switch (config.sortBy) {
      case 'x_asc': return String(a[xKey] ?? '').localeCompare(String(b[xKey] ?? ''));
      case 'x_desc': return String(b[xKey] ?? '').localeCompare(String(a[xKey] ?? ''));
      case 'y_asc': return (toNumber(a[yKey]) ?? 0) - (toNumber(b[yKey]) ?? 0);
      case 'y_desc': return (toNumber(b[yKey]) ?? 0) - (toNumber(a[yKey]) ?? 0);
      default: return 0;
    }
  });
}
