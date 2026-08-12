import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen } from '@testing-library/react';
import { ChartWidgetTile } from '../ChartWidgetTile';
import type { DashboardWidget, WidgetRun } from '../../../types/dashboard';
import { DEFAULT_CHART_CONFIG } from '../../../types/chart';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../components/chart/ChartCanvas', () => ({
  ChartCanvas: () => <div data-testid="mock-chart-canvas" />,
}));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: () => <div data-testid="mock-data-table" />,
}));

const widget: DashboardWidget = {
  id: 'w1',
  title: 'Sales Chart',
  workflowId: 'wf-1',
  viewMode: 'chart',
  chartConfig: { ...DEFAULT_CHART_CONFIG, xAxis: 'category', yAxes: ['amount'] },
  layout: { x: 0, y: 0, w: 6, h: 4 },
  refresh: { mode: 'manual' },
  enabled: true,
};

const okRun: WidgetRun = {
  id: 'run-1',
  dashboardId: 'dash-1',
  widgetId: 'w1',
  workflowId: 'wf-1',
  startedAt: '2026-01-01T00:00:00Z',
  finishedAt: '2026-01-01T00:00:01Z',
  status: 'ok',
  rowCount: 2,
  columns: ['category', 'amount'],
  rows: [
    ['Alpha', 100],
    ['Beta', 200],
  ],
};

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe('ChartWidgetTile', () => {
  it('renders tile with title and chart view toggle', () => {
    render(
      <ChartWidgetTile
        widget={widget}
        run={okRun}
        onEdit={vi.fn()}
        onHistory={vi.fn()}
        onRefresh={vi.fn()}
        onViewModeChange={vi.fn()}
      />,
    );

    expect(screen.getByTestId('dashboard-tile')).toBeInTheDocument();
    expect(screen.getByText('Sales Chart')).toBeInTheDocument();
    expect(screen.getByTestId('dashboard-view-toggle')).toBeInTheDocument();
    expect(screen.getByTestId('mock-chart-canvas')).toBeInTheDocument();
  });

  it('switches to table view when table toggle is clicked', () => {
    const onViewModeChange = vi.fn();
    render(
      <ChartWidgetTile
        widget={{ ...widget, viewMode: 'chart' }}
        run={okRun}
        onEdit={vi.fn()}
        onHistory={vi.fn()}
        onRefresh={vi.fn()}
        onViewModeChange={onViewModeChange}
      />,
    );

    const toggle = screen.getByTestId('dashboard-view-toggle');
    const buttons = toggle.querySelectorAll('button');
    fireEvent.click(buttons[1]!);
    expect(onViewModeChange).toHaveBeenCalledWith('table');
  });

  it('renders table when viewMode is table', () => {
    render(
      <ChartWidgetTile
        widget={{ ...widget, viewMode: 'table' }}
        run={okRun}
        onEdit={vi.fn()}
        onHistory={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );

    expect(screen.getByTestId('mock-data-table')).toBeInTheDocument();
  });

  it('fires edit, history, and refresh callbacks', () => {
    const onEdit = vi.fn();
    const onHistory = vi.fn();
    const onRefresh = vi.fn();

    render(
      <ChartWidgetTile
        widget={widget}
        run={okRun}
        onEdit={onEdit}
        onHistory={onHistory}
        onRefresh={onRefresh}
      />,
    );

    fireEvent.click(screen.getByTestId('dashboard-tile-edit'));
    fireEvent.click(screen.getByTestId('dashboard-tile-history'));
    fireEvent.click(screen.getByTestId('dashboard-tile-refresh'));

    expect(onEdit).toHaveBeenCalledTimes(1);
    expect(onHistory).toHaveBeenCalledTimes(1);
    expect(onRefresh).toHaveBeenCalledTimes(1);
  });

  it('shows error state when run failed', () => {
    render(
      <ChartWidgetTile
        widget={widget}
        run={{ ...okRun, status: 'error', error: 'Query failed' }}
        onEdit={vi.fn()}
        onHistory={vi.fn()}
        onRefresh={vi.fn()}
      />,
    );

    expect(screen.getByText('Query failed')).toBeInTheDocument();
  });
});
