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

vi.mock('../../../stores/connectionStore', () => {
  const fetchConnections = vi.fn();
  return {
    useConnectionStore: (
      sel: (s: {
        connections: { id: string; name: string }[];
        fetchConnections: () => void;
      }) => unknown,
    ) => sel({ connections: [{ id: 'c1', name: 'Conn 1' }], fetchConnections }),
  };
});

vi.mock('../../../components/ui/Select', () => ({
  Select: ({
    value,
    options,
    onChange,
  }: {
    value: string;
    options: { value: string; label: string }[];
    onChange: (v: string) => void;
  }) => (
    <select data-testid="mock-select" value={value} onChange={(e) => onChange(e.target.value)}>
      {options.map((o) => (
        <option key={o.value} value={o.value}>
          {o.label}
        </option>
      ))}
    </select>
  ),
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
  it('renders null when open is false', () => {
    const { container } = render(
      <WidgetEditorDrawer open={false} widget={baseWidget} onClose={vi.fn()} onSave={vi.fn()} />,
    );
    expect(container.firstChild).toBeNull();
  });

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

  it('calls onSave with normalized refresh policy and hiddenSql', () => {
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
    const [saved, hiddenSql] = onSave.mock.calls[0] as [
      DashboardWidget,
      { configId: string; sql: string },
    ];
    expect(saved.title).toBe('My Tile');
    expect(saved.refresh.mode).toBe('interval');
    expect(saved.refresh.refreshSec).toBeGreaterThanOrEqual(30);
    expect(hiddenSql).toEqual({ configId: 'c1', sql: 'SELECT 1 AS v' });
  });

  it('edits title, toggles enabled, and changes refresh mode', () => {
    const onSave = vi.fn();
    render(
      <WidgetEditorDrawer
        open
        widget={{ ...baseWidget, title: 'Original', refresh: { mode: 'manual' }, enabled: true }}
        onClose={vi.fn()}
        onSave={onSave}
      />,
    );

    fireEvent.change(screen.getByDisplayValue('Original'), {
      target: { value: 'Updated Title' },
    });

    const enabledCheckbox = screen.getAllByRole('checkbox')[0]!;
    fireEvent.click(enabledCheckbox);

    const selects = screen.getAllByTestId('mock-select');
    const refreshSelect = selects.find((s) => (s as HTMLSelectElement).value === 'manual');
    fireEvent.change(refreshSelect!, { target: { value: 'onOpen' } });

    fireEvent.click(screen.getByTestId('widget-editor-save'));

    const [saved] = onSave.mock.calls[0] as [DashboardWidget];
    expect(saved.title).toBe('Updated Title');
    expect(saved.enabled).toBe(false);
    expect(saved.refresh.mode).toBe('onOpen');
  });

  it('changes hidden SQL fields and passes them on save', () => {
    const onSave = vi.fn();
    render(
      <WidgetEditorDrawer
        open
        widget={baseWidget}
        hiddenSql={{ configId: 'c1', sql: 'SELECT 1 AS v' }}
        onClose={vi.fn()}
        onSave={onSave}
      />,
    );

    fireEvent.change(screen.getByDisplayValue('SELECT 1 AS v'), {
      target: { value: 'SELECT 2 AS v' },
    });

    fireEvent.click(screen.getByTestId('widget-editor-save'));

    const [, hiddenSql] = onSave.mock.calls[0] as [
      DashboardWidget,
      { configId: string; sql: string },
    ];
    expect(hiddenSql.sql).toBe('SELECT 2 AS v');
  });

  it('shows workflow select and open workflow editor button', () => {
    const onOpenWorkflowEditor = vi.fn();
    render(
      <WidgetEditorDrawer
        open
        widget={{ ...baseWidget, workflowId: 'wf-1' }}
        userWorkflows={[
          { id: 'wf-1', name: 'Workflow One' },
          { id: 'wf-2', name: 'Workflow Two' },
        ]}
        onOpenWorkflowEditor={onOpenWorkflowEditor}
        onClose={vi.fn()}
        onSave={vi.fn()}
      />,
    );

    const workflowSelect = screen.getAllByTestId('mock-select')[0] as HTMLSelectElement;
    expect(workflowSelect.value).toBe('wf-1');
    fireEvent.click(screen.getByTestId('widget-open-workflow-editor'));
    expect(onOpenWorkflowEditor).toHaveBeenCalledTimes(1);
  });

  it('enables alert rule and saves alert config', () => {
    const onSave = vi.fn();
    render(
      <WidgetEditorDrawer
        open
        widget={{ ...baseWidget, title: 'Alert Tile' }}
        onClose={vi.fn()}
        onSave={onSave}
      />,
    );

    const alertCheckbox = screen
      .getAllByRole('checkbox')
      .find((el) => el.closest('label')?.textContent?.includes('dashboard.alertEnabled'));
    fireEvent.click(alertCheckbox!);

    fireEvent.change(screen.getByPlaceholderText('v'), { target: { value: 'amount' } });
    fireEvent.click(screen.getByTestId('widget-editor-save'));

    const [saved] = onSave.mock.calls[0] as [DashboardWidget];
    expect(saved.alert?.metric.column).toBe('amount');
    expect(saved.alert?.op).toBe('>');
  });

  it('calls onClose from cancel and backdrop', () => {
    const onClose = vi.fn();
    render(<WidgetEditorDrawer open widget={baseWidget} onClose={onClose} onSave={vi.fn()} />);

    fireEvent.click(screen.getByText('common.cancel'));
    expect(onClose).toHaveBeenCalledTimes(1);

    onClose.mockClear();
    fireEvent.click(screen.getByLabelText('common.close'));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it('does not save when title is empty', () => {
    const onSave = vi.fn();
    render(
      <WidgetEditorDrawer
        open
        widget={{ ...baseWidget, title: '' }}
        onClose={vi.fn()}
        onSave={onSave}
      />,
    );

    fireEvent.click(screen.getByTestId('widget-editor-save'));
    expect(onSave).not.toHaveBeenCalled();
  });
});
