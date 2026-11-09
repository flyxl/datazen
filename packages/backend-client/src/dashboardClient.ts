/**
 * The Dashboard domain client (contract §6.15, W8 刀3d): dashboard CRUD,
 * widget creation, run history, manual widget execution and the monitor
 * schedule grant faces.
 *
 * Three desktop commands are deliberately NOT methods here:
 * - `export_dashboard_with_dialog` / `import_dashboard_with_dialog` — file
 *   dialog semantics owned by the host, never on the HTTP surface (the
 *   browser form rides the §6.5 artifact upload endpoint and downloads);
 *   they keep riding the legacy rail.
 * - `webdriver_monitor_evidence` — a test-only projection; it is not a
 *   product capability and does not belong on a client surface.
 *
 * Envelope notes (the queryAssets/schemaDiff precedents, restated for this
 * domain):
 * - The domain request is the desktop IPC argument bag plus a top-level
 *   `submissionToken` on write faces; the desktop transport strips the
 *   token before invoking, the HTTP rail flattens what its route needs.
 *   `saveDashboard` therefore carries `{ dashboard, submissionToken }` —
 *   the contract row's `Dashboard & SubmitRequest` is the PUT *body* shape
 *   (dashboard flattened + token), same split as `saveSchemaDiffProfile`.
 * - The `createWidgetFrom*` requests carry the desktop `params` bag
 *   verbatim (it includes `dashboardId`, which the Tauri command needs)
 *   plus a top-level `dashboardId` the HTTP rail interpolates into the
 *   route path; the path segment and the bag field are the same fact.
 * - `runDashboardWidget`: the HTTP body may be absent server-side, but the
 *   domain request carries the token (the acceptance face checks the
 *   binding when a body is present — and the manual run is a write).
 * - DELETE faces (`deleteDashboard` / `revokeMonitorSchedule`) carry no
 *   token, matching the delete/clear convention across converged domains.
 * - `saveDashboard`'s answer is the post-save re-read (normalized,
 *   server-stamped), not an echo of the caller's draft.
 */

import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import type {
  CreateWidgetFromSqlParams,
  CreateWidgetFromWorkflowParams,
  CreateWidgetResult,
  Dashboard,
  DashboardWorkflowRef,
  MonitorGrant,
  RunIndexEntry,
  UpdateHiddenWidgetSqlParams,
  WidgetRun,
} from './types/dashboard';
import type { SubmissionToken, SubmitRequest } from './types/session';

/** The Dashboard slice of `MethodMap`. */
export interface DashboardMethodMap {
  listDashboards: { request: Record<string, never>; response: Dashboard[] };
  getDashboard: { request: { id: string }; response: Dashboard };
  saveDashboard: {
    request: { dashboard: Dashboard } & SubmitRequest;
    response: Dashboard;
  };
  deleteDashboard: { request: { id: string }; response: void };
  setDashboardRefreshPaused: {
    request: { id: string; paused: boolean } & SubmitRequest;
    response: void;
  };
  findDashboardWorkflowRefs: {
    request: { workflowId: string };
    response: DashboardWorkflowRef[];
  };
  createWidgetFromSql: {
    request: { dashboardId: string; params: CreateWidgetFromSqlParams } & SubmitRequest;
    response: CreateWidgetResult;
  };
  createWidgetFromWorkflow: {
    request: { dashboardId: string; params: CreateWidgetFromWorkflowParams } & SubmitRequest;
    response: CreateWidgetResult;
  };
  updateHiddenWidgetSql: {
    request: { params: UpdateHiddenWidgetSqlParams } & SubmitRequest;
    response: void;
  };
  runDashboardWidget: {
    request: { dashboardId: string; widgetId: string } & SubmitRequest;
    response: WidgetRun;
  };
  /**
   * `limit` is optional on the server (defaults to 20); the desktop IPC
   * command still requires it, so desktop callers pass it explicitly.
   */
  listWidgetRuns: {
    request: { dashboardId: string; widgetId: string; limit?: number };
    response: RunIndexEntry[];
  };
  getWidgetRun: {
    request: { dashboardId: string; widgetId: string; runId: string };
    response: WidgetRun;
  };
  getMonitorScheduleGrant: {
    request: { dashboardId: string; widgetId: string };
    response: MonitorGrant | null;
  };
  authorizeMonitorSchedule: {
    request: { dashboardId: string; widgetId: string; ttlSecs: number } & SubmitRequest;
    response: MonitorGrant;
  };
  revokeMonitorSchedule: {
    request: { dashboardId: string; widgetId: string };
    response: void;
  };
}

