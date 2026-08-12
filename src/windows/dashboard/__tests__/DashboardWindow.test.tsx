import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, cleanup, fireEvent, screen, waitFor } from '@testing-library/react';
import type { Dashboard, DashboardWidget, WidgetRun } from '../../../types/dashboard';
import { DEFAULT_CHART_CONFIG } from '../../../types/chart';

const {
  mockGetUrlParam,
  mockDashboardCommands,
  mockAiCommands,
  mockOpenDashboardWindow,
  mockOpenDocsWindow,
  mockOpenWorkflowWindow,
  editorPropsRef,
  historyPropsRef,
  stableT,
} = vi.hoisted(() => {
  const editorPropsRef: { current: Record<string, unknown> | null } = { current: null };
  const historyPropsRef: { current: Record<string, unknown> | null } = { current: null };
  const stableT = (key: string, params?: Record<string, unknown>) =>
    params ? `${key}:${JSON.stringify(params)}` : key;

  return {
    mockGetUrlParam: vi.fn(() => ''),
    mockDashboardCommands: {
      listDashboards: vi.fn(),
      getDashboard: vi.fn(),
      saveDashboard: vi.fn(),
      deleteDashboard: vi.fn(),
      setDashboardRefreshPaused: vi.fn(),
      listWidgetRuns: vi.fn().mockResolvedValue([]),
      getWidgetRun: vi.fn(),
      runDashboardWidget: vi.fn(),
      createWidgetFromSql: vi.fn(),
      updateHiddenWidgetSql: vi.fn(),
      exportWithDialog: vi.fn(),
      importWithDialog: vi.fn(),
    },
    mockAiCommands: {
      workflowGet: vi.fn(),
      workflowList: vi.fn(),
    },
    mockOpenDashboardWindow: vi.fn(),
    mockOpenDocsWindow: vi.fn(),
    mockOpenWorkflowWindow: vi.fn(),
    editorPropsRef,
    historyPropsRef,
    stableT,
  };
});

vi.mock('../../../hooks/useThemeListener', () => ({
  useThemeListener: vi.fn(),
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: stableT }),
}));

vi.mock('../../../lib/windowKind', () => ({
  getUrlParam: (key: string) => mockGetUrlParam(key),
}));

vi.mock('../../../lib/windowManager', () => ({
  openDashboardWindow: (...args: unknown[]) => mockOpenDashboardWindow(...args),
  openDocsWindow: (...args: unknown[]) => mockOpenDocsWindow(...args),
  openWorkflowWindow: (...args: unknown[]) => mockOpenWorkflowWindow(...args),
}));

vi.mock('../../../commands/dashboard', () => ({
  dashboardCommands: mockDashboardCommands,
}));

vi.mock('../../../commands/ai', () => ({
  aiCommands: mockAiCommands,
}));

vi.mock('../../../components/TitleBar', () => ({
  TitleBar: ({
    title,
    rightContent,
  }: {
    title?: React.ReactNode;
    rightContent?: React.ReactNode;
  }) => (
    <div data-testid="title-bar">
      <div data-testid="title-bar-title">{title}</div>
      <div data-testid="title-bar-right">{rightContent}</div>
    </div>
  ),
}));

vi.mock('../../../components/StatusBar', () => ({
  StatusBar: ({ left }: { left?: string }) => <div data-testid="status-bar">{left}</div>,
}));

vi.mock('../ChartWidgetTile', () => ({
  ChartWidgetTile: ({
    widget,
    onEdit,
    onDelete,
    onHistory,
    onRefresh,
    onViewModeChange,
  }: {
    widget: DashboardWidget;
    onEdit: () => void;
    onDelete: () => void;
    onHistory: () => void;
    onRefresh: () => void;
    onViewModeChange: (mode: 'chart' | 'table') => void;
  }) => (
    <div data-testid={`tile-${widget.id}`}>
      <span>{widget.title}</span>
      <button type="button" data-testid={`tile-edit-${widget.id}`} onClick={onEdit}>
        edit
      </button>
      <button type="button" data-testid={`tile-delete-${widget.id}`} onClick={onDelete}>
        delete
      </button>
      <button type="button" data-testid={`tile-history-${widget.id}`} onClick={onHistory}>
        history
      </button>
      <button type="button" data-testid={`tile-refresh-${widget.id}`} onClick={onRefresh}>
        refresh
      </button>
      <button
        type="button"
        data-testid={`tile-view-table-${widget.id}`}
        onClick={() => onViewModeChange('table')}
      >
        table
      </button>
    </div>
  ),
}));

