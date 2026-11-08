import { legacyInvoke as invoke, useBackendClient } from '@datazen/backend-client';
import type {
  CreateWidgetFromSqlParams,
  CreateWidgetFromWorkflowParams,
  Dashboard,
  MonitorGrant,
  UpdateHiddenWidgetSqlParams,
} from '@datazen/backend-client';

/**
 * Dashboard commands (W8 刀3d web 收口)。
 *
 * 15 个 legacyInvoke 已换轨 `@datazen/backend-client` 的 dashboard 域客户端
 * （契约 §6.15）；带体写面经 userAssetToken 铸令牌（saveUserAsset 操作族）。
 * 类型与 wire 归一化的单一来源在 backend-client 的 `types/dashboard.ts`。
 *
 * `exportWithDialog` / `importWithDialog` 两命令**故意留在 legacy 轨**：
 * 桌面文件对话框语义不上 HTTP 面（浏览器侧导出/导入走产物上传与下载，
 * 见契约 §6.15 域裁决）。
 */

// ── 域词汇转出（单一来源在 backend-client；UI 既有 import 路径不变）───────
export type {
  AggregationType,
  AlertChannel,
  AlertMetric,
  AlertMetricAgg,
  AlertMetricKind,
  AlertOperator,
  AlertRule,
  ChartConfig,
  ChartSortBy,
  ChartType,
  CreateWidgetFromSqlParams,
  CreateWidgetFromWorkflowParams,
  CreateWidgetResult,
  Dashboard,
  DashboardLayout,
  DashboardWidget,
  DashboardWorkflowRef,
  MonitorGrant,
  MonitorScope,
  RefreshMode,
  RefreshPolicy,
  RunIndexEntry,
  UpdateHiddenWidgetSqlParams,
  ViewMode,
  WidgetLayout,
  WidgetRun,
  WidgetRunStatus,
} from '@datazen/backend-client';

/**
 * 兼容别名：授权单摘要的旧 UI 名。域词汇按契约叫 `MonitorGrant`（完整
 * 授权单——旧读字段 id / expiresAt / revoked / workflowRevision 全在其中）。
 */
export type MonitorScheduleGrantSummary = MonitorGrant;

/** 用户资产写面令牌（契约 §6.15：saveUserAsset 操作族，不绑会话）。 */
async function userAssetToken() {
  return useBackendClient().issueSubmissionToken('saveUserAsset', null);
}

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
    useBackendClient().getMonitorScheduleGrant({ dashboardId, widgetId }),

  authorizeMonitorSchedule: async (dashboardId: string, widgetId: string) =>
    useBackendClient().authorizeMonitorSchedule(
      { dashboardId, widgetId, ttlSecs: 3600 },
      await userAssetToken(),
    ),

  revokeMonitorSchedule: (dashboardId: string, widgetId: string) =>
    useBackendClient().revokeMonitorSchedule({ dashboardId, widgetId }),

  listDashboards: () => useBackendClient().listDashboards(),

  getDashboard: (id: string) => useBackendClient().getDashboard({ id }),

  saveDashboard: async (dashboard: Dashboard) =>
    useBackendClient().saveDashboard({ dashboard }, await userAssetToken()),

  deleteDashboard: (id: string) => useBackendClient().deleteDashboard({ id }),

  setDashboardRefreshPaused: async (id: string, paused: boolean) =>
    useBackendClient().setDashboardRefreshPaused({ id, paused }, await userAssetToken()),

  findDashboardWorkflowRefs: (workflowId: string) =>
    useBackendClient().findDashboardWorkflowRefs({ workflowId }),

  listWidgetRuns: (dashboardId: string, widgetId: string, limit: number) =>
    useBackendClient().listWidgetRuns({ dashboardId, widgetId, limit }),

  getWidgetRun: (dashboardId: string, widgetId: string, runId: string) =>
    useBackendClient().getWidgetRun({ dashboardId, widgetId, runId }),

  runDashboardWidget: async (dashboardId: string, widgetId: string) =>
    useBackendClient().runDashboardWidget({ dashboardId, widgetId }, await userAssetToken()),

  createWidgetFromSql: async (params: CreateWidgetFromSqlParams) => {
    const result = await useBackendClient().createWidgetFromSql(
      { dashboardId: params.dashboardId, params },
      await userAssetToken(),
    );
    runDashboardWidgetsOnce(result.dashboard);
    return result;
  },

  createWidgetFromWorkflow: async (params: CreateWidgetFromWorkflowParams) => {
    const result = await useBackendClient().createWidgetFromWorkflow(
      { dashboardId: params.dashboardId, params },
      await userAssetToken(),
    );
    runDashboardWidgetsOnce(result.dashboard);
    return result;
  },

  updateHiddenWidgetSql: async (params: UpdateHiddenWidgetSqlParams) =>
    useBackendClient().updateHiddenWidgetSql({ params }, await userAssetToken()),

  exportWithDialog: (dashboardId: string, defaultFileName: string) =>
    invoke<boolean>('export_dashboard_with_dialog', { dashboardId, defaultFileName }),

  importWithDialog: async () => {
    const dashboard = await invoke<Dashboard | null>('import_dashboard_with_dialog');
    if (dashboard) runDashboardWidgetsOnce(dashboard);
    return dashboard;
  },
};
