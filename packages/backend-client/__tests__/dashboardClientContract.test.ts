import { describe, expect, it } from 'vitest';

import { createBackendClient, toId, toTimestamp, type MethodMap } from '../src/index';
import type { Dashboard, MonitorGrant, RunIndexEntry, WidgetRun } from '../src/types/dashboard';
import { createFakeTransport } from './client.test';

/**
 * The Dashboard domain contract table (§6.15, W8 刀3d): method × request
 * envelope × response passthrough, one row per domain method, through the
 * same table-driven runner the F5 per-domain tables use. The three
 * assertions per row are the facts that matter on the wire:
 * 1. the **method name** the transport receives (the MethodMap key);
 * 2. the **request envelope** (the desktop argument bag, token folded at
 *    the top level on write faces, `params` bags kept verbatim);
 * 3. the **response passthrough** (the facade adds nothing, drops nothing).
 *
 * `export_dashboard_with_dialog` / `import_dashboard_with_dialog`
 * (desktop file dialogs) and `webdriver_monitor_evidence` (test-only
 * projection) are deliberately not rows here — they are not domain methods.
 */

interface ContractRow {
  /** Facade method exercised. */
  call: (client: ReturnType<typeof createBackendClient>) => Promise<unknown>;
  /** Method name the transport must receive. */
  method: keyof MethodMap;
  /** The exact payload the transport must receive. */
  payload: unknown;
  /** Canned response served for `method` (also asserted as the resolution). */
  response: unknown;
}

function mustNarrow<T>(narrowed: T | undefined): T {
  if (narrowed === undefined) throw new Error('fixture did not narrow');
  return narrowed;
}

const token = {
  idempotencyKey: 'idem-dash-1',
  expiresAt: mustNarrow(toTimestamp(1_700_000_000_000)),
};

function runContractTable(domain: string, rows: ContractRow[]): void {
  describe(`${domain} domain contract table`, () => {
    for (const row of rows) {
      it(`${String(row.method)}: name, envelope and response are the contract`, async () => {
        const fake = createFakeTransport();
        fake.responses.set(row.method, row.response);
        const client = createBackendClient('desktop', fake.transport);

        await expect(row.call(client)).resolves.toEqual(row.response);
        expect(fake.calls).toHaveLength(1);
        expect(fake.calls[0]?.method).toBe(row.method);
        expect(fake.calls[0]?.payload).toEqual(row.payload);
      });
    }
  });
}

// ── fixtures (the wire vocabulary, one representative per shape) ────────────

const sampleDashboard: Dashboard = {
  id: 'd-1',
  name: 'Ops',
  createdAt: '2026-01-01T00:00:00Z',
  updatedAt: '2026-01-02T00:00:00Z',
  layout: { cols: 12, rowHeight: 80 },
  widgets: [
    {
      id: 'w-1',
      title: 'Revenue',
      workflowId: 'wf-1',
      viewMode: 'chart',
      chartConfig: {
        chartType: 'line',
        xAxis: null,
        yAxes: ['total'],
        groupBy: null,
        aggregation: 'none',
        sortBy: 'x_asc',
        showLegend: true,
        showGrid: true,
        showValues: false,
        colorScheme: 'default',
      },
      layout: { x: 0, y: 0, w: 6, h: 4 },
      refresh: { mode: 'interval', refreshSec: 60 },
      enabled: true,
    },
  ],
  enabled: true,
  refreshPaused: false,
};

/** The save answer is the post-save re-read, not the caller's echo. */
const reSavedDashboard: Dashboard = {
  ...sampleDashboard,
  updatedAt: '2026-01-03T00:00:00Z',
};

const sampleRef = {
  workflowId: 'wf-1',
  dashboardId: 'd-1',
  widgetId: 'w-1',
  dashboardName: 'Ops',
  widgetTitle: 'Revenue',
};

const sampleRun: WidgetRun = {
  id: 'run-1',
  dashboardId: 'd-1',
  widgetId: 'w-1',
  workflowId: 'wf-1',
  startedAt: '2026-01-02T00:00:00Z',
  finishedAt: '2026-01-02T00:00:01Z',
  status: 'ok',
  rowCount: 3,
  columns: ['total'],
  rows: [[1], [2], [3]],
};

const sampleIndexEntry: RunIndexEntry = {
  id: 'run-1',
  startedAt: '2026-01-02T00:00:00Z',
  status: 'ok',
  alertFired: false,
};

const sampleGrant: MonitorGrant = {
  id: 'grant-1',
  dashboardId: 'd-1',
  widgetId: 'w-1',
  workflowId: 'wf-1',
  workflowRevision: 'deadbeef',
  organizationId: 'org-1',
  servicePrincipal: 'monitor-service:grant-1',
  approvedBy: 'p-1',
  scopes: [
    {
      target: {
        connectionId: mustNarrow(toId('c-1')),
        namespace: { database: 'app', catalog: null, schema: null, path: [] },
        object: null,
      },
      configRevision: 3,
      command: 'query',
    },
  ],
  actions: ['connect', 'query'],
  readOnly: true,
  expiresAt: '2026-01-03T00:00:00Z',
  revocationVersion: 1,
  revoked: false,
};

