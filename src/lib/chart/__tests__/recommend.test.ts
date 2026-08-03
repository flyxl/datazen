import { describe, it, expect } from 'vitest';
import { recommendChart } from '../recommend';
import type { ChartField } from '../../../types/chart';

function field(name: string, type: string, distinctCount = 20): ChartField {
  return {
    name,
    dataType: 'text',
    inferredType: type as ChartField['inferredType'],
    sampleValues: [],
    distinctCount,
  };
}

describe('recommendChart', () => {
  it('recommends bar for categorical + numeric with many distinct values', () => {
    const fields = [field('category', 'categorical', 15), field('count', 'numeric')];
    const rec = recommendChart(fields, 50);

    expect(rec).not.toBeNull();
    expect(rec!.chartType).toBe('bar');
    expect(rec!.xAxis).toBe('category');
    expect(rec!.yAxes).toContain('count');
  });

  it('recommends line for temporal + numeric', () => {
    const fields = [field('date', 'datetime'), field('value', 'numeric')];
    const rec = recommendChart(fields, 30);

    expect(rec).not.toBeNull();
    expect(rec!.chartType).toBe('line');
    expect(rec!.xAxis).toBe('date');
  });

  it('recommends pie for categorical + numeric with few distinct values and few rows', () => {
    const fields = [field('category', 'categorical', 5), field('count', 'numeric')];
    const rec = recommendChart(fields, 5);

    expect(rec).not.toBeNull();
    expect(rec!.chartType).toBe('pie');
  });

  it('recommends scatter for two numeric fields', () => {
    const fields = [field('x', 'numeric'), field('y', 'numeric')];
    const rec = recommendChart(fields, 100);

    expect(rec).not.toBeNull();
    expect(rec!.chartType).toBe('scatter');
  });

  it('returns null when no numeric fields', () => {
    const fields = [field('name', 'categorical'), field('desc', 'categorical')];
    const rec = recommendChart(fields, 10);

    expect(rec).toBeNull();
  });

  it('recommends area for single numeric field with enough rows', () => {
    const fields = [field('amount', 'numeric')];
    const rec = recommendChart(fields, 20);

    expect(rec).not.toBeNull();
    expect(rec!.chartType).toBe('area');
    expect(rec!.yAxes).toContain('amount');
  });

  it('datetime takes priority over categorical', () => {
    const fields = [
      field('date', 'datetime'),
      field('category', 'categorical'),
      field('value', 'numeric'),
    ];
    const rec = recommendChart(fields, 30);

    expect(rec!.chartType).toBe('line');
    expect(rec!.xAxis).toBe('date');
  });
});