vi.mock('../WidgetEditorDrawer', () => ({
  WidgetEditorDrawer: (props: Record<string, unknown>) => {
    editorPropsRef.current = props;
    if (!props.open) return null;
    const widget = props.widget as DashboardWidget;
    return (
      <div data-testid="mock-widget-editor">
        <button
          type="button"
          data-testid="mock-editor-save"
          onClick={() =>
            (props.onSave as (w: DashboardWidget, h?: { configId: string; sql: string }) => void)(
              widget,
              props.hiddenSql as { configId: string; sql: string } | undefined,
            )
          }
        >
          save
        </button>
        <button
          type="button"
          data-testid="mock-editor-save-no-hidden"
          onClick={() =>
            (props.onSave as (w: DashboardWidget) => void)({ ...widget, title: 'Saved Widget' })
          }
        >
          save-no-hidden
        </button>
        <button type="button" data-testid="mock-editor-close" onClick={props.onClose as () => void}>
          close
        </button>
      </div>
    );
  },
}));

vi.mock('../RunHistoryDrawer', () => ({
  RunHistoryDrawer: (props: Record<string, unknown>) => {
    historyPropsRef.current = props;
    if (!props.open) return null;
    return (
      <div data-testid="mock-run-history">
        <button
          type="button"
          data-testid="mock-history-close"
          onClick={props.onClose as () => void}
        >
          close
        </button>
      </div>
    );
  },
}));

function makeWidget(id: string, overrides?: Partial<DashboardWidget>): DashboardWidget {
  return {
    id,
    title: `Widget ${id}`,
    workflowId: 'wf-1',
    viewMode: 'chart',
    chartConfig: { ...DEFAULT_CHART_CONFIG, yAxes: ['v'] },
    layout: { x: 0, y: 0, w: 6, h: 4 },
    refresh: { mode: 'manual' },
    enabled: true,
    ...overrides,
  };
}

function makeDashboard(id: string, widgets: DashboardWidget[] = []): Dashboard {
  return {
    id,
    name: `Board ${id}`,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    layout: { cols: 12, rowHeight: 80 },
    widgets,
    enabled: true,
  };
}

function makeRun(dashboardId: string, widgetId: string): WidgetRun {
  return {
    id: `run-${widgetId}`,
    dashboardId,
    widgetId,
    workflowId: 'wf-1',
    startedAt: '2026-01-01T00:00:00Z',
    finishedAt: '2026-01-01T00:00:01Z',
    status: 'ok',
    rowCount: 1,
    columns: ['v'],
    rows: [[1]],
  };
}

let uuidCounter = 0;

