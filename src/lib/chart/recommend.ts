import type { ChartField, ChartRecommendation } from '../../types/chart';

export function recommendChart(fields: ChartField[], rowCount: number): ChartRecommendation | null {
  const numerics = fields.filter((f) => f.inferredType === 'numeric');
  const datetimes = fields.filter((f) => f.inferredType === 'datetime');
  const categoricals = fields.filter((f) => f.inferredType === 'categorical');

  if (datetimes.length >= 1 && numerics.length >= 1) {
    return {
      chartType: 'line',
      xAxis: datetimes[0].name,
      yAxes: numerics.slice(0, 3).map((f) => f.name),
      groupBy: null,
      aggregation: 'none',
      confidence: 0.9,
      reason: 'chart.recommend.timeSeries',
    };
  }

  if (categoricals.length >= 1 && numerics.length >= 1) {
    const cat = categoricals[0];
    const distinctRatio = (cat.distinctCount ?? rowCount) / rowCount;

    if ((cat.distinctCount ?? 0) <= 8 && rowCount <= 20) {
      return {
        chartType: 'pie',
        xAxis: cat.name,
        yAxes: [numerics[0].name],
        groupBy: null,
        aggregation: 'none',
        confidence: 0.8,
        reason: 'chart.recommend.distribution',
      };
    }

    return {
      chartType: 'bar',
      xAxis: cat.name,
      yAxes: numerics.slice(0, 3).map((f) => f.name),
      groupBy: categoricals.length > 1 ? categoricals[1].name : null,
      aggregation: distinctRatio < 0.5 ? 'sum' : 'none',
      confidence: 0.85,
      reason: 'chart.recommend.comparison',
    };
  }

  if (numerics.length >= 2 && categoricals.length === 0 && datetimes.length === 0) {
    return {
      chartType: 'scatter',
      xAxis: numerics[0].name,
      yAxes: [numerics[1].name],
      groupBy: null,
      aggregation: 'none',
      confidence: 0.7,
      reason: 'chart.recommend.correlation',
    };
  }

  if (numerics.length === 1 && rowCount > 5) {
    return {
      chartType: 'area',
      xAxis: null,
      yAxes: [numerics[0].name],
      groupBy: null,
      aggregation: 'none',
      confidence: 0.5,
      reason: 'chart.recommend.trend',
    };
  }

  return null;
}
