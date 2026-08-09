import { create } from 'zustand';
import { dashboardCommands } from '../commands/dashboard';
import type { Dashboard, WidgetRun } from '../types/dashboard';

export interface DashboardEntry {
  dashboard: Dashboard | null;
  runs: Record<string, WidgetRun | null>;
  busyWidgets: Record<string, boolean>;
  loading?: boolean;
  error?: string | null;
  refCount: number;
}

interface DashboardStore {
  dashboardsById: Record<string, DashboardEntry>;
  list: Dashboard[];
  listError?: string | null;
  listLoading?: boolean;

  fetchDashboards: () => Promise<void>;
  mountDashboard: (id: string) => void;
  releaseDashboard: (id: string) => void;
  loadDashboard: (id: string) => Promise<void>;
  saveDashboard: (dashboard: Dashboard) => Promise<Dashboard>;
  deleteDashboard: (id: string) => Promise<void>;
  refreshWidget: (dashboardId: string, widgetId: string) => Promise<WidgetRun>;
  refreshAllWidgets: (dashboardId: string) => Promise<void>;
  setRun: (dashboardId: string, widgetId: string, run: WidgetRun | null) => void;
}

function emptyEntry(): DashboardEntry {
  return {
    dashboard: null,
    runs: {},
    busyWidgets: {},
    loading: false,
    error: null,
    refCount: 0,
  };
}

function patchEntry(
  dashboardsById: Record<string, DashboardEntry>,
  id: string,
  patch: Partial<DashboardEntry>,
): Record<string, DashboardEntry> {
  const prev = dashboardsById[id] ?? emptyEntry();
  return { ...dashboardsById, [id]: { ...prev, ...patch } };
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
  dashboardsById: {},
  list: [],
  listError: null,
  listLoading: false,

  fetchDashboards: async () => {
    set({ listLoading: true, listError: null });
    try {
      const list = await dashboardCommands.listDashboards();
      set({ list, listLoading: false });
    } catch (e) {
      set({
        listLoading: false,
        listError: e instanceof Error ? e.message : String(e),
      });
    }
  },

  mountDashboard: (id: string) => {
    set((s) => {
      const prev = s.dashboardsById[id] ?? emptyEntry();
      return {
        dashboardsById: {
          ...s.dashboardsById,
          [id]: { ...prev, refCount: prev.refCount + 1 },
        },
      };
    });
  },

  releaseDashboard: (id: string) => {
    set((s) => {
      const prev = s.dashboardsById[id];
      if (!prev) return s;
      const nextRef = prev.refCount - 1;
      if (nextRef <= 0) {
        const { [id]: _, ...rest } = s.dashboardsById;
        return { dashboardsById: rest };
      }
      return {
        dashboardsById: {
          ...s.dashboardsById,
          [id]: { ...prev, refCount: nextRef },
        },
      };
    });
  },

  loadDashboard: async (id: string) => {
    set((s) => ({
      dashboardsById: patchEntry(s.dashboardsById, id, { loading: true, error: null }),
    }));
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
      set((s) => ({
        dashboardsById: patchEntry(s.dashboardsById, id, {
          dashboard,
          runs,
          loading: false,
        }),
      }));
    } catch (e) {
      set((s) => ({
        dashboardsById: patchEntry(s.dashboardsById, id, {
          loading: false,
          error: e instanceof Error ? e.message : String(e),
        }),
      }));
    }
  },

  saveDashboard: async (dashboard: Dashboard) => {
    const saved = await dashboardCommands.saveDashboard({
      ...dashboard,
      updatedAt: new Date().toISOString(),
    });
    set((s) => {
      const hasEntry = s.dashboardsById[saved.id] != null;
      const dashboardsById = hasEntry
        ? patchEntry(s.dashboardsById, saved.id, { dashboard: saved })
        : s.dashboardsById;
      const list = s.list.some((d) => d.id === saved.id)
        ? s.list.map((d) => (d.id === saved.id ? saved : d))
        : [...s.list, saved];
      return { dashboardsById, list };
    });
    return saved;
  },

  deleteDashboard: async (id: string) => {
    await dashboardCommands.deleteDashboard(id);
    set((s) => {
      const { [id]: _, ...rest } = s.dashboardsById;
      return {
        dashboardsById: rest,
        list: s.list.filter((d) => d.id !== id),
      };
    });
  },

  refreshWidget: async (dashboardId: string, widgetId: string) => {
    set((s) => {
      const entry = s.dashboardsById[dashboardId] ?? emptyEntry();
      return {
        dashboardsById: {
          ...s.dashboardsById,
          [dashboardId]: {
            ...entry,
            busyWidgets: { ...entry.busyWidgets, [widgetId]: true },
          },
        },
      };
    });
    try {
      const run = await dashboardCommands.runDashboardWidget(dashboardId, widgetId);
      set((s) => {
        const entry = s.dashboardsById[dashboardId] ?? emptyEntry();
        const busyWidgets = { ...entry.busyWidgets };
        delete busyWidgets[widgetId];
        return {
          dashboardsById: {
            ...s.dashboardsById,
            [dashboardId]: {
              ...entry,
              runs: { ...entry.runs, [widgetId]: run },
              busyWidgets,
            },
          },
        };
      });
      return run;
    } catch (e) {
      set((s) => {
        const entry = s.dashboardsById[dashboardId] ?? emptyEntry();
        const busyWidgets = { ...entry.busyWidgets };
        delete busyWidgets[widgetId];
        return {
          dashboardsById: {
            ...s.dashboardsById,
            [dashboardId]: {
              ...entry,
              busyWidgets,
              error: e instanceof Error ? e.message : String(e),
            },
          },
        };
      });
      throw e;
    }
  },

  refreshAllWidgets: async (dashboardId: string) => {
    const entry = get().dashboardsById[dashboardId];
    if (!entry?.dashboard) return;
    await Promise.all(
      entry.dashboard.widgets
        .filter((w) => w.enabled)
        .map((w) => get().refreshWidget(dashboardId, w.id)),
    );
  },

  setRun: (dashboardId, widgetId, run) => {
    set((s) => {
      const entry = s.dashboardsById[dashboardId] ?? emptyEntry();
      return {
        dashboardsById: patchEntry(s.dashboardsById, dashboardId, {
          runs: { ...entry.runs, [widgetId]: run },
        }),
      };
    });
  },
}));
