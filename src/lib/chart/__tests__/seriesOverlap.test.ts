import { describe, expect, it } from 'vitest';
import { buildOverlapDashMap } from '../seriesOverlap';
import { STROKE_DASH_PATTERNS } from '../colors';
import type { ChartDataPoint } from '../../../types/chart';

describe('buildOverlapDashMap', () => {
  it('returns empty map for single series', () => {
    const data: ChartDataPoint[] = [
      { x: 1, a: 10 },
      { x: 2, a: 20 },
    ];
    const result = buildOverlapDashMap(data, ['a']);
    expect(result).toEqual({ a: undefined });
  });

  it('returns all solid when series are far apart', () => {
    const data: ChartDataPoint[] = [
      { x: 1, a: 10, b: 100 },
      { x: 2, a: 20, b: 200 },
      { x: 3, a: 30, b: 300 },
    ];
    const result = buildOverlapDashMap(data, ['a', 'b']);
    expect(result.a).toBeUndefined();
    expect(result.b).toBeUndefined();
  });

  it('applies dash patterns when series overlap', () => {
    const data: ChartDataPoint[] = [
      { x: 1, a: 100, b: 101 },
      { x: 2, a: 200, b: 201 },
      { x: 3, a: 300, b: 301 },
    ];
    const result = buildOverlapDashMap(data, ['a', 'b']);
    expect(result.a).toBe(STROKE_DASH_PATTERNS[0]);
    expect(result.b).toBe(STROKE_DASH_PATTERNS[1]);
  });

  it('applies dash patterns when all values are identical', () => {
    const data: ChartDataPoint[] = [
      { x: 1, a: 50, b: 50 },
      { x: 2, a: 50, b: 50 },
    ];
    const result = buildOverlapDashMap(data, ['a', 'b']);
    expect(result.a).toBe(STROKE_DASH_PATTERNS[0]);
    expect(result.b).toBe(STROKE_DASH_PATTERNS[1]);
  });

  it('only dashes overlapping pairs when three series exist', () => {
    const data: ChartDataPoint[] = [
      { x: 1, a: 100, b: 101, c: 500 },
      { x: 2, a: 200, b: 201, c: 600 },
    ];
    const result = buildOverlapDashMap(data, ['a', 'b', 'c']);
    // a gets STROKE_DASH_PATTERNS[0] (undefined = solid), b gets [1] (dashed)
    expect(result.a).toBe(STROKE_DASH_PATTERNS[0]);
    expect(result.b).toBe(STROKE_DASH_PATTERNS[1]);
    expect(result.c).toBeUndefined();
  });

  it('returns all undefined for empty data', () => {
    const result = buildOverlapDashMap([], ['a', 'b']);
    expect(result.a).toBeUndefined();
    expect(result.b).toBeUndefined();
  });

  it('handles non-finite values gracefully', () => {
    // NaN points are skipped; only finite points are compared.
    // Here a=100,200 and b=NaN,201 → only x=2 compares (200 vs 201).
    // Global range is 100..201 = 101, threshold = 5.05, diff = 1 < 5.05 → overlapping.
    const data: ChartDataPoint[] = [
      { x: 1, a: 100, b: NaN },
      { x: 2, a: 200, b: 201 },
    ];
    const result = buildOverlapDashMap(data, ['a', 'b']);
    expect(result.a).toBe(STROKE_DASH_PATTERNS[0]);
    expect(result.b).toBe(STROKE_DASH_PATTERNS[1]);
  });
});
