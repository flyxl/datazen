import { STROKE_DASH_PATTERNS } from './colors';
import type { ChartDataPoint } from '../../types/chart';

/**
 * Threshold: two series are "overlapping" when their max absolute difference
 * across all data points is less than this fraction of the overall Y range.
 * 5% means the lines would be nearly indistinguishable in the chart.
 */
const OVERLAP_RATIO = 0.05;

/**
 * Given chart data and series keys, returns a map of seriesKey → strokeDasharray.
 * Only series that overlap closely with another series get a dash pattern;
 * all other series remain solid (undefined).
 */
export function buildOverlapDashMap(
  data: ChartDataPoint[],
  yAxes: string[],
): Record<string, string | undefined> {
  const result: Record<string, string | undefined> = {};
  for (const k of yAxes) result[k] = undefined;

  if (yAxes.length < 2 || data.length === 0) return result;

  let globalMin = Infinity;
  let globalMax = -Infinity;
  for (const pt of data) {
    for (const key of yAxes) {
      const v = Number(pt[key]);
      if (!Number.isFinite(v)) continue;
      if (v < globalMin) globalMin = v;
      if (v > globalMax) globalMax = v;
    }
  }
  const range = globalMax - globalMin;
  if (range === 0) {
    // All values identical — apply dash patterns so lines are distinguishable
    let dashIdx = 0;
    for (const key of yAxes) {
      result[key] = STROKE_DASH_PATTERNS[dashIdx % STROKE_DASH_PATTERNS.length];
      dashIdx++;
    }
    return result;
  }

  const threshold = range * OVERLAP_RATIO;
  const overlapping = new Set<string>();

  for (let i = 0; i < yAxes.length; i++) {
    for (let j = i + 1; j < yAxes.length; j++) {
      if (areSeriesTooClose(data, yAxes[i], yAxes[j], threshold)) {
        overlapping.add(yAxes[i]);
        overlapping.add(yAxes[j]);
      }
    }
  }

  if (overlapping.size === 0) return result;

  // Assign dash patterns only to overlapping series (skip the first one → solid)
  let dashIdx = 0;
  for (const key of yAxes) {
    if (overlapping.has(key)) {
      result[key] = STROKE_DASH_PATTERNS[dashIdx % STROKE_DASH_PATTERNS.length];
      dashIdx++;
    }
  }
  return result;
}

function areSeriesTooClose(
  data: ChartDataPoint[],
  keyA: string,
  keyB: string,
  threshold: number,
): boolean {
  let maxDiff = 0;
  for (const pt of data) {
    const a = Number(pt[keyA]);
    const b = Number(pt[keyB]);
    if (!Number.isFinite(a) || !Number.isFinite(b)) continue;
    const diff = Math.abs(a - b);
    if (diff > threshold) return false;
    if (diff > maxDiff) maxDiff = diff;
  }
  return true;
}
