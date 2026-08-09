import { create } from 'zustand';
import { dashboardCommands } from '../commands/dashboard';
import type { Dashboard, WidgetRun } from '../types/dashboard';

interface DashboardStore {
  dashboards: Dashboard[];
  current: Dashboard | null;
  /** Latest run per widget id for the active dashboard. */
  runs: Record<string, WidgetRun | null>;
  /** Widget ids currently refreshing. */
  busyWidgets: Set<string>;
  loading: boolean;
  error: string | null;

  fetchDashboards: () => Promise<void>;
  loadDashboard: (id: string) => Promise<void>;
  saveDashboard: (dashboard: Dashboard) => Promise<Dashboard>;
  deleteDashboard: (id: string) => Promise<void>;
  refreshWidget: (dashboardId: string, widgetId: string) => Promise<WidgetRun>;
  refreshAllWidgets: (dashboardId: string) => Promise<void>;
  setRun: (widgetId: string, run: WidgetRun | null) => void;
  clearCurrent: () => void;
}

async function loadLatestRun(
  dashboardId: string,
  widgetId: string,
): Promise<WidgetRun | null> {
  const index = await dashboardCommands.listWidgetRuns(dashboardId, widgetId, 1);
  const latest = index.find((e) => e.status === 'ok') ?? index[0];
  if (!latest) return null;
  return dashboardCommands.getWidgetRun(dashboardId, widgetId, latest.id);
}

export const useDashboardStore = create<DashboardStore>((set, get) => ({
  dashboards: [],
  current: null,
  runs: {},
  busyWidgets: new Set(),
  loading: false,
  error: null,

  fetchDashboards: async () => {
    set({ loading: true, error: null });
    try {
      const dashboards = await dashboardCommands.listDashboards();
      set({ dashboards, loading: false });
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : String(e),
      });
    }
  },

  loadDashboard: async (id: string) => {
    set({ loading: true, error: null, current: null, runs: {} });
    try {
      const dashboard = await dashboardCommands.getDashboard(id);
      const runs: Record<string, WidgetRun | null> = {};
      await Promise.all(
        dashboard.widgets.map(async (w) => {
          try {
            runs[w.id] = await loadLatestRun(id, w.id);
          } catch {
            runs[w.id] = null;
          }
        }),
      );
      set({ current: dashboard, runs, loading: false });
    } catch (e) {
      set({
        loading: false,
        error: e instanceof Error ? e.message : String(e),
      });
    }
  },

  saveDashboard: async (dashboard: Dashboard) => {
    const saved = await dashboardCommands.saveDashboard({
      ...dashboard,
      updatedAt: new Date().toISOString(),
    });
    set((s) => ({
      current: s.current?.id === saved.id ? saved : s.current,
      dashboards: s.dashboards.some((d) => d.id === saved.id)
        ? s.dashboards.map((d) => (d.id === saved.id ? saved : d))
        : [...s.dashboards, saved],
    }));
    return saved;
  },

  deleteDashboard: async (id: string) => {
    await dashboardCommands.deleteDashboard(id);
    set((s) => ({
      dashboards: s.dashboards.filter((d) => d.id !== id),
      current: s.current?.id === id ? null : s.current,
    }));
  },

  refreshWidget: async (dashboardId: string, widgetId: string) => {
    set((s) => ({
      busyWidgets: new Set([...s.busyWidgets, widgetId]),
    }));
    try {
      const run = await dashboardCommands.runDashboardWidget(dashboardId, widgetId);
      set((s) => ({
        runs: { ...s.runs, [widgetId]: run },
        busyWidgets: new Set([...s.busyWidgets].filter((id) => id !== widgetId)),
      }));
      return run;
    } catch (e) {
      set((s) => ({
        busyWidgets: new Set([...s.busyWidgets].filter((id) => id !== widgetId)),
        error: e instanceof Error ? e.message : String(e),
      }));
      throw e;
    }
  },

  refreshAllWidgets: async (dashboardId: string) => {
    const { current } = get();
    if (!current || current.id !== dashboardId) return;
    await Promise.all(
      current.widgets.filter((w) => w.enabled).map((w) => get().refreshWidget(dashboardId, w.id)),
    );
  },

  setRun: (widgetId, run) => {
    set((s) => ({ runs: { ...s.runs, [widgetId]: run } }));
  },

  clearCurrent: () => {
    set({ current: null, runs: {}, error: null });
  },
}));
