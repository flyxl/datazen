import type { StatementResult } from '../../types';
import type { AggregationType, ChartConfig, ChartDataPoint } from '../../types/chart';

export interface TransformResult {
  data: ChartDataPoint[];
  seriesKeys: string[];
}

export function transformData(result: StatementResult, config: ChartConfig): TransformResult {
  const records = result.rows.map((row) =>
    Object.fromEntries(result.columns.map((col, i) => [col.name, row[i]])),
  );

  const flat =
    config.aggregation === 'none'
      ? transformDirect(records, config)
      : transformAggregated(records, config);

  if (config.groupBy) {
    return pivotByGroup(flat, config);
  }

  return { data: sortData(flat, config), seriesKeys: config.yAxes };
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
      point['__group__'] = String(rec[config.groupBy] ?? '');
    }
    return point;
  });
}

function transformAggregated(
  records: Record<string, unknown>[],
  config: ChartConfig,
): ChartDataPoint[] {
  if (!config.xAxis) return [];

  if (config.groupBy) {
    const compositeGroups = new Map<string, Record<string, unknown>[]>();
    for (const rec of records) {
      const xVal = String(rec[config.xAxis] ?? '__null__');
      const gVal = String(rec[config.groupBy] ?? '__null__');
      const key = `${xVal}\0${gVal}`;
      if (!compositeGroups.has(key)) compositeGroups.set(key, []);
      compositeGroups.get(key)!.push(rec);
    }

    return Array.from(compositeGroups.entries()).map(([key, rows]) => {
      const [xVal, gVal] = key.split('\0');
      const point: ChartDataPoint = {
        [config.xAxis!]: xVal,
        __group__: gVal,
      };
      for (const y of config.yAxes) {
        point[y] = aggregate(
          rows.map((r) => r[y]),
          config.aggregation,
        );
      }
      return point;
    });
  }

  const groups = new Map<string, Record<string, unknown>[]>();
  for (const rec of records) {
    const key = String(rec[config.xAxis] ?? '__null__');
    if (!groups.has(key)) groups.set(key, []);
    groups.get(key)!.push(rec);
  }

  return Array.from(groups.entries()).map(([key, rows]) => {
    const point: ChartDataPoint = { [config.xAxis!]: key };
    for (const y of config.yAxes) {
      point[y] = aggregate(
        rows.map((r) => r[y]),
        config.aggregation,
      );
    }
    return point;
  });
}

function pivotByGroup(flat: ChartDataPoint[], config: ChartConfig): TransformResult {
  const xKey = config.xAxis ?? '__index';

  const groupValuesSet = new Set<string>();
  for (const point of flat) {
    groupValuesSet.add(String(point['__group__'] ?? ''));
  }
  const groupValues = [...groupValuesSet];

  const pivotMap = new Map<string, ChartDataPoint>();
  const xOrder: string[] = [];
  for (const point of flat) {
    const xVal = String(point[xKey] ?? '');
    if (!pivotMap.has(xVal)) {
      pivotMap.set(xVal, { [xKey]: point[xKey] });
      xOrder.push(xVal);
    }
    const pivotPoint = pivotMap.get(xVal)!;
    const gVal = String(point['__group__'] ?? '');
    for (const y of config.yAxes) {
      const seriesKey = config.yAxes.length > 1 ? `${gVal}·${y}` : gVal;
      pivotPoint[seriesKey] = point[y];
    }
  }

  const seriesKeys: string[] = [];
  for (const gVal of groupValues) {
    for (const y of config.yAxes) {
      seriesKeys.push(config.yAxes.length > 1 ? `${gVal}·${y}` : gVal);
    }
  }

  const data = xOrder.map((x) => pivotMap.get(x)!);
  return { data: sortData(data, config), seriesKeys };
}

function aggregate(values: unknown[], type: AggregationType): number | null {
  const nums = values.map(toNumber).filter((n): n is number => n !== null);
  if (nums.length === 0) return null;
  switch (type) {
    case 'sum':
      return nums.reduce((a, b) => a + b, 0);
    case 'avg':
      return nums.reduce((a, b) => a + b, 0) / nums.length;
    case 'count':
      return values.length;
    case 'min':
      return Math.min(...nums);
    case 'max':
      return Math.max(...nums);
    case 'distinct_count':
      return new Set(values).size;
    default:
      return null;
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

export interface LogScaleHint {
  use: boolean;
  domainMin: number;
}

/**
 * Check whether Y-axis series have value ranges differing by > 10x AND all
 * values are non-negative, making a log scale appropriate.  Returns a domain
 * minimum (smallest positive value, floored to 1) for the log scale.
 */
export function computeLogScaleHint(data: ChartDataPoint[], seriesKeys: string[]): LogScaleHint {
  if (seriesKeys.length < 2) return { use: false, domainMin: 1 };

  let globalMin = Infinity;
  let globalMax = -Infinity;
  let minPositive = Infinity;
  let hasNegative = false;

  for (const key of seriesKeys) {
    for (const point of data) {
      const v = toNumber(point[key]);
      if (v == null) continue;
      if (v < 0) hasNegative = true;
      if (v < globalMin) globalMin = v;
      if (v > globalMax) globalMax = v;
      if (v > 0 && v < minPositive) minPositive = v;
    }
  }

  if (hasNegative || globalMax <= 0) return { use: false, domainMin: 1 };
  if (globalMax / Math.max(globalMin, minPositive, 1) < 10) return { use: false, domainMin: 1 };

  const domainMin =
    minPositive === Infinity ? 1 : Math.max(1, Math.pow(10, Math.floor(Math.log10(minPositive))));
  return { use: true, domainMin };
}

function sortData(data: ChartDataPoint[], config: ChartConfig): ChartDataPoint[] {
  if (config.sortBy === 'none') return data;
  const xKey = config.xAxis ?? '__index';
  const yKey = config.yAxes[0];
  if (!yKey) return data;

  return [...data].sort((a, b) => {
    switch (config.sortBy) {
      case 'x_asc':
        return String(a[xKey] ?? '').localeCompare(String(b[xKey] ?? ''));
      case 'x_desc':
        return String(b[xKey] ?? '').localeCompare(String(a[xKey] ?? ''));
      case 'y_asc':
        return (toNumber(a[yKey]) ?? 0) - (toNumber(b[yKey]) ?? 0);
      case 'y_desc':
        return (toNumber(b[yKey]) ?? 0) - (toNumber(a[yKey]) ?? 0);
      default:
        return 0;
    }
  });
}
