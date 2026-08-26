import { beforeEach, describe, expect, it, vi } from 'vitest';

const invokeMock = vi.hoisted(() => vi.fn());

vi.mock('@tauri-apps/api/core', () => ({
  invoke: invokeMock,
}));

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

  it('listDashboards invokes list_dashboards without args', async () => {
    const dashboards = [sampleDashboard('d1')];
    invokeMock.mockResolvedValueOnce(dashboards);
    await expect(dashboardCommands.listDashboards()).resolves.toEqual(dashboards);
    expect(invokeMock).toHaveBeenCalledWith('list_dashboards');
  });

  it('getDashboard forwards the dashboard id', async () => {
    invokeMock.mockResolvedValueOnce(sampleDashboard('d2'));
    await expect(dashboardCommands.getDashboard('d2')).resolves.toMatchObject({ id: 'd2' });
    expect(invokeMock).toHaveBeenCalledWith('get_dashboard', { id: 'd2' });
  });

  it('saveDashboard forwards the dashboard payload', async () => {
    const dashboard = sampleDashboard('d3');
    invokeMock.mockResolvedValueOnce(dashboard);
    await expect(dashboardCommands.saveDashboard(dashboard)).resolves.toBe(dashboard);
    expect(invokeMock).toHaveBeenCalledWith('save_dashboard', { dashboard });
  });

  it('deleteDashboard forwards the dashboard id', async () => {
    await dashboardCommands.deleteDashboard('d4');
    expect(invokeMock).toHaveBeenCalledWith('delete_dashboard', { id: 'd4' });
  });

  it('setDashboardRefreshPaused forwards id and paused flag', async () => {
    await dashboardCommands.setDashboardRefreshPaused('d5', true);
    expect(invokeMock).toHaveBeenCalledWith('set_dashboard_refresh_paused', {
      id: 'd5',
      paused: true,
    });
  });

  it('findDashboardWorkflowRefs forwards the workflow id', async () => {
    invokeMock.mockResolvedValueOnce([]);
    await expect(dashboardCommands.findDashboardWorkflowRefs('wf-1')).resolves.toEqual([]);
    expect(invokeMock).toHaveBeenCalledWith('find_dashboard_workflow_refs', { workflowId: 'wf-1' });
  });

  it('listWidgetRuns forwards dashboard/widget ids and limit', async () => {
    invokeMock.mockResolvedValueOnce([]);
    await expect(dashboardCommands.listWidgetRuns('d6', 'w1', 25)).resolves.toEqual([]);
    expect(invokeMock).toHaveBeenCalledWith('list_widget_runs', {
      dashboardId: 'd6',
      widgetId: 'w1',
      limit: 25,
    });
  });

  it('getWidgetRun forwards composite run id', async () => {
    const run = { id: { dashboardId: 'd7', widgetId: 'w2', seq: 3 } };
    invokeMock.mockResolvedValueOnce(run);
    await expect(dashboardCommands.getWidgetRun('d7', 'w2', 3)).resolves.toBe(run);
    expect(invokeMock).toHaveBeenCalledWith('get_widget_run', {
      dashboardId: 'd7',
      widgetId: 'w2',
      runId: 3,
    });
  });

  it('runDashboardWidget forwards dashboard and widget ids', async () => {
    await dashboardCommands.runDashboardWidget('d8', 'w3');
    expect(invokeMock).toHaveBeenCalledWith('run_dashboard_widget', {
      dashboardId: 'd8',
      widgetId: 'w3',
    });
  });

  it('createWidgetFromSql forwards the params envelope verbatim', async () => {
    invokeMock.mockResolvedValueOnce({ widgetId: 'w4', dashboardId: 'd9' });
    const params = {
      dashboardId: 'd9',
      connectionId: 'c1',
      sql: 'SELECT 1',
      title: 'T',
      viewMode: 'chart' as const,
    };
    await expect(dashboardCommands.createWidgetFromSql(params)).resolves.toMatchObject({
      widgetId: 'w4',
    });
    expect(invokeMock).toHaveBeenCalledWith('create_widget_from_sql', { params });
  });

  it('createWidgetFromWorkflow forwards the params envelope verbatim', async () => {
    invokeMock.mockResolvedValueOnce({ widgetId: 'w5', dashboardId: 'd10' });
    const params = {
      dashboardId: 'd10',
      workflowId: 'wf-2',
      title: 'W',
      viewMode: 'chart' as const,
    };
    await expect(dashboardCommands.createWidgetFromWorkflow(params)).resolves.toMatchObject({
      widgetId: 'w5',
    });
    expect(invokeMock).toHaveBeenCalledWith('create_widget_from_workflow', { params });
  });

  it('updateHiddenWidgetSql forwards workflow/connection/sql triple', async () => {
    await dashboardCommands.updateHiddenWidgetSql({
      workflowId: 'wf-3',
      connectionId: 'c2',
      sql: 'SELECT 2',
    });
    expect(invokeMock).toHaveBeenCalledWith('update_hidden_widget_sql', {
      params: { workflowId: 'wf-3', connectionId: 'c2', sql: 'SELECT 2' },
    });
  });

  it('exportWithDialog returns dialog outcome', async () => {
    invokeMock.mockResolvedValueOnce(true);
    await expect(dashboardCommands.exportWithDialog('d11', 'dash.json')).resolves.toBe(true);
    expect(invokeMock).toHaveBeenCalledWith('export_dashboard_with_dialog', {
      dashboardId: 'd11',
      defaultFileName: 'dash.json',
    });
  });

  it('importWithDialog returns imported dashboard or null when cancelled', async () => {
    const imported = sampleDashboard('d12');
    invokeMock.mockResolvedValueOnce(imported);
    await expect(dashboardCommands.importWithDialog()).resolves.toBe(imported);

    invokeMock.mockResolvedValueOnce(null);
    await expect(dashboardCommands.importWithDialog()).resolves.toBeNull();
    expect(invokeMock).toHaveBeenLastCalledWith('import_dashboard_with_dialog');
  });
});
