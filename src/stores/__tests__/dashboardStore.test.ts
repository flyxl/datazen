import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { Dashboard, WidgetRun } from '../../types/dashboard';
import { DEFAULT_CHART_CONFIG } from '../../types/chart';

const listenHandlers: Record<string, (...args: unknown[]) => void> = {};

const mockDashboardCommands = {
  listDashboards: vi.fn(),
  getDashboard: vi.fn(),
  saveDashboard: vi.fn(),
  deleteDashboard: vi.fn(),
  listWidgetRuns: vi.fn().mockResolvedValue([]),
  getWidgetRun: vi.fn(),
  runDashboardWidget: vi.fn(),
  setDashboardRefreshPaused: vi.fn(),
};

vi.mock('../../commands/dashboard', () => ({
  dashboardCommands: mockDashboardCommands,
}));

vi.mock('../../lib/crossWindowBus', () => ({
  listenCrossWindow: vi.fn(async (event: string, handler: (...args: unknown[]) => void) => {
    listenHandlers[event] = handler;
    return () => {};
  }),
}));

function makeDashboard(id: string, widgetId: string, overrides?: Partial<Dashboard>): Dashboard {
  return {
    id,
    name: `Dashboard ${id}`,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    layout: { cols: 12, rowHeight: 80 },
    enabled: true,
    widgets: [
      {
        id: widgetId,
        title: 'Widget',
        workflowId: 'wf-1',
        viewMode: 'chart',
        chartConfig: { ...DEFAULT_CHART_CONFIG, yAxes: ['v'] },
        layout: { x: 0, y: 0, w: 6, h: 4 },
        refresh: { mode: 'manual' },
        enabled: true,
      },
    ],
    ...overrides,
  };
}

