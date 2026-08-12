import { invoke } from '@tauri-apps/api/core';
import type {
  ChartConfig,
  CreateWidgetResult,
  Dashboard,
  DashboardWorkflowRef,
  RefreshPolicy,
  RunIndexEntry,
  ViewMode,
  WidgetRun,
} from '../types/dashboard';

export const dashboardCommands = {
  listDashboards: () => invoke<Dashboard[]>('list_dashboards'),

  getDashboard: (id: string) => invoke<Dashboard>('get_dashboard', { id }),

  saveDashboard: (dashboard: Dashboard) => invoke<Dashboard>('save_dashboard', { dashboard }),

  deleteDashboard: (id: string) => invoke<void>('delete_dashboard', { id }),

  setDashboardRefreshPaused: (id: string, paused: boolean) =>
    invoke<void>('set_dashboard_refresh_paused', { id, paused }),

  findDashboardWorkflowRefs: (workflowId: string) =>
    invoke<DashboardWorkflowRef[]>('find_dashboard_workflow_refs', { workflowId }),

  listWidgetRuns: (dashboardId: string, widgetId: string, limit: number) =>
    invoke<RunIndexEntry[]>('list_widget_runs', { dashboardId, widgetId, limit }),

  getWidgetRun: (dashboardId: string, widgetId: string, runId: string) =>
    invoke<WidgetRun>('get_widget_run', { dashboardId, widgetId, runId }),

  runDashboardWidget: (dashboardId: string, widgetId: string) =>
    invoke<WidgetRun>('run_dashboard_widget', { dashboardId, widgetId }),

  createWidgetFromSql: (params: {
    dashboardId: string;
    configId: string;
    sql: string;
    title?: string;
    viewMode: ViewMode;
    chartConfig?: ChartConfig;
  }) => invoke<CreateWidgetResult>('create_widget_from_sql', { params }),

  createWidgetFromWorkflow: (params: {
    dashboardId: string;
    workflowId: string;
    title?: string;
    viewMode: ViewMode;
    chartConfig?: ChartConfig;
  }) => invoke<CreateWidgetResult>('create_widget_from_workflow', { params }),

  exportWithDialog: (dashboardId: string, defaultFileName: string) =>
    invoke<boolean>('export_dashboard_with_dialog', { dashboardId, defaultFileName }),

  importWithDialog: () => invoke<Dashboard | null>('import_dashboard_with_dialog'),

  /** @deprecated Use setDashboardRefreshPaused */
  getMonitorPaused: () => invoke<boolean>('get_monitor_paused'),

  /** @deprecated Use setDashboardRefreshPaused */
  setMonitorPaused: (paused: boolean) => invoke<void>('set_monitor_paused', { paused }),
};