/** The Dashboard domain client surface. */
export interface DashboardClient {
  listDashboards(): Promise<Dashboard[]>;
  getDashboard(request: { id: string }): Promise<Dashboard>;
  saveDashboard(request: { dashboard: Dashboard }, token: SubmissionToken): Promise<Dashboard>;
  deleteDashboard(request: { id: string }): Promise<void>;
  setDashboardRefreshPaused(
    request: { id: string; paused: boolean },
    token: SubmissionToken,
  ): Promise<void>;
  findDashboardWorkflowRefs(request: { workflowId: string }): Promise<DashboardWorkflowRef[]>;
  createWidgetFromSql(
    request: { dashboardId: string; params: CreateWidgetFromSqlParams },
    token: SubmissionToken,
  ): Promise<CreateWidgetResult>;
  createWidgetFromWorkflow(
    request: { dashboardId: string; params: CreateWidgetFromWorkflowParams },
    token: SubmissionToken,
  ): Promise<CreateWidgetResult>;
  updateHiddenWidgetSql(
    request: { params: UpdateHiddenWidgetSqlParams },
    token: SubmissionToken,
  ): Promise<void>;
  runDashboardWidget(
    request: { dashboardId: string; widgetId: string },
    token: SubmissionToken,
  ): Promise<WidgetRun>;
  listWidgetRuns(request: {
    dashboardId: string;
    widgetId: string;
    limit?: number;
  }): Promise<RunIndexEntry[]>;
  getWidgetRun(request: {
    dashboardId: string;
    widgetId: string;
    runId: string;
  }): Promise<WidgetRun>;
  getMonitorScheduleGrant(request: {
    dashboardId: string;
    widgetId: string;
  }): Promise<MonitorGrant | null>;
  authorizeMonitorSchedule(
    request: { dashboardId: string; widgetId: string; ttlSecs: number },
    token: SubmissionToken,
  ): Promise<MonitorGrant>;
  revokeMonitorSchedule(request: { dashboardId: string; widgetId: string }): Promise<void>;
}

/** Build the Dashboard domain client over a transport. */
export function createDashboardMethods(
  transport: BackendTransport,
  guard: BackendGuard,
): DashboardClient {
  return {
    listDashboards: () => guard('listDashboards', () => transport.call('listDashboards', {})),

    getDashboard: (request) => guard('getDashboard', () => transport.call('getDashboard', request)),

    saveDashboard: (request, submissionToken) =>
      guard('saveDashboard', () =>
        transport.call('saveDashboard', { ...request, submissionToken }),
      ),

    deleteDashboard: (request) =>
      guard('deleteDashboard', () => transport.call('deleteDashboard', request)),

    setDashboardRefreshPaused: (request, submissionToken) =>
      guard('setDashboardRefreshPaused', () =>
        transport.call('setDashboardRefreshPaused', { ...request, submissionToken }),
      ),

    findDashboardWorkflowRefs: (request) =>
      guard('findDashboardWorkflowRefs', () =>
        transport.call('findDashboardWorkflowRefs', request),
      ),

    createWidgetFromSql: (request, submissionToken) =>
      guard('createWidgetFromSql', () =>
        transport.call('createWidgetFromSql', { ...request, submissionToken }),
      ),

    createWidgetFromWorkflow: (request, submissionToken) =>
      guard('createWidgetFromWorkflow', () =>
        transport.call('createWidgetFromWorkflow', { ...request, submissionToken }),
      ),

    updateHiddenWidgetSql: (request, submissionToken) =>
      guard('updateHiddenWidgetSql', () =>
        transport.call('updateHiddenWidgetSql', { ...request, submissionToken }),
      ),

    runDashboardWidget: (request, submissionToken) =>
      guard('runDashboardWidget', () =>
        transport.call('runDashboardWidget', { ...request, submissionToken }),
      ),

    listWidgetRuns: (request) =>
      guard('listWidgetRuns', () => transport.call('listWidgetRuns', request)),

    getWidgetRun: (request) => guard('getWidgetRun', () => transport.call('getWidgetRun', request)),

    getMonitorScheduleGrant: (request) =>
      guard('getMonitorScheduleGrant', () => transport.call('getMonitorScheduleGrant', request)),

    authorizeMonitorSchedule: (request, submissionToken) =>
      guard('authorizeMonitorSchedule', () =>
        transport.call('authorizeMonitorSchedule', { ...request, submissionToken }),
      ),

    revokeMonitorSchedule: (request) =>
      guard('revokeMonitorSchedule', () => transport.call('revokeMonitorSchedule', request)),
  };
}