function makeRun(dashboardId: string, widgetId: string, id: string): WidgetRun {
  return {
    id,
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

describe('dashboardStore', () => {
  let useDashboardStore: typeof import('../dashboardStore').useDashboardStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    for (const key of Object.keys(listenHandlers)) delete listenHandlers[key];
    const mod = await import('../dashboardStore');
    useDashboardStore = mod.useDashboardStore;
    useDashboardStore.setState({
      dashboardsById: {},
      list: [],
      listError: null,
      listLoading: false,
    });
  });

  it('fetchDashboards sets list on success', async () => {
    const boards = [makeDashboard('d1', 'w1')];
    mockDashboardCommands.listDashboards.mockResolvedValue(boards);

    await useDashboardStore.getState().fetchDashboards();

    const state = useDashboardStore.getState();
    expect(state.list).toEqual(boards);
    expect(state.listLoading).toBe(false);
    expect(state.listError).toBeNull();
  });

  it('fetchDashboards sets listError on failure', async () => {
    mockDashboardCommands.listDashboards.mockRejectedValue(new Error('list failed'));

    await useDashboardStore.getState().fetchDashboards();

    const state = useDashboardStore.getState();
    expect(state.listError).toBe('list failed');
    expect(state.listLoading).toBe(false);
  });

  it('mountDashboard increments refCount and releaseDashboard cleans up at zero', () => {
    const store = useDashboardStore.getState();
    store.mountDashboard('dash-a');
    store.mountDashboard('dash-a');
    expect(useDashboardStore.getState().dashboardsById['dash-a']?.refCount).toBe(2);

    store.releaseDashboard('dash-a');
    expect(useDashboardStore.getState().dashboardsById['dash-a']?.refCount).toBe(1);

    store.releaseDashboard('dash-a');
    expect(useDashboardStore.getState().dashboardsById['dash-a']).toBeUndefined();
  });

  it('loadDashboard loads dashboard and latest ok run', async () => {
    const dash = makeDashboard('dash-a', 'widget-a');
    const run = makeRun('dash-a', 'widget-a', 'run-1');
    mockDashboardCommands.getDashboard.mockResolvedValue(dash);
    mockDashboardCommands.listWidgetRuns.mockResolvedValue([
      { id: 'run-err', startedAt: '2026-01-01T00:00:00Z', status: 'error' },
      { id: 'run-1', startedAt: '2026-01-01T00:00:01Z', status: 'ok' },
    ]);
    mockDashboardCommands.getWidgetRun.mockResolvedValue(run);

    await useDashboardStore.getState().loadDashboard('dash-a');

    const entry = useDashboardStore.getState().dashboardsById['dash-a'];
    expect(entry?.dashboard).toEqual(dash);
    expect(entry?.runs['widget-a']).toEqual(run);
    expect(entry?.loading).toBe(false);
    expect(mockDashboardCommands.getWidgetRun).toHaveBeenCalledWith('dash-a', 'widget-a', 'run-1');
  });

  it('loadDashboard falls back to first index entry when none are ok', async () => {
    const dash = makeDashboard('dash-a', 'widget-a');
    const run = makeRun('dash-a', 'widget-a', 'run-err');
    run.status = 'error';
    mockDashboardCommands.getDashboard.mockResolvedValue(dash);
    mockDashboardCommands.listWidgetRuns.mockResolvedValue([
      { id: 'run-err', startedAt: '2026-01-01T00:00:00Z', status: 'error' },
    ]);
    mockDashboardCommands.getWidgetRun.mockResolvedValue(run);

    await useDashboardStore.getState().loadDashboard('dash-a');

    expect(useDashboardStore.getState().dashboardsById['dash-a']?.runs['widget-a']).toEqual(run);
  });

  it('loadDashboard sets null run when latest run fetch fails', async () => {
    const dash = makeDashboard('dash-a', 'widget-a');
    mockDashboardCommands.getDashboard.mockResolvedValue(dash);
    mockDashboardCommands.listWidgetRuns.mockRejectedValue(new Error('index fail'));

    await useDashboardStore.getState().loadDashboard('dash-a');

    expect(useDashboardStore.getState().dashboardsById['dash-a']?.runs['widget-a']).toBeNull();
  });

  it('loadDashboard sets error when getDashboard fails', async () => {
    mockDashboardCommands.getDashboard.mockRejectedValue(new Error('not found'));

    await useDashboardStore.getState().loadDashboard('missing');

    const entry = useDashboardStore.getState().dashboardsById['missing'];
    expect(entry?.error).toBe('not found');
    expect(entry?.loading).toBe(false);
  });

  it('saveDashboard updates list and existing entry', async () => {
    const dash = makeDashboard('dash-a', 'widget-a');
    const saved = { ...dash, name: 'Renamed' };
    mockDashboardCommands.saveDashboard.mockResolvedValue(saved);

    useDashboardStore.setState({
      list: [dash],
      dashboardsById: {
        'dash-a': {
          dashboard: dash,
          runs: {},
          busyWidgets: {},
          refCount: 1,
        },
      },
    });

    const result = await useDashboardStore.getState().saveDashboard(saved);

    expect(result).toEqual(saved);
    expect(useDashboardStore.getState().list[0]?.name).toBe('Renamed');
    expect(useDashboardStore.getState().dashboardsById['dash-a']?.dashboard?.name).toBe('Renamed');
  });

  it('saveDashboard appends to list when dashboard is new', async () => {
    const dash = makeDashboard('dash-new', 'w1');
    mockDashboardCommands.saveDashboard.mockResolvedValue(dash);

    await useDashboardStore.getState().saveDashboard(dash);

    expect(useDashboardStore.getState().list).toHaveLength(1);
    expect(useDashboardStore.getState().list[0]?.id).toBe('dash-new');
  });

  it('deleteDashboard removes from list and dashboardsById', async () => {
    const dash = makeDashboard('dash-a', 'widget-a');
    useDashboardStore.setState({
      list: [dash],
      dashboardsById: {
        'dash-a': { dashboard: dash, runs: {}, busyWidgets: {}, refCount: 1 },
      },
    });
    mockDashboardCommands.deleteDashboard.mockResolvedValue(undefined);

    await useDashboardStore.getState().deleteDashboard('dash-a');

    expect(useDashboardStore.getState().list).toHaveLength(0);
    expect(useDashboardStore.getState().dashboardsById['dash-a']).toBeUndefined();
  });

  it('refreshWidget error path clears busy and sets error', async () => {
    useDashboardStore.setState({
      dashboardsById: {
        'dash-a': { dashboard: null, runs: {}, busyWidgets: {}, refCount: 1 },
      },
    });
    mockDashboardCommands.runDashboardWidget.mockRejectedValue(new Error('run failed'));

    await expect(useDashboardStore.getState().refreshWidget('dash-a', 'widget-a')).rejects.toThrow(
      'run failed',
    );

    const entry = useDashboardStore.getState().dashboardsById['dash-a'];
    expect(entry?.busyWidgets['widget-a']).toBeUndefined();
    expect(entry?.error).toBe('run failed');
  });

  it('refreshAllWidgets refreshes only enabled widgets', async () => {
    const dash = makeDashboard('dash-a', 'w1', {
      widgets: [
        {
          id: 'w1',
          title: 'Enabled',
          workflowId: 'wf-1',
          viewMode: 'chart',
          chartConfig: { ...DEFAULT_CHART_CONFIG, yAxes: ['v'] },
          layout: { x: 0, y: 0, w: 6, h: 4 },
          refresh: { mode: 'manual' },
          enabled: true,
        },
        {
          id: 'w2',
          title: 'Disabled',
          workflowId: 'wf-2',
          viewMode: 'chart',
          chartConfig: { ...DEFAULT_CHART_CONFIG, yAxes: ['v'] },
          layout: { x: 6, y: 0, w: 6, h: 4 },
          refresh: { mode: 'manual' },
          enabled: false,
        },
      ],
    });
    useDashboardStore.setState({
      dashboardsById: {
        'dash-a': { dashboard: dash, runs: {}, busyWidgets: {}, refCount: 1 },
      },
    });
    mockDashboardCommands.runDashboardWidget.mockImplementation(async (_d, widgetId) =>
      makeRun('dash-a', widgetId, `run-${widgetId}`),
    );

    await useDashboardStore.getState().refreshAllWidgets('dash-a');

    expect(mockDashboardCommands.runDashboardWidget).toHaveBeenCalledTimes(1);
    expect(mockDashboardCommands.runDashboardWidget).toHaveBeenCalledWith('dash-a', 'w1');
  });

  it('setRun patches runs for a widget', () => {
    const run = makeRun('dash-a', 'widget-a', 'run-1');
    useDashboardStore.setState({
      dashboardsById: {
        'dash-a': { dashboard: null, runs: {}, busyWidgets: {}, refCount: 1 },
      },
    });

    useDashboardStore.getState().setRun('dash-a', 'widget-a', run);

    expect(useDashboardStore.getState().dashboardsById['dash-a']?.runs['widget-a']).toEqual(run);
  });

  it('cross-window run-updated updates run when dashboard is mounted', async () => {
    const dash = makeDashboard('dash-a', 'widget-a');
    useDashboardStore.setState({
      dashboardsById: {
        'dash-a': { dashboard: dash, runs: {}, busyWidgets: {}, refCount: 1 },
      },
    });

    useDashboardStore.getState().mountDashboard('dash-a');
    const handler = listenHandlers['dashboard:run-updated'];
    expect(handler).toBeTypeOf('function');

    const run = makeRun('dash-a', 'widget-a', 'run-remote');
    handler?.({ dashboardId: 'dash-a', widgetId: 'widget-a', run });

    expect(useDashboardStore.getState().dashboardsById['dash-a']?.runs['widget-a']).toEqual(run);
  });

  it('cross-window run-updated ignores updates when dashboard is not mounted', () => {
    const run = makeRun('dash-a', 'widget-a', 'run-remote');
    useDashboardStore.getState().mountDashboard('dash-a');
    useDashboardStore.getState().releaseDashboard('dash-a');

    const handler = listenHandlers['dashboard:run-updated'];
    handler?.({ dashboardId: 'dash-a', widgetId: 'widget-a', run });

    expect(useDashboardStore.getState().dashboardsById['dash-a']).toBeUndefined();
  });

  it('keeps runs isolated per dashboard id on refreshWidget', async () => {
    const dashA = makeDashboard('dash-a', 'widget-a');
    const dashB = makeDashboard('dash-b', 'widget-b');
    mockDashboardCommands.getDashboard.mockImplementation(async (id: string) =>
      id === 'dash-a' ? dashA : dashB,
    );
    mockDashboardCommands.listWidgetRuns.mockResolvedValue([]);
    mockDashboardCommands.runDashboardWidget.mockImplementation(
      async (dashboardId: string, widgetId: string) =>
        makeRun(dashboardId, widgetId, `run-${dashboardId}`),
    );

    const store = useDashboardStore.getState();
    store.mountDashboard('dash-a');
    store.mountDashboard('dash-b');
    await store.loadDashboard('dash-a');
    await store.loadDashboard('dash-b');

    await store.refreshWidget('dash-a', 'widget-a');
    await store.refreshWidget('dash-b', 'widget-b');

    const entryA = useDashboardStore.getState().dashboardsById['dash-a'];
    const entryB = useDashboardStore.getState().dashboardsById['dash-b'];
    expect(entryA?.runs['widget-a']?.id).toBe('run-dash-a');
    expect(entryB?.runs['widget-b']?.id).toBe('run-dash-b');
  });
});
