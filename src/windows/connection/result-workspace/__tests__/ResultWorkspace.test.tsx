import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { StatementResult } from '../../../../types';
import type { ChartConfig } from '../../../../types/chart';
import { DEFAULT_CHART_CONFIG } from '../../../../types/chart';
import { ResultWorkspace } from '../ResultWorkspace';
import type { ResultTableViewProps } from '../ResultTableView';

const mocks = vi.hoisted(() => ({
  table: vi.fn(),
  chart: vi.fn(),
}));

vi.mock('../../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string) => key,
  }),
}));

vi.mock('../ResultTableView', () => ({
  ResultTableView: (props: ResultTableViewProps) => {
    mocks.table(props);
    return (
      <div role="grid">
        <button type="button" onClick={() => props.onRowDetail?.(2)}>
          table row detail
        </button>
      </div>
    );
  },
}));

vi.mock('../../../../components/chart/ChartView', () => ({
  ChartView: (props: {
    result: StatementResult;
    savedConfig?: ChartConfig;
    onConfigChange?: (config: ChartConfig) => void;
    onDataPointClick?: (rowIndex: number) => void;
  }) => {
    mocks.chart(props);
    return (
      <div role="img" aria-label="mock chart">
        <button type="button" onClick={() => props.onDataPointClick?.(1)}>
          chart row detail
        </button>
        <button type="button" onClick={() => props.onConfigChange?.(DEFAULT_CHART_CONFIG)}>
          chart config change
        </button>
      </div>
    );
  },
}));

function result(overrides: Partial<StatementResult> = {}): StatementResult {
  return {
    sql: 'select label, amount from metrics',
    columns: [
      { name: 'label', dataType: 'text', nullable: true },
      { name: 'amount', dataType: 'int4', nullable: true },
    ],
    rows: [['one', 1]],
    executionTimeMs: 4,
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
});

describe('ResultWorkspace', () => {
  it('renders table first and changes the requested view without executing anything', () => {
    const onViewChange = vi.fn();
    const { rerender } = render(
      <ResultWorkspace
        result={result()}
        view="table"
        chartConfig={DEFAULT_CHART_CONFIG}
        onViewChange={onViewChange}
      />,
    );

    expect(screen.getByRole('grid')).toBeInTheDocument();
    fireEvent.click(screen.getByRole('button', { name: 'chart.viewChart' }));
    expect(onViewChange).toHaveBeenCalledWith('chart');

    rerender(
      <ResultWorkspace
        result={result()}
        view="chart"
        chartConfig={DEFAULT_CHART_CONFIG}
        onViewChange={onViewChange}
      />,
    );
    expect(screen.getByRole('img', { name: 'mock chart' })).toBeInTheDocument();
    expect(mocks.chart).toHaveBeenCalledWith(
      expect.objectContaining({ savedConfig: DEFAULT_CHART_CONFIG }),
    );
  });

  it('switches chart data point detail back to table before forwarding the row', () => {
    const onChartConfigChange = vi.fn();
    const onViewChange = vi.fn();
    const onRowDetail = vi.fn();
    const { rerender } = render(
      <ResultWorkspace
        result={result()}
        view="chart"
        chartConfig={DEFAULT_CHART_CONFIG}
        onViewChange={onViewChange}
        onChartConfigChange={onChartConfigChange}
        onRowDetail={onRowDetail}
      />,
    );

    fireEvent.click(screen.getByRole('button', { name: 'chart row detail' }));
    fireEvent.click(screen.getByRole('button', { name: 'chart config change' }));
    expect(onViewChange).toHaveBeenCalledWith('table');
    expect(onRowDetail).toHaveBeenCalledWith(1);
    expect(onViewChange.mock.invocationCallOrder[0]).toBeLessThan(
      onRowDetail.mock.invocationCallOrder[0],
    );
    expect(onChartConfigChange).toHaveBeenCalledWith(DEFAULT_CHART_CONFIG);

    rerender(
      <ResultWorkspace
        result={result()}
        view="table"
        chartConfig={DEFAULT_CHART_CONFIG}
        rowDetailIndex={1}
        onRowDetail={onRowDetail}
      />,
    );
    expect(mocks.table).toHaveBeenCalledWith(expect.objectContaining({ rowDetailIndex: 1 }));
    fireEvent.click(screen.getByRole('button', { name: 'table row detail' }));
    expect(onRowDetail).toHaveBeenCalledWith(2);
    expect(onViewChange).toHaveBeenCalledTimes(1);
  });

  it('allows a chartable result to render ChartView without a saved config', () => {
    const onViewChange = vi.fn();
    const onChartConfigChange = vi.fn();
    const { rerender } = render(
      <ResultWorkspace result={result()} view="table" onViewChange={onViewChange} />,
    );

    expect(screen.getByRole('grid')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'chart.viewChart' })).not.toBeDisabled();
    fireEvent.click(screen.getByRole('button', { name: 'chart.viewChart' }));
    expect(onViewChange).toHaveBeenCalledWith('chart');

    rerender(
      <ResultWorkspace result={result()} view="chart" onChartConfigChange={onChartConfigChange} />,
    );
    expect(screen.getByRole('img', { name: 'mock chart' })).toBeInTheDocument();
    expect(mocks.chart).toHaveBeenCalledWith(expect.objectContaining({ savedConfig: undefined }));
    fireEvent.click(screen.getByRole('button', { name: 'chart config change' }));
    expect(onChartConfigChange).toHaveBeenCalledWith(DEFAULT_CHART_CONFIG);
  });

  it('tolerates row detail events when callbacks are omitted', () => {
    const { rerender } = render(
      <ResultWorkspace result={result()} view="chart" chartConfig={DEFAULT_CHART_CONFIG} />,
    );

    expect(() =>
      fireEvent.click(screen.getByRole('button', { name: 'chart row detail' })),
    ).not.toThrow();

    rerender(<ResultWorkspace result={result()} view="table" />);
    expect(() =>
      fireEvent.click(screen.getByRole('button', { name: 'table row detail' })),
    ).not.toThrow();
  });

  it('falls back to table for empty and non-chartable results', () => {
    const { rerender } = render(
      <ResultWorkspace
        result={result({ rows: [] })}
        view="chart"
        chartConfig={DEFAULT_CHART_CONFIG}
      />,
    );
    expect(screen.getByRole('grid')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'chart.viewChart' })).toBeDisabled();

    rerender(
      <ResultWorkspace
        result={result({
          columns: [
            { name: 'label', dataType: 'text', nullable: true },
            { name: 'category', dataType: 'text', nullable: true },
          ],
          rows: [['one', 'a']],
        })}
        view="chart"
        chartConfig={DEFAULT_CHART_CONFIG}
      />,
    );
    expect(screen.getByRole('grid')).toBeInTheDocument();
    expect(mocks.chart).not.toHaveBeenCalled();
  });

  it('exposes empty and error UI contracts without rendering a view', () => {
    const { rerender } = render(<ResultWorkspace view="table" />);
    expect(screen.getByRole('status')).toHaveTextContent('sqlFile.noResults');
    expect(screen.queryByRole('button', { name: 'chart.viewTable' })).toBeNull();

    rerender(<ResultWorkspace view="chart" error="query failed" />);
    expect(screen.getByRole('alert')).toHaveTextContent('query failed');
    expect(screen.queryByRole('grid')).toBeNull();
    expect(screen.queryByRole('img', { name: 'mock chart' })).toBeNull();
  });
});