beforeEach(async () => {
  vi.clearAllMocks();
  uuidCounter = 0;
  editorPropsRef.current = null;
  historyPropsRef.current = null;
  mockGetUrlParam.mockReturnValue('');
  vi.stubGlobal(
    'confirm',
    vi.fn(() => true),
  );
  vi.stubGlobal('crypto', {
    randomUUID: vi.fn(() => `uuid-${++uuidCounter}`),
  });

  const { useDashboardStore } = await import('../../../stores/dashboardStore');
  useDashboardStore.setState({
    dashboardsById: {},
    list: [],
    listError: null,
    listLoading: false,
  });

  mockDashboardCommands.listDashboards.mockResolvedValue([]);
  mockDashboardCommands.saveDashboard.mockImplementation(async (d: Dashboard) => d);
  mockDashboardCommands.getDashboard.mockImplementation(async (id: string) =>
    makeDashboard(id, [makeWidget('w1')]),
  );
  mockDashboardCommands.runDashboardWidget.mockImplementation(async (d: string, w: string) =>
    makeRun(d, w),
  );
  mockDashboardCommands.setDashboardRefreshPaused.mockResolvedValue(undefined);
  mockDashboardCommands.exportWithDialog.mockResolvedValue(true);
  mockDashboardCommands.importWithDialog.mockResolvedValue(null);
  mockDashboardCommands.deleteDashboard.mockResolvedValue(undefined);
  mockDashboardCommands.createWidgetFromSql.mockResolvedValue({
    widget: makeWidget('created-w'),
    workflowId: 'wf-hidden',
  });
  mockDashboardCommands.updateHiddenWidgetSql.mockResolvedValue(undefined);
  mockAiCommands.workflowGet.mockResolvedValue({
    visibility: 'user',
    steps: [],
    connection: 'c1',
  });
  mockAiCommands.workflowList.mockResolvedValue([{ id: 'wf-1', name: 'WF 1' }]);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

async function renderWindow() {
  const { DashboardWindow } = await import('../DashboardWindow');
  return render(<DashboardWindow />);
}

describe('DashboardWindow', () => {
  it('shows empty boards CTA and creates first board', async () => {
    mockDashboardCommands.listDashboards.mockResolvedValue([]);

    await renderWindow();

    await waitFor(() => expect(screen.getByTestId('dashboard-empty-boards')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('dashboard-create-first'));

    await waitFor(() => expect(mockDashboardCommands.saveDashboard).toHaveBeenCalled());
    await waitFor(() => expect(screen.getByTestId('dashboard-window')).toBeInTheDocument());
  });

  it('bootstraps with existing boards and selects the first tab', async () => {
    const boards = [makeDashboard('d1'), makeDashboard('d2')];
    mockDashboardCommands.listDashboards.mockResolvedValue(boards);
    mockDashboardCommands.getDashboard.mockResolvedValue(boards[0]!);

    await renderWindow();

    await waitFor(() => expect(screen.getAllByTestId('dashboard-tab')).toHaveLength(2));
    const tabs = screen.getAllByTestId('dashboard-tab');
    expect(tabs[0]).toHaveAttribute('data-dashboard-id', 'd1');
  });

  it('mounts dashboard from urlDashboardId param', async () => {
    mockGetUrlParam.mockImplementation((key: string) => (key === 'dashboardId' ? 'url-dash' : ''));
    const board = makeDashboard('url-dash', [makeWidget('w-url')]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);

    await renderWindow();

    await waitFor(() =>
      expect(mockDashboardCommands.getDashboard).toHaveBeenCalledWith('url-dash'),
    );
    await waitFor(() => expect(screen.getByTestId('tile-w-url')).toBeInTheDocument());
  });

  it('switches tabs and creates a new panel', async () => {
    const boards = [makeDashboard('d1'), makeDashboard('d2', [makeWidget('w2')])];
    mockDashboardCommands.listDashboards.mockResolvedValue(boards);
    mockDashboardCommands.getDashboard.mockImplementation(
      async (id: string) => boards.find((b) => b.id === id) ?? boards[0]!,
    );

    await renderWindow();
    await waitFor(() => expect(screen.getAllByTestId('dashboard-tab')).toHaveLength(2));

    fireEvent.click(screen.getAllByTestId('dashboard-tab')[1]!);
    await waitFor(() => expect(screen.getByTestId('tile-w2')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('dashboard-tab-add'));
    await waitFor(() => expect(mockDashboardCommands.saveDashboard).toHaveBeenCalled());
  });

  it('deletes panel after confirm', async () => {
    const boards = [makeDashboard('d1', [makeWidget('w1')]), makeDashboard('d2')];
    mockDashboardCommands.listDashboards.mockResolvedValue(boards);
    mockDashboardCommands.getDashboard.mockImplementation(
      async (id: string) => boards.find((b) => b.id === id) ?? boards[0]!,
    );

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('dashboard-delete-panel')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('dashboard-delete-panel'));
    await waitFor(() => expect(mockDashboardCommands.deleteDashboard).toHaveBeenCalledWith('d1'));
  });

  it('opens widget editor for new widget and saves with hidden SQL', async () => {
    const board = makeDashboard('d1', []);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('dashboard-add-widget')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('dashboard-add-widget'));
    await waitFor(() => expect(screen.getByTestId('mock-widget-editor')).toBeInTheDocument());
    expect(editorPropsRef.current?.isNew).toBe(true);
    expect(editorPropsRef.current?.hiddenSql).toEqual({ configId: '', sql: 'SELECT 1 AS v' });

    fireEvent.click(screen.getByTestId('mock-editor-save'));
    await waitFor(() => expect(mockDashboardCommands.createWidgetFromSql).toHaveBeenCalled());
    await waitFor(() => expect(mockDashboardCommands.getDashboard).toHaveBeenCalled());
  });

  it('edits existing widget with hidden workflow and saves SQL update', async () => {
    const board = makeDashboard('d1', [makeWidget('w1')]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);
    mockAiCommands.workflowGet.mockResolvedValue({
      visibility: 'dashboardHidden',
      connection: 'c1',
      steps: [{ type: 'query', sql: 'SELECT old AS v' }],
    });

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('tile-edit-w1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('tile-edit-w1'));
    await waitFor(() => expect(screen.getByTestId('mock-widget-editor')).toBeInTheDocument());
    expect(editorPropsRef.current?.hiddenSql).toEqual({
      configId: 'c1',
      sql: 'SELECT old AS v',
    });

    fireEvent.click(screen.getByTestId('mock-editor-save'));
    await waitFor(() =>
      expect(mockDashboardCommands.updateHiddenWidgetSql).toHaveBeenCalledWith({
        workflowId: 'wf-1',
        configId: 'c1',
        sql: 'SELECT old AS v',
      }),
    );
  });

  it('saves existing widget without hidden SQL path', async () => {
    const board = makeDashboard('d1', [makeWidget('w1')]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('tile-edit-w1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('tile-edit-w1'));
    await waitFor(() => expect(screen.getByTestId('mock-widget-editor')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('mock-editor-save-no-hidden'));
    await waitFor(() => expect(mockDashboardCommands.saveDashboard).toHaveBeenCalled());
    await waitFor(() => expect(mockDashboardCommands.runDashboardWidget).toHaveBeenCalled());
  });

  it('refreshes all widgets and toggles monitor pause', async () => {
    const board = makeDashboard('d1', [
      makeWidget('w1', { enabled: true }),
      makeWidget('w2', { enabled: false }),
    ]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('dashboard-refresh-all')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('dashboard-refresh-all'));
    await waitFor(() =>
      expect(mockDashboardCommands.runDashboardWidget).toHaveBeenCalledWith('d1', 'w1'),
    );
    expect(mockDashboardCommands.runDashboardWidget).not.toHaveBeenCalledWith('d1', 'w2');

    fireEvent.click(screen.getByTestId('dashboard-pause-toggle'));
    await waitFor(() =>
      expect(mockDashboardCommands.setDashboardRefreshPaused).toHaveBeenCalledWith('d1', true),
    );
  });

  it('changes widget view mode and deletes widget', async () => {
    const board = makeDashboard('d1', [makeWidget('w1')]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('tile-view-table-w1')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('tile-view-table-w1'));
    await waitFor(() => expect(mockDashboardCommands.saveDashboard).toHaveBeenCalled());

    fireEvent.click(screen.getByTestId('tile-delete-w1'));
    await waitFor(() => expect(mockDashboardCommands.saveDashboard).toHaveBeenCalledTimes(2));
  });

  it('renames dashboard from title click', async () => {
    const board = makeDashboard('d1', []);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    const titleBar = await screen.findByTestId('title-bar-title');
    await waitFor(() => expect(titleBar.textContent).toContain('Board d1'));

    fireEvent.click(titleBar.querySelector('button')!);
    const input = screen.getByDisplayValue('Board d1');
    fireEvent.change(input, { target: { value: 'Renamed Board' } });
    fireEvent.blur(input);

    await waitFor(() =>
      expect(mockDashboardCommands.saveDashboard).toHaveBeenCalledWith(
        expect.objectContaining({ name: 'Renamed Board' }),
      ),
    );
  });

  it('handles import and export actions', async () => {
    const board = makeDashboard('d1', [makeWidget('w1')]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('dashboard-export')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('dashboard-export'));
    await waitFor(() =>
      expect(mockDashboardCommands.exportWithDialog).toHaveBeenCalledWith('d1', 'Board_d1.json'),
    );

    mockDashboardCommands.importWithDialog.mockResolvedValue(makeDashboard('imported', []));
    fireEvent.click(screen.getByTestId('dashboard-import'));
    await waitFor(() =>
      expect(mockOpenDashboardWindow).toHaveBeenCalledWith('imported', 'Board imported'),
    );
  });

  it('reloads same dashboard when import returns current id', async () => {
    const board = makeDashboard('d1', [makeWidget('w1')]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('dashboard-import')).toBeInTheDocument());

    mockDashboardCommands.importWithDialog.mockResolvedValue(board);
    mockDashboardCommands.getDashboard.mockClear();
    fireEvent.click(screen.getByTestId('dashboard-import'));
    await waitFor(() => expect(mockDashboardCommands.getDashboard).toHaveBeenCalledWith('d1'));
    expect(mockOpenDashboardWindow).not.toHaveBeenCalled();
  });

  it('opens docs help and run history drawer', async () => {
    const board = makeDashboard('d1', [makeWidget('w1')]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('dashboard-docs-help')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('dashboard-docs-help'));
    expect(mockOpenDocsWindow).toHaveBeenCalledWith('opsDashboard');

    fireEvent.click(screen.getByTestId('tile-history-w1'));
    await waitFor(() => expect(screen.getByTestId('mock-run-history')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('mock-history-close'));
    await waitFor(() => expect(screen.queryByTestId('mock-run-history')).not.toBeInTheDocument());
  });

  it('closes widget editor via mock close button', async () => {
    const board = makeDashboard('d1', [makeWidget('w1')]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    fireEvent.click(await screen.findByTestId('tile-edit-w1'));
    await waitFor(() => expect(screen.getByTestId('mock-widget-editor')).toBeInTheDocument());

    fireEvent.click(screen.getByTestId('mock-editor-close'));
    await waitFor(() => expect(screen.queryByTestId('mock-widget-editor')).not.toBeInTheDocument());
  });

  it('shows loading state while dashboard entry is loading', async () => {
    mockGetUrlParam.mockImplementation((key: string) => (key === 'dashboardId' ? 'slow-dash' : ''));
    mockDashboardCommands.listDashboards.mockResolvedValue([makeDashboard('slow-dash')]);
    mockDashboardCommands.getDashboard.mockImplementation(
      () => new Promise((resolve) => setTimeout(() => resolve(makeDashboard('slow-dash')), 50)),
    );

    await renderWindow();
    expect(screen.getByText('common.loading')).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId('dashboard-window')).toBeInTheDocument());
  });

  it('shows error banner when dashboard load fails', async () => {
    mockGetUrlParam.mockImplementation((key: string) => (key === 'dashboardId' ? 'bad-dash' : ''));
    mockDashboardCommands.listDashboards.mockResolvedValue([makeDashboard('bad-dash')]);
    mockDashboardCommands.getDashboard.mockRejectedValue(new Error('load failed'));

    await renderWindow();
    await waitFor(() => expect(screen.getByText('load failed')).toBeInTheDocument());
  });

  it('auto-refreshes onOpen widgets without existing runs', async () => {
    const board = makeDashboard('d1', [makeWidget('w1', { refresh: { mode: 'onOpen' } })]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    await waitFor(() =>
      expect(mockDashboardCommands.runDashboardWidget).toHaveBeenCalledWith('d1', 'w1'),
    );
  });

  it('shows empty widget state and add widget from empty panel', async () => {
    const board = makeDashboard('d1', []);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('dashboard-empty')).toBeInTheDocument());

    const addButtons = screen.getAllByText('dashboard.addWidget');
    fireEvent.click(addButtons[addButtons.length - 1]!);
    await waitFor(() => expect(screen.getByTestId('mock-widget-editor')).toBeInTheDocument());
  });

  it('skips delete panel when confirm is cancelled', async () => {
    vi.stubGlobal(
      'confirm',
      vi.fn(() => false),
    );
    const board = makeDashboard('d1', [makeWidget('w1')]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);

    await renderWindow();
    await waitFor(() => expect(screen.getByTestId('dashboard-delete-panel')).toBeInTheDocument());
    fireEvent.click(screen.getByTestId('dashboard-delete-panel'));
    expect(mockDashboardCommands.deleteDashboard).not.toHaveBeenCalled();
  });

  it('loads user workflows when editing visible workflow widget', async () => {
    const board = makeDashboard('d1', [makeWidget('w1')]);
    mockDashboardCommands.listDashboards.mockResolvedValue([board]);
    mockDashboardCommands.getDashboard.mockResolvedValue(board);
    mockAiCommands.workflowGet.mockResolvedValue({
      visibility: 'user',
      steps: [],
    });

    await renderWindow();
    fireEvent.click(await screen.findByTestId('tile-edit-w1'));
    await waitFor(() => expect(mockAiCommands.workflowList).toHaveBeenCalled());
    expect(editorPropsRef.current?.userWorkflows).toEqual([{ id: 'wf-1', name: 'WF 1' }]);
  });
});
