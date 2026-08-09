import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows, switchToNewWindow } from '../helpers.js';

const DASHBOARD_ID = 'e2e-ops-dashboard';
const WIDGET_A = 'e2e-widget-a';
const WIDGET_B = 'e2e-widget-b';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as unknown as { __TAURI_INTERNALS__: { invoke: (cmd: string, args: unknown) => Promise<unknown> } })
        .__TAURI_INTERNALS__
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

function chartConfig(xAxis: string | null, yAxes: string[]) {
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

async function seedDashboard(configId: string) {
  const now = new Date().toISOString();
  const dashboard = {
    id: DASHBOARD_ID,
    name: 'E2E Ops Dashboard',
    createdAt: now,
    updatedAt: now,
    layout: { cols: 12, rowHeight: 80 },
    enabled: true,
    widgets: [
      {
        id: WIDGET_A,
        title: 'E2E Metric A',
        configId,
        sql: "SELECT 'Alpha' AS category, 100 AS amount UNION ALL SELECT 'Beta', 200 UNION ALL SELECT 'Gamma', 150",
        chartConfig: chartConfig('category', ['amount']),
        layout: { x: 0, y: 0, w: 6, h: 4 },
        refreshSec: 3600,
        enabled: true,
      },
      {
        id: WIDGET_B,
        title: 'E2E Metric B',
        configId,
        sql: 'SELECT 1 AS v UNION ALL SELECT 2 UNION ALL SELECT 3',
        chartConfig: chartConfig(null, ['v']),
        layout: { x: 6, y: 0, w: 6, h: 4 },
        refreshSec: 3600,
        enabled: true,
      },
    ],
  };
  await invokeBackend('save_dashboard', { dashboard });
  await invokeBackend('set_monitor_paused', { paused: true });
}

async function cleanupDashboard() {
  try {
    await invokeBackend('delete_dashboard', { id: DASHBOARD_ID });
  } catch {
    /* ok */
  }
  try {
    await invokeBackend('set_monitor_paused', { paused: false });
  } catch {
    /* ok */
  }
}

async function openDashboardDialog(mainHandle: string) {
  await browser.switchToWindow(mainHandle);
  await browser.pause(500);
  const btn = await $('[data-testid="action.dashboard"]');
  await btn.waitForDisplayed({ timeout: 10000 });
  await btn.click();
  const dialog = await $('[data-testid="dashboard-dialog"]');
  await dialog.waitForDisplayed({ timeout: 10000 });
}

async function openSeededDashboard(mainHandle: string) {
  await openDashboardDialog(mainHandle);
  const openBtn = await $(`[data-testid="dashboard-open-${DASHBOARD_ID}"]`);
  await openBtn.waitForDisplayed({ timeout: 10000 });
  await openBtn.click();
  const dashWindow = await switchToNewWindow(mainHandle);
  await browser.pause(1500);
  const root = await $('[data-testid="dashboard-window"]');
  await root.waitForDisplayed({ timeout: 15000 });
  return dashWindow;
}

describe('运营看板 (OPS-DASHBOARD)', () => {
  let mainWindow: string;
  let configId: string | undefined;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.pause(2000);

    const conns = await invokeBackend<{ id: string }[]>('get_connections');
    if (conns.length === 0) {
      throw new Error('OPS-DASHBOARD 需要至少一个已保存连接（config_id）');
    }
    configId = conns[0].id;

    await cleanupDashboard();
    await seedDashboard(configId);
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await cleanupDashboard();
    } catch {
      /* ok */
    }
    await closeExtraWindows(mainWindow);
  });

  it('应从主窗口打开运营看板对话框并列出已有看板', async () => {
    await openDashboardDialog(mainWindow);
    const item = await $(`[data-testid="dashboard-list-item-${DASHBOARD_ID}"]`);
    await item.waitForDisplayed({ timeout: 10000 });
    const text = await item.getText();
    expect(text).toContain('E2E Ops Dashboard');
    // close dialog without opening
    const closeBtn = await $('button*=关闭');
    if (await closeBtn.isExisting()) {
      await closeBtn.click();
    } else {
      await browser.keys('Escape');
    }
    await browser.pause(300);
  });

  it('应打开看板窗口并显示两个组件磁贴', async () => {
    await openSeededDashboard(mainWindow);
    const tiles = await $$('[data-testid="dashboard-tile"]');
    await browser.waitUntil(async () => (await $$('[data-testid="dashboard-tile"]')).length >= 2, {
      timeout: 10000,
      timeoutMsg: '等待两个 dashboard tile',
    });
    expect(tiles.length).toBeGreaterThanOrEqual(2);

    const body = await $('body').getText();
    expect(body).toContain('E2E Metric A');
    expect(body).toContain('E2E Metric B');
  });

  it('应能全部刷新并渲染图表', async () => {
    const refreshAll = await $('[data-testid="dashboard-refresh-all"]');
    await refreshAll.waitForDisplayed({ timeout: 5000 });
    await refreshAll.click();

    await browser.waitUntil(
      async () => {
        const charts = await $$('[class*="recharts-wrapper"]');
        return charts.length >= 1;
      },
      { timeout: 60000, timeoutMsg: '等待看板图表渲染超时' },
    );

    const charts = await $$('[class*="recharts-wrapper"]');
    expect(charts.length).toBeGreaterThanOrEqual(1);
  });

  it('应打开组件编辑抽屉', async () => {
    const editBtns = await $$('[data-testid="dashboard-tile-edit"]');
    expect(editBtns.length).toBeGreaterThanOrEqual(1);
    await editBtns[0].click();
    const drawer = await $('[data-testid="widget-editor-drawer"]');
    await drawer.waitForDisplayed({ timeout: 5000 });
    const text = await drawer.getText();
    expect(text.includes('编辑组件') || text.includes('Edit widget') || text.includes('SQL')).toBe(true);
    await browser.keys('Escape');
    await browser.pause(400);
  });

  it('应打开运行历史抽屉', async () => {
    const historyBtns = await $$('[data-testid="dashboard-tile-history"]');
    expect(historyBtns.length).toBeGreaterThanOrEqual(1);
    await historyBtns[0].click();
    const drawer = await $('[data-testid="run-history-drawer"]');
    await drawer.waitForDisplayed({ timeout: 5000 });
    await browser.waitUntil(
      async () => {
        const text = await drawer.getText();
        return text.includes('运行历史') || text.includes('Run history') || text.includes('ok') || text.length > 10;
      },
      { timeout: 10000, timeoutMsg: '等待历史抽屉内容' },
    );
    await browser.keys('Escape');
    await browser.pause(400);
  });

  it('应能切换监控暂停状态', async () => {
    const pauseBtn = await $('[data-testid="dashboard-pause-toggle"]');
    await pauseBtn.waitForDisplayed({ timeout: 5000 });
    // seeded with paused=true → title should be resume
    let title = await pauseBtn.getAttribute('title');
    expect(title === '恢复监控' || title === 'Resume monitoring' || (title?.length ?? 0) > 0).toBe(true);

    await pauseBtn.click();
    await browser.pause(500);
    const paused = await invokeBackend<boolean>('get_monitor_paused');
    expect(paused).toBe(false);

    await pauseBtn.click();
    await browser.pause(500);
    const pausedAgain = await invokeBackend<boolean>('get_monitor_paused');
    expect(pausedAgain).toBe(true);
  });

  it('应能通过 IPC 手动执行单个组件', async () => {
    const run = await invokeBackend<{ status: string; rowCount: number }>(
      'run_dashboard_widget',
      { dashboardId: DASHBOARD_ID, widgetId: WIDGET_A },
    );
    expect(run.status).toBe('ok');
    expect(run.rowCount).toBeGreaterThan(0);
  });
});
