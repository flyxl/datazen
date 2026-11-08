import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

import {
  createBackendGuard,
  createDashboardMethods,
  setBackendClient,
} from '@datazen/backend-client';
// W8 刀3d 收口：wrappers 走域门面方法。假门面用**真域客户端**（dashboard
// 域）架在假 transport 上——令牌折叠是被测产品代码，测试只钉 wire 面：
// invokeMock(method, payload) 的键与形状。令牌铸造返回固定形状且**不进
// invokeMock 队列**（schemaDiff.test.ts 同款绑定）。
const TEST_TOKEN = { idempotencyKey: 'test-key', expiresAt: '2999-01-01T00:00:00Z' };
const fakeTransport = {
  call: (c: string, p?: unknown) => (p === undefined ? invokeMock(c) : invokeMock(c, p)),
};
setBackendClient('test', {
  transport: fakeTransport,
  issueSubmissionToken: async () => TEST_TOKEN,
  ...createDashboardMethods(fakeTransport, createBackendGuard('test', {})),
} as never);
import { dashboardCommands } from '../dashboard';
import type { Dashboard } from '../../types/dashboard';

function sampleDashboard(id: string): Dashboard {
  return {
    id,
    name: `Dash ${id}`,
    createdAt: '2026-01-01T00:00:00Z',
    updatedAt: '2026-01-01T00:00:00Z',
    layout: { cols: 12, rowHeight: 80 },
    widgets: [],
  } as unknown as Dashboard;
}

