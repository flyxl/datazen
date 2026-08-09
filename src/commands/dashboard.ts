import { invoke } from '@tauri-apps/api/core';
import type { Dashboard, RunIndexEntry, WidgetRun } from '../types/dashboard';

export const dashboardCommands = {
  listDashboards: () => invoke<Dashboard[]>('list_dashboards'),

  getDashboard: (id: string) => invoke<Dashboard>('get_dashboard', { id }),

  saveDashboard: (dashboard: Dashboard) =>
    invoke<Dashboard>('save_dashboard', { dashboard }),

  deleteDashboard: (id: string) => invoke<void>('delete_dashboard', { id }),

  listWidgetRuns: (dashboardId: string, widgetId: string, limit: number) =>
    invoke<RunIndexEntry[]>('list_widget_runs', { dashboardId, widgetId, limit }),

  getWidgetRun: (dashboardId: string, widgetId: string, runId: string) =>
    invoke<WidgetRun>('get_widget_run', { dashboardId, widgetId, runId }),

  runDashboardWidget: (dashboardId: string, widgetId: string) =>
    invoke<WidgetRun>('run_dashboard_widget', { dashboardId, widgetId }),
};