// ── the table: one row per §6.15 method ─────────────────────────────────────

runContractTable('Dashboard', [
  {
    call: (c) => c.listDashboards(),
    method: 'listDashboards',
    payload: {},
    response: [sampleDashboard],
  },
  {
    call: (c) => c.getDashboard({ id: 'd-1' }),
    method: 'getDashboard',
    payload: { id: 'd-1' },
    response: sampleDashboard,
  },
  {
    call: (c) => c.saveDashboard({ dashboard: sampleDashboard }, token),
    method: 'saveDashboard',
    payload: { dashboard: sampleDashboard, submissionToken: token },
    response: reSavedDashboard,
  },
  {
    call: (c) => c.deleteDashboard({ id: 'd-1' }),
    method: 'deleteDashboard',
    payload: { id: 'd-1' },
    response: undefined,
  },
  {
    call: (c) => c.setDashboardRefreshPaused({ id: 'd-1', paused: true }, token),
    method: 'setDashboardRefreshPaused',
    payload: { id: 'd-1', paused: true, submissionToken: token },
    response: undefined,
  },
  {
    call: (c) => c.findDashboardWorkflowRefs({ workflowId: 'wf-1' }),
    method: 'findDashboardWorkflowRefs',
    payload: { workflowId: 'wf-1' },
    response: [sampleRef],
  },
  {
    call: (c) =>
      c.createWidgetFromSql(
        {
          dashboardId: 'd-1',
          params: {
            dashboardId: 'd-1',
            connectionId: 'c-1',
            sql: 'SELECT 1',
            viewMode: 'table',
          },
        },
        token,
      ),
    method: 'createWidgetFromSql',
    payload: {
      dashboardId: 'd-1',
      params: {
        dashboardId: 'd-1',
        connectionId: 'c-1',
        sql: 'SELECT 1',
        viewMode: 'table',
      },
      submissionToken: token,
    },
    response: { dashboard: sampleDashboard, widget: sampleDashboard.widgets[0] },
  },
  {
    call: (c) =>
      c.createWidgetFromWorkflow(
        {
          dashboardId: 'd-1',
          params: {
            dashboardId: 'd-1',
            workflowId: 'wf-2',
            title: 'Fixed query',
            viewMode: 'chart',
          },
        },
        token,
      ),
    method: 'createWidgetFromWorkflow',
    payload: {
      dashboardId: 'd-1',
      params: {
        dashboardId: 'd-1',
        workflowId: 'wf-2',
        title: 'Fixed query',
        viewMode: 'chart',
      },
      submissionToken: token,
    },
    response: { dashboard: sampleDashboard, widget: sampleDashboard.widgets[0] },
  },
  {
    call: (c) =>
      c.updateHiddenWidgetSql(
        { params: { workflowId: 'wf-1', connectionId: 'c-1', sql: 'SELECT 2' } },
        token,
      ),
    method: 'updateHiddenWidgetSql',
    payload: {
      params: { workflowId: 'wf-1', connectionId: 'c-1', sql: 'SELECT 2' },
      submissionToken: token,
    },
    response: undefined,
  },
  {
    call: (c) => c.runDashboardWidget({ dashboardId: 'd-1', widgetId: 'w-1' }, token),
    method: 'runDashboardWidget',
    payload: { dashboardId: 'd-1', widgetId: 'w-1', submissionToken: token },
    response: sampleRun,
  },
  {
    call: (c) => c.listWidgetRuns({ dashboardId: 'd-1', widgetId: 'w-1', limit: 25 }),
    method: 'listWidgetRuns',
    payload: { dashboardId: 'd-1', widgetId: 'w-1', limit: 25 },
    response: [sampleIndexEntry],
  },
  {
    call: (c) => c.getWidgetRun({ dashboardId: 'd-1', widgetId: 'w-1', runId: 'run-1' }),
    method: 'getWidgetRun',
    payload: { dashboardId: 'd-1', widgetId: 'w-1', runId: 'run-1' },
    response: sampleRun,
  },
  {
    call: (c) => c.getMonitorScheduleGrant({ dashboardId: 'd-1', widgetId: 'w-1' }),
    method: 'getMonitorScheduleGrant',
    payload: { dashboardId: 'd-1', widgetId: 'w-1' },
    response: sampleGrant,
  },
  {
    call: (c) =>
      c.authorizeMonitorSchedule({ dashboardId: 'd-1', widgetId: 'w-1', ttlSecs: 3600 }, token),
    method: 'authorizeMonitorSchedule',
    payload: { dashboardId: 'd-1', widgetId: 'w-1', ttlSecs: 3600, submissionToken: token },
    response: sampleGrant,
  },
  {
    call: (c) => c.revokeMonitorSchedule({ dashboardId: 'd-1', widgetId: 'w-1' }),
    method: 'revokeMonitorSchedule',
    payload: { dashboardId: 'd-1', widgetId: 'w-1' },
    response: undefined,
  },
]);