describe('dashboardCommands wrappers', () => {
  beforeEach(() => {
    invokeMock.mockReset();
    invokeMock.mockResolvedValue(undefined);
  });

  it('schedule approval sends identifiers, duration and the user-asset token; revoke sends no identity', async () => {
    await dashboardCommands.authorizeMonitorSchedule('d', 'w');
    expect(invokeMock).toHaveBeenCalledWith('authorizeMonitorSchedule', {
      dashboardId: 'd',
      widgetId: 'w',
      ttlSecs: 3600,
      submissionToken: TEST_TOKEN,
    });
    await dashboardCommands.revokeMonitorSchedule('d', 'w');
    expect(invokeMock).toHaveBeenCalledWith('revokeMonitorSchedule', {
      dashboardId: 'd',
      widgetId: 'w',
    });
  });

  it('getMonitorScheduleGrant reads the grant without a token', async () => {
    const grant = { id: 'grant-1', revoked: false };
    invokeMock.mockResolvedValueOnce(grant);
    await expect(dashboardCommands.getMonitorScheduleGrant('d0', 'w0')).resolves.toBe(grant);
    expect(invokeMock).toHaveBeenCalledWith('getMonitorScheduleGrant', {
      dashboardId: 'd0',
      widgetId: 'w0',
    });
  });

  it('listDashboards calls the domain list with an empty envelope', async () => {
    const dashboards = [sampleDashboard('d1')];
    invokeMock.mockResolvedValueOnce(dashboards);
    await expect(dashboardCommands.listDashboards()).resolves.toEqual(dashboards);
    expect(invokeMock).toHaveBeenCalledWith('listDashboards', {});
  });

  it('getDashboard forwards the dashboard id', async () => {
    invokeMock.mockResolvedValueOnce(sampleDashboard('d2'));
    await expect(dashboardCommands.getDashboard('d2')).resolves.toMatchObject({ id: 'd2' });
    expect(invokeMock).toHaveBeenCalledWith('getDashboard', { id: 'd2' });
  });

  it('saveDashboard forwards the dashboard payload with the token folded in', async () => {
    const dashboard = sampleDashboard('d3');
    invokeMock.mockResolvedValueOnce(dashboard);
    await expect(dashboardCommands.saveDashboard(dashboard)).resolves.toBe(dashboard);
    expect(invokeMock).toHaveBeenCalledWith('saveDashboard', {
      dashboard,
      submissionToken: TEST_TOKEN,
    });
  });

  it('deleteDashboard forwards the dashboard id without a token', async () => {
    await dashboardCommands.deleteDashboard('d4');
    expect(invokeMock).toHaveBeenCalledWith('deleteDashboard', { id: 'd4' });
  });

  it('setDashboardRefreshPaused forwards id and paused flag with the token', async () => {
    await dashboardCommands.setDashboardRefreshPaused('d5', true);
    expect(invokeMock).toHaveBeenCalledWith('setDashboardRefreshPaused', {
      id: 'd5',
      paused: true,
      submissionToken: TEST_TOKEN,
    });
  });

  it('findDashboardWorkflowRefs forwards the workflow id', async () => {
    invokeMock.mockResolvedValueOnce([]);
    await expect(dashboardCommands.findDashboardWorkflowRefs('wf-1')).resolves.toEqual([]);
    expect(invokeMock).toHaveBeenCalledWith('findDashboardWorkflowRefs', { workflowId: 'wf-1' });
  });

  it('listWidgetRuns forwards dashboard/widget ids and limit', async () => {
    invokeMock.mockResolvedValueOnce([]);
    await expect(dashboardCommands.listWidgetRuns('d6', 'w1', 25)).resolves.toEqual([]);
    expect(invokeMock).toHaveBeenCalledWith('listWidgetRuns', {
      dashboardId: 'd6',
      widgetId: 'w1',
      limit: 25,
    });
  });

  it('getWidgetRun forwards run id', async () => {
    // The backend identifies a widget run by an opaque string id (`getWidgetRun`
    // takes `runId`), not by a numeric sequence.
    const run = { id: 'run-3' };
    invokeMock.mockResolvedValueOnce(run);
    await expect(dashboardCommands.getWidgetRun('d7', 'w2', 'run-3')).resolves.toBe(run);
    expect(invokeMock).toHaveBeenCalledWith('getWidgetRun', {
      dashboardId: 'd7',
      widgetId: 'w2',
      runId: 'run-3',
    });
  });

  it('runDashboardWidget forwards dashboard and widget ids with the token', async () => {
    await dashboardCommands.runDashboardWidget('d8', 'w3');
    expect(invokeMock).toHaveBeenCalledWith('runDashboardWidget', {
      dashboardId: 'd8',
      widgetId: 'w3',
      submissionToken: TEST_TOKEN,
    });
  });

  it('createWidgetFromSql forwards the params envelope verbatim with a top-level dashboardId', async () => {
    const dashboard = sampleDashboard('d9');
    invokeMock.mockResolvedValueOnce({ dashboard, widget: { id: 'w4' } });
    const params = {
      dashboardId: 'd9',
      connectionId: 'c1',
      sql: 'SELECT 1',
      title: 'T',
      viewMode: 'chart' as const,
    };
    await expect(dashboardCommands.createWidgetFromSql(params)).resolves.toMatchObject({
      widget: { id: 'w4' },
    });
    expect(invokeMock).toHaveBeenCalledWith('createWidgetFromSql', {
      dashboardId: 'd9',
      params,
      submissionToken: TEST_TOKEN,
    });
  });

  it('createWidgetFromWorkflow forwards the params envelope verbatim with a top-level dashboardId', async () => {
    const dashboard = sampleDashboard('d10');
    invokeMock.mockResolvedValueOnce({ dashboard, widget: { id: 'w5' } });
    const params = {
      dashboardId: 'd10',
      workflowId: 'wf-2',
      title: 'W',
      viewMode: 'chart' as const,
    };
    await expect(dashboardCommands.createWidgetFromWorkflow(params)).resolves.toMatchObject({
      widget: { id: 'w5' },
    });
    expect(invokeMock).toHaveBeenCalledWith('createWidgetFromWorkflow', {
      dashboardId: 'd10',
      params,
      submissionToken: TEST_TOKEN,
    });
  });

  it('updateHiddenWidgetSql forwards the workflow/connection/sql triple nested under params', async () => {
    await dashboardCommands.updateHiddenWidgetSql({
      workflowId: 'wf-3',
      connectionId: 'c2',
      sql: 'SELECT 2',
    });
    expect(invokeMock).toHaveBeenCalledWith('updateHiddenWidgetSql', {
      params: { workflowId: 'wf-3', connectionId: 'c2', sql: 'SELECT 2' },
      submissionToken: TEST_TOKEN,
    });
  });

  it('exportWithDialog returns dialog outcome (desktop-only legacy rail)', async () => {
    invokeMock.mockResolvedValueOnce(true);
    await expect(dashboardCommands.exportWithDialog('d11', 'dash.json')).resolves.toBe(true);
    expect(invokeMock).toHaveBeenCalledWith('export_dashboard_with_dialog', {
      dashboardId: 'd11',
      defaultFileName: 'dash.json',
    });
  });

  it('importWithDialog returns imported dashboard or null when cancelled (desktop-only legacy rail)', async () => {
    const imported = sampleDashboard('d12');
    invokeMock.mockResolvedValueOnce(imported);
    await expect(dashboardCommands.importWithDialog()).resolves.toBe(imported);

    invokeMock.mockResolvedValueOnce(null);
    await expect(dashboardCommands.importWithDialog()).resolves.toBeNull();
    expect(invokeMock).toHaveBeenLastCalledWith('import_dashboard_with_dialog');
  });
});
