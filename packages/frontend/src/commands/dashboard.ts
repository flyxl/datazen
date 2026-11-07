import { legacyInvoke as invoke } from '@datazen/backend-client';
import type {
  ChartConfig,
  MonitorScheduleGrantSummary,
  CreateWidgetResult,
  Dashboard,
  DashboardWorkflowRef,
  RunIndexEntry,
  ViewMode,
  WidgetRun,
} from '../types/dashboard';

function runDashboardWidgetsOnce(dashboard: Dashboard): void {
  void Promise.all(
    dashboard.widgets
      .filter((widget) => widget.enabled)
      .map((widget) =>
        dashboardCommands.runDashboardWidget(dashboard.id, widget.id).catch(() => undefined),
      ),
  );
}

export const dashboardCommands = {
  getMonitorScheduleGrant: (dashboardId: string, widgetId: string) =>
    invoke<MonitorScheduleGrantSummary | null>('get_monitor_widget_schedule_grant', {
      dashboardId,
      widgetId,
    }),
  authorizeMonitorSchedule: (dashboardId: string, widgetId: string) =>
    invoke<MonitorScheduleGrantSummary>('authorize_monitor_widget_schedule', {
      dashboardId,
      widgetId,
      ttlSecs: 3600,
    }),
  revokeMonitorSchedule: (dashboardId: string, widgetId: string) =>
    invoke<void>('revoke_monitor_widget_schedule', { dashboardId, widgetId }),
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

  createWidgetFromSql: async (params: {
    dashboardId: string;
    connectionId: string;
    sql: string;
    title?: string;
    viewMode: ViewMode;
    chartConfig?: ChartConfig;
  }) => {
    const result = await invoke<CreateWidgetResult>('create_widget_from_sql', { params });
    runDashboardWidgetsOnce(result.dashboard);
    return result;
  },

  createWidgetFromWorkflow: async (params: {
    dashboardId: string;
    workflowId: string;
    title?: string;
    viewMode: ViewMode;
    chartConfig?: ChartConfig;
  }) => {
    const result = await invoke<CreateWidgetResult>('create_widget_from_workflow', { params });
    runDashboardWidgetsOnce(result.dashboard);
    return result;
  },

  updateHiddenWidgetSql: (params: { workflowId: string; connectionId: string; sql: string }) =>
    invoke<void>('update_hidden_widget_sql', { params }),

  exportWithDialog: (dashboardId: string, defaultFileName: string) =>
    invoke<boolean>('export_dashboard_with_dialog', { dashboardId, defaultFileName }),

  importWithDialog: async () => {
    const dashboard = await invoke<Dashboard | null>('import_dashboard_with_dialog');
    if (dashboard) runDashboardWidgetsOnce(dashboard);
    return dashboard;
  },
};
