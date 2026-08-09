import { describe, expect, it, vi, beforeEach } from 'vitest';
import type { Dashboard, WidgetRun } from '../../types/dashboard';
import { DEFAULT_CHART_CONFIG } from '../../types/chart';

const mockDashboardCommands = {
  listDashboards: vi.fn(),
  getDashboard: vi.fn(),
  saveDashboard: vi.fn(),
  deleteDashboard: vi.fn(),
  listWidgetRuns: vi.fn().mockResolvedValue([]),
  getWidgetRun: vi.fn(),
  runDashboardWidget: vi.fn(),
};

vi.mock('../../commands/dashboard', () => ({
  dashboardCommands: mockDashboardCommands,
}));

function makeDashboard(id: string, widgetId: string): Dashboard {
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
        configId: 'conn-1',
        sql: 'SELECT 1 AS v',
        chartConfig: { ...DEFAULT_CHART_CONFIG, yAxes: ['v'] },
        layout: { x: 0, y: 0, w: 6, h: 4 },
        refreshSec: 60,
        enabled: true,
      },
    ],
  };
}

function makeRun(dashboardId: string, widgetId: string, id: string): WidgetRun {
  return {
    id,
    dashboardId,
    widgetId,
    startedAt: '2026-01-01T00:00:00Z',
    finishedAt: '2026-01-01T00:00:01Z',
    status: 'ok',
    rowCount: 1,
    columns: ['v'],
    rows: [[1]],
  };
}

describe('dashboardStore multi-id isolation', () => {
  let useDashboardStore: typeof import('../dashboardStore').useDashboardStore;

  beforeEach(async () => {
    vi.resetModules();
    vi.clearAllMocks();
    const mod = await import('../dashboardStore');
    useDashboardStore = mod.useDashboardStore;
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

    const state = useDashboardStore.getState();
    expect(state.dashboardsById['dash-a']?.runs['widget-a']?.id).toBe('run-dash-a');
    expect(state.dashboardsById['dash-b']?.runs['widget-a']).toBeUndefined();
  });

  it('releaseDashboard removes entry only when refCount reaches zero', () => {
    const store = useDashboardStore.getState();
    store.mountDashboard('dash-1');
    store.mountDashboard('dash-1');

    expect(useDashboardStore.getState().dashboardsById['dash-1']?.refCount).toBe(2);

    store.releaseDashboard('dash-1');
    expect(useDashboardStore.getState().dashboardsById['dash-1']?.refCount).toBe(1);

    store.releaseDashboard('dash-1');
    expect(useDashboardStore.getState().dashboardsById['dash-1']).toBeUndefined();
  });

  it('fetchDashboards uses listLoading/list without touching per-dashboard state', async () => {
    mockDashboardCommands.listDashboards.mockResolvedValue([
      makeDashboard('dash-list', 'w1'),
    ]);

    const store = useDashboardStore.getState();
    store.mountDashboard('dash-open');
    useDashboardStore.setState({
      dashboardsById: {
        'dash-open': {
          dashboard: makeDashboard('dash-open', 'w-open'),
          runs: {},
          busyWidgets: {},
          refCount: 1,
        },
      },
    });

    await store.fetchDashboards();

    const state = useDashboardStore.getState();
    expect(state.list).toHaveLength(1);
    expect(state.listLoading).toBe(false);
    expect(state.dashboardsById['dash-open']).toBeDefined();
  });
});
