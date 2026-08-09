import type { ChartConfig } from './chart';

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
  configId: string;
  sql: string;
  chartConfig: ChartConfig;
  layout: WidgetLayout;
  refreshSec: number;
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
}

export type WidgetRunStatus = 'ok' | 'error' | 'timeout';

export interface WidgetRun {
  id: string;
  dashboardId: string;
  widgetId: string;
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

export function clampRefreshSec(n: number): number {
  return Math.max(MIN_REFRESH_SEC, n);
}

export const DEFAULT_MONITOR_SETTINGS: MonitorSettings = {
  trayEnabled: true,
  closeToTray: true,
  maxConcurrentQueries: 2,
  exportIncludeDashboardRuns: true,
  runRetentionCount: 200,
  runRetentionDays: 30,
};
