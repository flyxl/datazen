import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, waitFor, screen, fireEvent } from '@testing-library/react';

const executeMock = vi.hoisted(() => vi.fn());

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

vi.mock('../../../commands/driver', () => ({
  driverCommands: {
    execute: (...args: unknown[]) => executeMock(...args),
  },
}));

// Mock chart shell so the dashboard can render charts headlessly.
vi.mock('../../../components/chart/ChartCanvas', () => ({
  ChartCanvas: ({ data }: { data: unknown[] }) => (
    <div data-testid="chart-canvas">{JSON.stringify(data.length)}</div>
  ),
}));

vi.mock('../../../components/ui/Button', () => ({
  Button: ({ children, ...rest }: { children?: React.ReactNode }) => (
    <button type="button" {...rest}>
      {children}
    </button>
  ),
}));

vi.mock('../../../components/ui/Select', () => ({
  Select: ({ value, options, onChange }: any) => (
    <select
      title="auto-refresh"
      value={String(value)}
      onChange={(e: any) => onChange?.(Number(e.target.value))}
    >
      {options.map((o: any) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  ),
}));

import { ServerStatusView } from '../ServerStatusView';

/** A flat snapshot exactly like `server_status_snapshot` data payloads. */
function payload(overrides: Record<string, unknown> = {}) {
  return {
    version: '9.7.1',
    database: 'db',
    uptimeSeconds: 100,
    connections: 3,
    maxConnections: 151,
    activeQueries: 2,
    qps: '0.34',
    statusVariables: [{ name: 'X', value: 'y' }],
    ...overrides,
  };
}

/** 构造带时间戳的 TrendSeries 夹具：values 顺序即时间递增。 */
function series(baseTs: number, values: number[]): { t: number; v: number }[] {
  return values.map((v, i) => ({ t: baseTs + i * 5000, v }));
}

describe('ServerStatusView data cards + charts (data-driven)', () => {
  beforeEach(() => {
    executeMock.mockReset();
  });

  afterEach(() => {
    cleanup();
  });

  it('renders a stat-card grid from available snapshot fields only', async () => {
    executeMock.mockImplementation(async () => ({ data: payload() }));
    render(<ServerStatusView dbSessionId="conn-mysql" connectionName="mysql" />);

    await waitFor(() => {
      expect(screen.getByText('serverStatus.connections')).toBeInTheDocument();
    });
    // A card whose backing field is absent must not render.
    expect(screen.queryByText('serverStatus.deadlocks')).not.toBeInTheDocument();
    expect(screen.queryByText('serverStatus.cacheHitRatio')).not.toBeInTheDocument();
  });

  it('shows the cache-hit-ratio card when the snapshot provides it', async () => {
    executeMock.mockImplementation(async () => ({
      data: payload({ cacheHitRatio: '99.86%' }),
    }));
    render(<ServerStatusView dbSessionId="conn-pg" connectionName="pg" />);

    await waitFor(() => {
      expect(screen.getByText('serverStatus.cacheHitRatio')).toBeInTheDocument();
    });
  });

  it('renders full-size chart cards when backing trend series exist', async () => {
    // Pre-populate history so charts render from the very first frame (seeding
    // needs a live second poll; caching proves the data-driven panel selection).
    const data = payload();
    executeMock.mockImplementation(async () => ({ data }));
    render(
      <ServerStatusView
        dbSessionId="conn-mysql"
        connectionName="mysql"
        initialData={{
          status: data,
          variables: data.statusVariables,
          history: {
            qps: series(Date.now(), [1, 2, 3]),
            netIn: series(Date.now(), [10, 20]),
            netOut: series(Date.now(), [5, 6]),
          },
        }}
      />,
    );

    await waitFor(() => {
      expect(screen.getAllByTestId('chart-canvas').length).toBeGreaterThan(0);
    });
    // commands chart hidden because no cmd_* series in the history.
    expect(screen.queryByText('serverStatus.chartCommands')).not.toBeInTheDocument();
  });

  it('wraps every chart canvas in a `relative` container so recharts stays inside its card', async () => {
    const data = payload();
    executeMock.mockImplementation(async () => ({ data }));
    render(
      <ServerStatusView
        dbSessionId="conn-pg"
        initialData={{
          status: data,
          variables: data.statusVariables,
          history: {
            qps: series(Date.now(), [1, 2, 3]),
            netIn: series(Date.now(), [10, 20]),
            netOut: series(Date.now(), [5, 6]),
          },
        }}
      />,
    );

    await waitFor(() => {
      expect(screen.getAllByTestId('chart-canvas').length).toBeGreaterThan(0);
    });
    // Regression: ChartCanvas uses `absolute inset-0`; without a `relative`
    // ancestor the recharts SVG escapes to page size and draws overflowing
    // splines across the whole dashboard.
    const canvases = screen.getAllByTestId('chart-canvas');
    for (const c of canvases) {
      expect(c.closest('.relative')).not.toBeNull();
    }
  });

  it('reloads snapshot when dbSessionId changes (per-tab binding)', async () => {
    executeMock.mockImplementation(async ({ dbSessionId }: { dbSessionId: string }) => ({
      data: payload({ uptimeSeconds: dbSessionId === 'a' ? 1 : 2 }),
    }));
    const { rerender } = render(<ServerStatusView dbSessionId="a" />);
    await waitFor(() => {
      expect(executeMock).toHaveBeenCalledWith(
        expect.objectContaining({ command: 'server_status_snapshot', dbSessionId: 'a' }),
      );
    });
    rerender(<ServerStatusView dbSessionId="b" />);
    await waitFor(() => {
      expect(executeMock).toHaveBeenCalledWith(
        expect.objectContaining({ command: 'server_status_snapshot', dbSessionId: 'b' }),
      );
    });
  });

  it('shows the refresh spinner only on manual refresh, not on auto-refresh', async () => {
    // First load resolves immediately; we can then observe a deferred manual load.
    let done: () => void = () => {};
    const first = Promise.resolve({ data: payload() });
    executeMock
      .mockImplementationOnce(() => first)
      .mockImplementationOnce(
        () =>
          new Promise((resolve) => {
            done = () => resolve({ data: payload() });
          }),
      );

    render(<ServerStatusView dbSessionId="conn-mysql" />);
    // Auto/initial load finishes → button returns to static (not disabled, no spin).
    await waitFor(() => {
      expect(screen.getByTestId('server-dashboard-refresh')).toBeEnabled();
    });

    // Manual refresh: button becomes disabled (manual loading) until it resolves.
    fireEvent.click(screen.getByTestId('server-dashboard-refresh'));
    await waitFor(() => {
      expect(screen.getByTestId('server-dashboard-refresh')).toBeDisabled();
    });

    // Resolve the manual load → button re-enabled.
    done();
    await waitFor(() => {
      expect(screen.getByTestId('server-dashboard-refresh')).toBeEnabled();
    });
  });
});
