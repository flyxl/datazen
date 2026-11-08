/**
 * Dashboard 类型（W8 刀3d web 收口：wire 词汇单一来源上移
 * `@datazen/backend-client` 的 `types/dashboard.ts`，此处保留既有 import
 * 路径；本地保留的是**显示态**词汇与运行时常量，不是 wire 词汇）。
 *
 * 分界裁决：`ChartConfig` 的 `timeDomain` / `timeTicks` 是渲染期显示扩展
 * （Rust 结构没有、serde 落库即丢），单一来源在 `./chart`（显示版）；
 * backend-client 的 `ChartConfig` 是 wire 版（无显示扩展）。两者结构互容
 * （显示扩展全部可选），但直接换源会让「往 config 上写 timeDomain」的
 * 消费点断链——所以 `Dashboard` / `DashboardWidget` 在本地以显示版
 * `ChartConfig` 覆写，其余 wire 类型原样再导出。
 */

import type { ChartConfig } from './chart';

export type { ChartConfig };

import type {
  Dashboard as WireDashboard,
  DashboardWidget as WireDashboardWidget,
  MonitorGrant,
  RefreshPolicy,
} from '@datazen/backend-client';

export type {
  AlertChannel,
  AlertMetric,
  AlertMetricAgg,
  AlertMetricKind,
  AlertOperator,
  AlertRule,
  CreateWidgetFromSqlParams,
  CreateWidgetFromWorkflowParams,
  DashboardLayout,
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

/** 显示态覆写：wire widget 的 chartConfig 落到显示版 ChartConfig（可选语义与 wire 一致）。 */
export interface DashboardWidget extends Omit<WireDashboardWidget, 'chartConfig'> {
  chartConfig?: ChartConfig;
}

/** 显示态覆写：widgets 数组元素带显示版 chartConfig。 */
export interface Dashboard extends Omit<WireDashboard, 'widgets'> {
  widgets: DashboardWidget[];
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

/**
 * 旧名兼容：前端的「授权单摘要」读的是完整 `MonitorGrant` 的子集
 * （backend-client 按契约名提供完整形状，四个旧读字段全在）。
 */
export type MonitorScheduleGrantSummary = MonitorGrant;
