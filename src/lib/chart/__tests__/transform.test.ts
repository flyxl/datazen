import { describe, it, expect } from 'vitest';
import { transformData, type TransformResult } from '../transform';
import type { StatementResult } from '../../../types';
import type { ChartConfig } from '../../../types/chart';
import { DEFAULT_CHART_CONFIG } from '../../../types/chart';

function makeResult(
  columns: { name: string; dataType: string }[],
  rows: unknown[][],
): StatementResult {
  return {
    sql: 'SELECT ...',
    columns: columns.map((c) => ({ ...c, nullable: true })),
    rows,
    executionTimeMs: 10,
  };
}

function cfg(overrides: Partial<ChartConfig>): ChartConfig {
  return { ...DEFAULT_CHART_CONFIG, ...overrides };
}

describe('transformData', () => {
  const salesResult = makeResult(
    [{ name: 'region', dataType: 'text' }, { name: 'sales', dataType: 'int4' }],
    [['East', 100], ['West', 200], ['East', 150], ['West', 50]],
  );

  describe('direct mode (aggregation=none)', () => {
    it('maps xAxis and yAxes correctly', () => {
      const config = cfg({ xAxis: 'region', yAxes: ['sales'], aggregation: 'none' });
      const { data, seriesKeys } = transformData(salesResult, config);

      expect(seriesKeys).toEqual(['sales']);
      expect(data).toHaveLength(4);
      expect(data[0]).toEqual({ region: 'East', sales: 100 });
      expect(data[1]).toEqual({ region: 'West', sales: 200 });
    });

    it('uses __index when xAxis is null', () => {
      const config = cfg({ xAxis: null, yAxes: ['sales'], aggregation: 'none' });
      const { data } = transformData(salesResult, config);

      expect(data[0]).toHaveProperty('__index', 0);
      expect(data[1]).toHaveProperty('__index', 1);
    });

    it('handles scatter chart with null x values', () => {
      const result = makeResult(
        [{ name: 'x', dataType: 'int4' }, { name: 'y', dataType: 'int4' }],
        [[null, 10], [5, 20]],
      );
      const config = cfg({ xAxis: 'x', yAxes: ['y'], chartType: 'scatter', aggregation: 'none' });
      const { data } = transformData(result, config);

      expect(data[0].x).toBe(0);
      expect(data[1].x).toBe(5);
    });
  });

  describe('aggregated mode', () => {
    it('sums values by xAxis', () => {
      const config = cfg({ xAxis: 'region', yAxes: ['sales'], aggregation: 'sum' });
      const { data, seriesKeys } = transformData(salesResult, config);

      expect(seriesKeys).toEqual(['sales']);
      expect(data).toHaveLength(2);
      const east = data.find((d) => d.region === 'East');
      const west = data.find((d) => d.region === 'West');
      expect(east?.sales).toBe(250);
      expect(west?.sales).toBe(250);
    });

    it('computes average', () => {
      const config = cfg({ xAxis: 'region', yAxes: ['sales'], aggregation: 'avg' });
      const { data } = transformData(salesResult, config);

      const east = data.find((d) => d.region === 'East');
      expect(east?.sales).toBe(125);
    });

    it('counts values', () => {
      const config = cfg({ xAxis: 'region', yAxes: ['sales'], aggregation: 'count' });
      const { data } = transformData(salesResult, config);

      const east = data.find((d) => d.region === 'East');
      expect(east?.sales).toBe(2);
    });

    it('returns empty when xAxis is null', () => {
      const config = cfg({ xAxis: null, yAxes: ['sales'], aggregation: 'sum' });
      const { data } = transformData(salesResult, config);
      expect(data).toEqual([]);
    });
  });

  describe('groupBy pivot', () => {
    const groupedResult = makeResult(
      [
        { name: 'month', dataType: 'text' },
        { name: 'region', dataType: 'text' },
        { name: 'sales', dataType: 'int4' },
      ],
      [
        ['Jan', 'East', 100],
        ['Jan', 'West', 200],
        ['Feb', 'East', 150],
        ['Feb', 'West', 50],
      ],
    );

    it('pivots data by group values (direct)', () => {
      const config = cfg({
        xAxis: 'month',
        yAxes: ['sales'],
        groupBy: 'region',
        aggregation: 'none',
      });
      const { data, seriesKeys } = transformData(groupedResult, config);

      expect(seriesKeys).toEqual(['East', 'West']);
      expect(data).toHaveLength(2);

      const jan = data.find((d) => d.month === 'Jan');
      expect(jan).toBeDefined();
      expect(jan?.East).toBe(100);
      expect(jan?.West).toBe(200);

      const feb = data.find((d) => d.month === 'Feb');
      expect(feb?.East).toBe(150);
      expect(feb?.West).toBe(50);
    });

    it('pivots data by group values (aggregated)', () => {
      const dupResult = makeResult(
        [
          { name: 'month', dataType: 'text' },
          { name: 'region', dataType: 'text' },
          { name: 'sales', dataType: 'int4' },
        ],
        [
          ['Jan', 'East', 100],
          ['Jan', 'East', 50],
          ['Jan', 'West', 200],
        ],
      );

      const config = cfg({
        xAxis: 'month',
        yAxes: ['sales'],
        groupBy: 'region',
        aggregation: 'sum',
      });
      const { data, seriesKeys } = transformData(dupResult, config);

      expect(seriesKeys).toEqual(['East', 'West']);
      const jan = data.find((d) => d.month === 'Jan');
      expect(jan?.East).toBe(150);
      expect(jan?.West).toBe(200);
    });

    it('handles groupBy same as xAxis', () => {
      const result = makeResult(
        [{ name: 'status', dataType: 'text' }, { name: 'count', dataType: 'int8' }],
        [['active', 100], ['inactive', 50]],
      );
      const config = cfg({
        xAxis: 'status',
        yAxes: ['count'],
        groupBy: 'status',
        aggregation: 'none',
      });
      const { data, seriesKeys } = transformData(result, config);

      expect(seriesKeys).toEqual(['active', 'inactive']);
      expect(data).toHaveLength(2);
      const active = data.find((d) => d.status === 'active');
      expect(active?.active).toBe(100);
    });

    it('creates compound series keys for multi yAxes', () => {
      const result = makeResult(
        [
          { name: 'month', dataType: 'text' },
          { name: 'region', dataType: 'text' },
          { name: 'revenue', dataType: 'int4' },
          { name: 'cost', dataType: 'int4' },
        ],
        [
          ['Jan', 'East', 100, 60],
          ['Jan', 'West', 200, 120],
        ],
      );
      const config = cfg({
        xAxis: 'month',
        yAxes: ['revenue', 'cost'],
        groupBy: 'region',
        aggregation: 'none',
      });
      const { data, seriesKeys } = transformData(result, config);

      expect(seriesKeys).toEqual(['East·revenue', 'East·cost', 'West·revenue', 'West·cost']);
      const jan = data[0];
      expect(jan['East·revenue']).toBe(100);
      expect(jan['East·cost']).toBe(60);
      expect(jan['West·revenue']).toBe(200);
      expect(jan['West·cost']).toBe(120);
    });
  });

  describe('sorting', () => {
    it('sorts by x ascending', () => {
      const result = makeResult(
        [{ name: 'name', dataType: 'text' }, { name: 'value', dataType: 'int4' }],
        [['Banana', 2], ['Apple', 3], ['Cherry', 1]],
      );
      const config = cfg({ xAxis: 'name', yAxes: ['value'], sortBy: 'x_asc', aggregation: 'none' });
      const { data } = transformData(result, config);

      expect(data.map((d) => d.name)).toEqual(['Apple', 'Banana', 'Cherry']);
    });

    it('sorts by y descending', () => {
      const result = makeResult(
        [{ name: 'name', dataType: 'text' }, { name: 'value', dataType: 'int4' }],
        [['A', 2], ['B', 5], ['C', 1]],
      );
      const config = cfg({ xAxis: 'name', yAxes: ['value'], sortBy: 'y_desc', aggregation: 'none' });
      const { data } = transformData(result, config);

      expect(data.map((d) => d.value)).toEqual([5, 2, 1]);
    });
  });
});
