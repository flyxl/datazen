import type { ChartConfig } from './chart';

export type { ChartConfig };

export interface DashboardLayout {
  cols: number;
  rowHeight: number;
}

export interface WidgetLayout {
  x: number;
  y: number;
  w: number;
  h: number;
}

export type ViewMode = 'chart' | 'table';

export type RefreshMode = 'manual' | 'onOpen' | 'interval';

export interface RefreshPolicy {
  mode: RefreshMode;
  refreshSec?: number;
}

export type AlertMetricKind = 'column' | 'aggregation';

export type AlertMetricAgg = 'last' | 'max' | 'min' | 'avg' | 'sum';

export interface AlertMetric {
  kind: AlertMetricKind;
  column: string;
  agg?: AlertMetricAgg;
}

export type AlertOperator = '>' | '>=' | '<' | '<=' | '==' | '!=';

export type AlertChannel = 'desktop' | 'webhook' | 'email';

export interface AlertRule {
  metric: AlertMetric;
  op: AlertOperator;
  threshold: number;
  cooldownSec: number;
  channels: AlertChannel[];
}

export interface DashboardWidget {
  id: string;
  title: string;
  workflowId: string;
  viewMode: ViewMode;
  chartConfig?: ChartConfig;
  layout: WidgetLayout;
  refresh: RefreshPolicy;
  alert?: AlertRule;
  enabled: boolean;
}

export interface Dashboard {
  id: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  layout: DashboardLayout;
  widgets: DashboardWidget[];
  enabled: boolean;
  refreshPaused?: boolean;
}

export type WidgetRunStatus = 'ok' | 'error' | 'timeout';

export interface RunIndexEntry {
  id: string;
  startedAt: string;
  status: WidgetRunStatus;
  alertFired?: boolean;
}

export interface WidgetRun {
  id: string;
  dashboardId: string;
  widgetId: string;
  workflowId: string;
  startedAt: string;
  finishedAt: string;
  status: WidgetRunStatus;
  error?: string;
  rowCount: number;
  columns: string[];
  rows: unknown[][];
  alertFired?: boolean;
  alertValue?: number;
}

export interface DashboardWorkflowRef {
  workflowId: string;
  dashboardId: string;
  widgetId: string;
  dashboardName: string;
  widgetTitle: string;
}

export interface CreateWidgetResult {
  dashboard: Dashboard;
  widget: DashboardWidget;
}

/** SMTP settings reserved for phase 2. */
export interface MonitorEmailSettings {
  enabled?: boolean;
  host?: string;
  port?: number;
  username?: string;
  password?: string;
  from?: string;
  to?: string[];
}

export interface MonitorSettings {
  trayEnabled: boolean;
  closeToTray: boolean;
  defaultWebhookUrl?: string;
  email?: MonitorEmailSettings;
  maxConcurrentQueries: number;
  exportIncludeDashboardRuns: boolean;
  runRetentionCount: number;
  runRetentionDays: number;
}

export const MIN_REFRESH_SEC = 30;
export const REFRESH_WARN_BELOW_SEC = 60;

export function clampRefreshSec(n: number): number {
  return Math.max(MIN_REFRESH_SEC, n);
}

export function normalizeRefreshPolicy(refresh: RefreshPolicy): RefreshPolicy {
  if (refresh.mode === 'interval') {
    return {
      mode: 'interval',
      refreshSec: clampRefreshSec(refresh.refreshSec ?? MIN_REFRESH_SEC),
    };
  }
  return { mode: refresh.mode };
}

export function shouldWarnRefreshSec(refresh: RefreshPolicy): boolean {
  return (
    refresh.mode === 'interval' &&
    refresh.refreshSec != null &&
    refresh.refreshSec < REFRESH_WARN_BELOW_SEC
  );
}

export const DEFAULT_MONITOR_SETTINGS: MonitorSettings = {
  trayEnabled: true,
  closeToTray: true,
  maxConcurrentQueries: 2,
  exportIncludeDashboardRuns: true,
  runRetentionCount: 200,
  runRetentionDays: 30,
};

export const DEFAULT_REFRESH: RefreshPolicy = { mode: 'manual' };
