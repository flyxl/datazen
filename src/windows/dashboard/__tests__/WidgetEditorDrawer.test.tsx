import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen } from '@testing-library/react';
import { WidgetEditorDrawer } from '../WidgetEditorDrawer';
import type { DashboardWidget } from '../../../types/dashboard';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({
    t: (key: string, vars?: Record<string, unknown>) =>
      vars ? `${key}:${JSON.stringify(vars)}` : key,
  }),
}));

vi.mock('../../../stores/connectionStore', () => ({
  useConnectionStore: (
    sel: (s: {
      connections: { id: string; name: string }[];
      fetchConnections: () => void;
    }) => unknown,
  ) => sel({ connections: [{ id: 'c1', name: 'Conn 1' }], fetchConnections: vi.fn() }),
}));

const baseWidget: DashboardWidget = {
  id: 'w1',
  title: 'Tile',
  workflowId: 'wf-hidden',
  viewMode: 'chart',
  chartConfig: {
    chartType: 'bar',
    xAxis: null,
    yAxes: ['v'],
    groupBy: null,
    aggregation: 'none',
    sortBy: 'none',
    showLegend: true,
    showGrid: true,
    showValues: false,
    colorScheme: 'default',
  },
  layout: { x: 0, y: 0, w: 6, h: 4 },
  refresh: { mode: 'interval', refreshSec: 45 },
  enabled: true,
};

beforeEach(() => {
  vi.clearAllMocks();
});

afterEach(cleanup);

describe('WidgetEditorDrawer', () => {
  it('shows dense refresh warning when interval is below threshold', () => {
    render(
      <WidgetEditorDrawer
        open
        widget={baseWidget}
        hiddenSql={{ configId: 'c1', sql: 'SELECT 1 AS v' }}
        onClose={vi.fn()}
        onSave={vi.fn()}
      />,
    );

    expect(screen.getByTestId('refresh-sec-warn')).toBeInTheDocument();
    expect(screen.getByTestId('refresh-sec-warn').textContent).toContain(
      'dashboard.refreshSecWarn',
    );
  });

  it('does not show refresh warning for manual mode', () => {
    render(
      <WidgetEditorDrawer
        open
        widget={{ ...baseWidget, refresh: { mode: 'manual' } }}
        hiddenSql={{ configId: 'c1', sql: 'SELECT 1 AS v' }}
        onClose={vi.fn()}
        onSave={vi.fn()}
      />,
    );

    expect(screen.queryByTestId('refresh-sec-warn')).not.toBeInTheDocument();
  });

  it('calls onSave with normalized refresh policy', () => {
    const onSave = vi.fn();
    render(
      <WidgetEditorDrawer
        open
        widget={{
          ...baseWidget,
          title: '  My Tile  ',
          refresh: { mode: 'interval', refreshSec: 10 },
        }}
        hiddenSql={{ configId: 'c1', sql: 'SELECT 1 AS v' }}
        onClose={vi.fn()}
        onSave={onSave}
      />,
    );

    fireEvent.click(screen.getByTestId('widget-editor-save'));
    expect(onSave).toHaveBeenCalledTimes(1);
    const [saved] = onSave.mock.calls[0] as [DashboardWidget];
    expect(saved.title).toBe('My Tile');
    expect(saved.refresh.mode).toBe('interval');
    expect(saved.refresh.refreshSec).toBeGreaterThanOrEqual(30);
  });
});
