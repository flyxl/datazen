/**
 * Shared helpers for data-dashboard E2E specs (UJ-01 … UJ-13).
 */
import { browser, $ } from '@wdio/globals';
import { switchToNewWindow } from '../helpers.js';

export const E2E_DASHBOARD_PREFIX = 'e2e-data-dashboard';

export async function invokeBackend<T>(
  cmd: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && result !== null && '__error' in result) {
    throw new Error(String((result as { __error: string }).__error));
  }
  return result as T;
}

/** Invoke backend and expect failure; returns error message. */
export async function invokeBackendExpectError(
  cmd: string,
  args: Record<string, unknown> = {},
): Promise<string> {
  try {
    await invokeBackend(cmd, args);
    throw new Error(`Expected ${cmd} to fail`);
  } catch (e) {
    return String(e);
  }
}

export function chartConfig(xAxis: string | null, yAxes: string[]) {
  return {
    chartType: 'bar',
    xAxis,
    yAxes,
    groupBy: null,
    aggregation: 'none',
    sortBy: 'none',
    showLegend: true,
    showGrid: true,
    showValues: false,
    colorScheme: 'default',
  };
}

export async function getSeededConnectionId(): Promise<string> {
  const conns = await invokeBackend<{ id: string; name?: string }[]>('get_connections');
  const seeded = conns.find((c) => c.id === 'conn_e2e_pg' || c.name === '本地 PostgreSQL');
  const connectionId = seeded?.id ?? conns[0]?.id;
  if (!connectionId) {
    throw new Error('data-dashboard E2E 需要至少一个已保存连接（持久化连接 id）');
  }
  return connectionId;
}

export async function createEmptyDashboard(id: string, name: string) {
  const now = new Date().toISOString();
  const dashboard = {
    id,
    name,
    createdAt: now,
    updatedAt: now,
    layout: { cols: 12, rowHeight: 80 },
    enabled: true,
    widgets: [],
  };
  await invokeBackend('save_dashboard', { dashboard });
  return dashboard;
}

export async function seedSqlWidget(
  dashboardId: string,
  connectionId: string,
  opts: {
    title: string;
    sql: string;
    xAxis?: string | null;
    yAxes?: string[];
  },
) {
  return invokeBackend<{ dashboard: { id: string }; widget: { id: string; title: string } }>(
    'create_widget_from_sql',
    {
      params: {
        dashboardId,
        connectionId,
        sql: opts.sql,
        title: opts.title,
        viewMode: 'chart',
        chartConfig: chartConfig(opts.xAxis ?? null, opts.yAxes ?? ['v']),
      },
    },
  );
}

export async function cleanupDashboard(id: string) {
  try {
    await invokeBackend('delete_dashboard', { id });
  } catch {
    /* ok */
  }
}

export async function setDashboardPaused(id: string, paused: boolean) {
  await invokeBackend('set_dashboard_refresh_paused', { id, paused });
}

/** Click main-window dashboard action → new dashboard window (no dialog). */
export async function openDashboardFromMain(mainHandle: string): Promise<string> {
  await browser.switchToWindow(mainHandle);
  await browser.pause(500);
  const btn = await $('[data-testid="action.dashboard"]');
  await btn.waitForDisplayed({ timeout: 10000 });
  await btn.click();

  const dialog = await $('[data-testid="dashboard-dialog"]');
  const hasDialog = await dialog.isExisting().catch(() => false);
  if (hasDialog && (await dialog.isDisplayed().catch(() => false))) {
    throw new Error('dashboard-dialog should not appear; expected direct window open');
  }

  const dashWindow = await switchToNewWindow(mainHandle);
  await browser.pause(1500);
  const root = await $('[data-testid="dashboard-window"]');
  await root.waitForDisplayed({ timeout: 15000 });
  return dashWindow;
}

export async function openSettingsWindow() {
  const { openSettingsInMainWindow } = await import('../helpers.js');
  await openSettingsInMainWindow();
}
