/**
 * Dashboard wire vocabulary (contract §6.15): dashboards, widgets, run
 * history and monitor schedule grants.
 *
 * The shapes are the shared Rust vocabulary — `datazen::dashboard::types`
 * (`Dashboard`, `DashboardWidget`, `WidgetRun`, `RunIndexEntry`,
 * `DashboardWorkflowRef`, `ChartConfig` and the view/refresh/alert enums)
 * and `datazen::monitor::domain` (`MonitorGrant`, `MonitorScope`) —
 * serialized camelCase. Both transports (the desktop IPC command rail and
 * the server HTTP surface) answer with this same shape, which is what lets
 * one client type serve both.
 *
 * What deliberately stays OUT of this file:
 * - `MonitorSettings` / `MonitorEmailSettings`: settings-domain vocabulary
 *   (the Rust side re-exports it from `settings::domain`); the dashboard
 *   HTTP surface never carries it.
 * - The refresh helpers (`MIN_REFRESH_SEC`, `clampRefreshSec`,
 *   `normalizeRefreshPolicy`, ...): UI-side derivations of the wire values,
 *   not wire facts.
 * - The frontend display extras on `ChartConfig` (`timeDomain` /
 *   `timeTicks` in `packages/frontend/src/types/chart.ts`): the Rust struct
 *   has no such fields, so serde drops them on the way into storage — they
 *   are render-time state, recomputed per paint, never persisted.
 */

import type { ExecutionTarget } from './session';

// ── chart vocabulary ────────────────────────────────────────────────────────

export type ChartType = 'bar' | 'line' | 'pie' | 'scatter' | 'area';

export type AggregationType = 'none' | 'sum' | 'avg' | 'count' | 'min' | 'max' | 'distinct_count';

export type ChartSortBy = 'x_asc' | 'x_desc' | 'y_asc' | 'y_desc' | 'none';

/** How a chart widget plots its run results (Rust `ChartConfig`, camelCase). */
export interface ChartConfig {
  chartType: ChartType;
  xAxis: string | null;
  yAxes: string[];
  groupBy: string | null;
  aggregation: AggregationType;
  sortBy: ChartSortBy;
  showLegend: boolean;
  showGrid: boolean;
  showValues: boolean;
  colorScheme: string;
}

// ── dashboards and widgets ──────────────────────────────────────────────────

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
  /** Present (and clamped server-side) only for `interval`. */
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

/**
 * Alert delivery channels. `app` is the host's own notification surface;
 * the pre-D6 wire spelling of `app` still deserializes (mapped, never
 * re-serialized), so persisted rules survive the rename.
 */
export type AlertChannel = 'app' | 'webhook' | 'email';

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
  /** Absent on hand-built drafts; the backend always writes it back. */
  refreshPaused?: boolean;
}

/** Where a workflow is referenced from (the workflow-delete gate's read). */
export interface DashboardWorkflowRef {
  workflowId: string;
  dashboardId: string;
  widgetId: string;
  dashboardName: string;
  widgetTitle: string;
}

// ── widget runs ─────────────────────────────────────────────────────────────

export type WidgetRunStatus = 'ok' | 'error' | 'timeout';

/** One entry of a widget's run history index. */
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

// ── widget creation ─────────────────────────────────────────────────────────

/**
 * `createWidgetFromSql`: mint a widget from a SQL statement (the backend
 * forges the hidden workflow it binds to). The `dashboardId` the widget is
 * created in rides the same bag on the desktop rail and the route path on
 * the HTTP rail.
 */
export interface CreateWidgetFromSqlParams {
  dashboardId: string;
  connectionId: string;
  sql: string;
  title?: string;
  viewMode: ViewMode;
  chartConfig?: ChartConfig;
}

/** `createWidgetFromWorkflow`: bind an existing workflow as a widget. */
export interface CreateWidgetFromWorkflowParams {
  dashboardId: string;
  workflowId: string;
  title?: string;
  viewMode: ViewMode;
  chartConfig?: ChartConfig;
}

/** `createWidgetFromSql` / `createWidgetFromWorkflow`: the stored dashboard re-read plus the new widget row. */
export interface CreateWidgetResult {
  dashboard: Dashboard;
  widget: DashboardWidget;
}

/**
 * `updateHiddenWidgetSql`: rewrite the SQL of the hidden workflow a widget
 * was minted from (keyed by workflowId; not dashboard-scoped).
 */
export interface UpdateHiddenWidgetSqlParams {
  workflowId: string;
  connectionId: string;
  sql: string;
}

// ── monitor schedule grant ──────────────────────────────────────────────────

/**
 * One frozen authorization for a widget's interval schedule (Rust
 * `monitor::domain::MonitorGrant`, camelCase). No bearer credentials: the
 * scopes are server-side facts, and execution rides a service identity
 * derived from the grant.
 *
 * The frontend's old UI name `MonitorScheduleGrantSummary` was a read
 * subset of this shape (`id` / `expiresAt` / `revoked` /
 * `workflowRevision` — all present here), so existing reads keep compiling
 * against the contract name.
 */
export interface MonitorGrant {
  id: string;
  dashboardId: string;
  widgetId: string;
  workflowId: string;
  /** SHA-256 of the workflow definition the grant froze. */
  workflowRevision: string;
  organizationId: string;
  servicePrincipal: string;
  approvedBy: string;
  /** The frozen, canonicalized targets the schedule may execute against. */
  scopes: MonitorScope[];
  actions: string[];
  readOnly: boolean;
  expiresAt: string;
  revocationVersion: number;
  revoked: boolean;
}

/** One frozen execution permission inside a {@link MonitorGrant}. */
export interface MonitorScope {
  target: ExecutionTarget;
  configRevision: number;
  command: string;
}
