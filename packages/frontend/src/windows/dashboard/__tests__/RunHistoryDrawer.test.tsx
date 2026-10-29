import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen, waitFor } from '@testing-library/react';
import { RunHistoryDrawer } from '../RunHistoryDrawer';
import type { DashboardWidget, WidgetRun } from '../../../types/dashboard';
import { DEFAULT_CHART_CONFIG } from '../../../types/chart';

const mockListWidgetRuns = vi.fn();
const mockGetWidgetRun = vi.fn();

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/dashboard', () => ({
  dashboardCommands: {
    listWidgetRuns: (...args: unknown[]) => mockListWidgetRuns(...args),
    getWidgetRun: (...args: unknown[]) => mockGetWidgetRun(...args),
  },
}));

vi.mock('../../../components/chart/ChartCanvas', () => ({
  ChartCanvas: () => <div data-testid="mock-chart-canvas" />,
}));

vi.mock('../../../components/DataTable/DataTable', () => ({
  DataTable: () => <div data-testid="mock-data-table" />,
}));

const widget: DashboardWidget = {
  id: 'w1',
  title: 'Sales',
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

describe('RunHistoryDrawer', () => {
  it('renders null when closed', () => {
    const { container } = render(
      <RunHistoryDrawer open={false} dashboardId="dash-1" widget={widget} onClose={vi.fn()} />,
    );
    expect(container.firstChild).toBeNull();
  });

  it('loads index and selects first entry on open', async () => {
    mockListWidgetRuns.mockResolvedValue([
      { id: 'run-1', startedAt: '2026-01-01T00:00:00Z', status: 'ok' },
      { id: 'run-2', startedAt: '2026-01-02T00:00:00Z', status: 'error' },
    ]);
    mockGetWidgetRun.mockResolvedValue(okRun);

    render(<RunHistoryDrawer open dashboardId="dash-1" widget={widget} onClose={vi.fn()} />);

    await waitFor(() => expect(mockListWidgetRuns).toHaveBeenCalledWith('dash-1', 'w1', 50));
    await waitFor(() => expect(mockGetWidgetRun).toHaveBeenCalledWith('dash-1', 'w1', 'run-1'));
    expect(screen.getByTestId('run-history-drawer')).toBeInTheDocument();
    // The chart renders only after the loading flag commits; poll so CI load
    // cannot observe the transient loader frame.
    await waitFor(() => expect(screen.getByTestId('mock-chart-canvas')).toBeInTheDocument());
  });

  it('loads a different run when an index entry is selected', async () => {
    mockListWidgetRuns.mockResolvedValue([
      { id: 'run-1', startedAt: '2026-01-01T00:00:00Z', status: 'ok' },
      { id: 'run-2', startedAt: '2026-01-02T00:00:00Z', status: 'error' },
    ]);
    mockGetWidgetRun.mockResolvedValue(okRun);

    render(<RunHistoryDrawer open dashboardId="dash-1" widget={widget} onClose={vi.fn()} />);

    await waitFor(() => expect(mockGetWidgetRun).toHaveBeenCalled());

    mockGetWidgetRun.mockClear();
    mockGetWidgetRun.mockResolvedValue({ ...okRun, id: 'run-2' });

    const buttons = screen.getAllByRole('button');
    const run2Button = buttons.find((b) => b.textContent?.includes('error'));
    expect(run2Button).toBeTruthy();
    fireEvent.click(run2Button!);

    await waitFor(() => expect(mockGetWidgetRun).toHaveBeenCalledWith('dash-1', 'w1', 'run-2'));
  });

  it('toggles chart and table views for ok runs', async () => {
    mockListWidgetRuns.mockResolvedValue([
      { id: 'run-1', startedAt: '2026-01-01T00:00:00Z', status: 'ok' },
    ]);
    mockGetWidgetRun.mockResolvedValue(okRun);

    render(<RunHistoryDrawer open dashboardId="dash-1" widget={widget} onClose={vi.fn()} />);

    await waitFor(() => expect(screen.getByTestId('run-history-view-toggle')).toBeInTheDocument());

    const toggle = screen.getByTestId('run-history-view-toggle');
    const tableBtn = toggle.querySelectorAll('button')[1]!;
    fireEvent.click(tableBtn);
    expect(screen.getByTestId('mock-data-table')).toBeInTheDocument();

    const chartBtn = toggle.querySelectorAll('button')[0]!;
    fireEvent.click(chartBtn);
    expect(screen.getByTestId('mock-chart-canvas')).toBeInTheDocument();
  });

  it('shows index error when listWidgetRuns fails', async () => {
    mockListWidgetRuns.mockRejectedValue(new Error('index boom'));

    render(<RunHistoryDrawer open dashboardId="dash-1" widget={widget} onClose={vi.fn()} />);

    await waitFor(() => expect(screen.getByText('Error: index boom')).toBeInTheDocument());
  });

  it('shows run error when getWidgetRun fails', async () => {
    mockListWidgetRuns.mockResolvedValue([
      { id: 'run-1', startedAt: '2026-01-01T00:00:00Z', status: 'ok' },
    ]);
    mockGetWidgetRun.mockRejectedValue(new Error('run load failed'));

    render(<RunHistoryDrawer open dashboardId="dash-1" widget={widget} onClose={vi.fn()} />);

    await waitFor(() => expect(screen.getByText('Error: run load failed')).toBeInTheDocument());
  });

  it('shows error status message for failed runs', async () => {
    mockListWidgetRuns.mockResolvedValue([
      { id: 'run-err', startedAt: '2026-01-01T00:00:00Z', status: 'error' },
    ]);
    mockGetWidgetRun.mockResolvedValue({
      ...okRun,
      id: 'run-err',
      status: 'error',
      error: 'Query failed',
    });

    render(<RunHistoryDrawer open dashboardId="dash-1" widget={widget} onClose={vi.fn()} />);

    await waitFor(() => expect(screen.getByText('Query failed')).toBeInTheDocument());
  });

  it('shows timeout status message', async () => {
    mockListWidgetRuns.mockResolvedValue([
      { id: 'run-t', startedAt: '2026-01-01T00:00:00Z', status: 'timeout' },
    ]);
    mockGetWidgetRun.mockResolvedValue({
      ...okRun,
      id: 'run-t',
      status: 'timeout',
    });

    render(<RunHistoryDrawer open dashboardId="dash-1" widget={widget} onClose={vi.fn()} />);

    await waitFor(() => expect(screen.getByText('dashboard.runTimeout')).toBeInTheDocument());
  });

  it('calls onClose from backdrop click', async () => {
    mockListWidgetRuns.mockResolvedValue([]);
    const onClose = vi.fn();

    render(<RunHistoryDrawer open dashboardId="dash-1" widget={widget} onClose={onClose} />);

    await waitFor(() => expect(mockListWidgetRuns).toHaveBeenCalled());
    fireEvent.click(screen.getByLabelText('common.close'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
