import { describe, it, expect } from 'vitest';
import { parseNlChartConfig } from '../nlConfig';
import type { ChartConfig } from '../../../types/chart';
import { DEFAULT_CHART_CONFIG } from '../../../types/chart';

const base: ChartConfig = {
  ...DEFAULT_CHART_CONFIG,
  chartType: 'bar',
  xAxis: 'name',
  yAxes: ['value'],
};

describe('parseNlChartConfig', () => {
  describe('chart type switching', () => {
    it('switches to pie chart (Chinese)', () => {
      const { matched, config } = parseNlChartConfig('换成饼图', [], base);
      expect(matched).toBe(true);
      expect(config.chartType).toBe('pie');
    });

    it('switches to line chart (Chinese)', () => {
      const { matched, config } = parseNlChartConfig('切换为折线图', [], base);
      expect(matched).toBe(true);
      expect(config.chartType).toBe('line');
    });

    it('switches to scatter chart (English)', () => {
      const { matched, config } = parseNlChartConfig('switch to scatter', [], base);
      expect(matched).toBe(true);
      expect(config.chartType).toBe('scatter');
    });

    it('handles standalone chart type name', () => {
      const { matched, config } = parseNlChartConfig('面积图', [], base);
      expect(matched).toBe(true);
      expect(config.chartType).toBe('area');
    });
  });

  describe('aggregation', () => {
    it('sets aggregation to sum', () => {
      const { matched, config } = parseNlChartConfig('按求和', [], base);
      expect(matched).toBe(true);
      expect(config.aggregation).toBe('sum');
    });

    it('sets aggregation to average', () => {
      const { matched, config } = parseNlChartConfig('按平均', [], base);
      expect(matched).toBe(true);
      expect(config.aggregation).toBe('avg');
    });

    it('sets aggregation to count', () => {
      const { matched, config } = parseNlChartConfig('用计数', [], base);
      expect(matched).toBe(true);
      expect(config.aggregation).toBe('count');
    });
  });

  describe('sorting', () => {
    it('sorts descending', () => {
      const { matched, config } = parseNlChartConfig('降序', [], base);
      expect(matched).toBe(true);
      expect(config.sortBy).toBe('y_desc');
    });

    it('sorts ascending', () => {
      const { matched, config } = parseNlChartConfig('升序', [], base);
      expect(matched).toBe(true);
      expect(config.sortBy).toBe('y_asc');
    });

    it('sorts by x axis descending', () => {
      const { matched, config } = parseNlChartConfig('按x轴降序', [], base);
      expect(matched).toBe(true);
      expect(config.sortBy).toBe('x_desc');
    });
  });

  describe('toggles', () => {
    it('shows legend', () => {
      const { matched, config } = parseNlChartConfig('显示图例', [], base);
      expect(matched).toBe(true);
      expect(config.showLegend).toBe(true);
    });

    it('hides grid', () => {
      const { matched, config } = parseNlChartConfig('隐藏网格', [], base);
      expect(matched).toBe(true);
      expect(config.showGrid).toBe(false);
    });

    it('shows values (English)', () => {
      const { matched, config } = parseNlChartConfig('show values', [], base);
      expect(matched).toBe(true);
      expect(config.showValues).toBe(true);
    });
  });

  describe('axis field changes', () => {
    it('sets x axis field', () => {
      const { matched, config } = parseNlChartConfig('x轴改为date', [], base);
      expect(matched).toBe(true);
      expect(config.xAxis).toBe('date');
    });

    it('sets y axis field', () => {
      const { matched, config } = parseNlChartConfig('y轴设为amount', [], base);
      expect(matched).toBe(true);
      expect(config.yAxes).toEqual(['amount']);
    });
  });

  describe('combined commands', () => {
    it('handles chart type + aggregation', () => {
      const { matched, config } = parseNlChartConfig('换成折线图 按求和', [], base);
      expect(matched).toBe(true);
      expect(config.chartType).toBe('line');
      expect(config.aggregation).toBe('sum');
    });
  });

  describe('no match', () => {
    it('returns matched=false for unrecognized input', () => {
      const { matched, config } = parseNlChartConfig('hello world', [], base);
      expect(matched).toBe(false);
      expect(config).toEqual({});
    });

    it('returns matched=false for empty input', () => {
      const { matched } = parseNlChartConfig('', [], base);
      expect(matched).toBe(false);
    });
  });
});
