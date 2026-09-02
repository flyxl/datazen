import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows } from '../helpers.js';
import {
  E2E_DASHBOARD_PREFIX,
  cleanupDashboard,
  createEmptyDashboard,
  getSeededConnectionId,
  invokeBackend,
  openDashboardFromMain,
  seedSqlWidget,
} from '../helpers/data-dashboard.js';

const DASHBOARD_ID = `${E2E_DASHBOARD_PREFIX}-sql`;
const WIDGET_TITLE = 'E2E SQL Metric';

describe('数据看板 SQL 添加 (UJ-03, UJ-13)', () => {
  let mainWindow: string;
  let connectionId: string;
  let widgetId: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.pause(1000);
    connectionId = await getSeededConnectionId();
    await cleanupDashboard(DASHBOARD_ID);
    await createEmptyDashboard(DASHBOARD_ID, 'E2E SQL Board');
    const created = await seedSqlWidget(DASHBOARD_ID, connectionId, {
      title: WIDGET_TITLE,
      sql: "SELECT 'Alpha' AS category, 100 AS amount UNION ALL SELECT 'Beta', 200 UNION ALL SELECT 'Gamma', 150",
      xAxis: 'category',
      yAxes: ['amount'],
    });
    widgetId = created.widget.id;
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await cleanupDashboard(DASHBOARD_ID);
    } catch {
      /* ok */
    }
    await closeExtraWindows(mainWindow);
  });

  it('UJ-03: IPC seed 后打开看板应显示磁贴', async () => {
    await openDashboardFromMain(mainWindow);

    await browser.waitUntil(async () => (await $$('[data-testid="dashboard-tile"]')).length >= 1, {
      timeout: 10000,
      timeoutMsg: '等待 dashboard tile',
    });

    const body = await $('body').getText();
    expect(body).toContain(WIDGET_TITLE);

    const dash = await invokeBackend<{
      widgets: { id: string; workflowId: string; sql?: string }[];
    }>('get_dashboard', { id: DASHBOARD_ID });
    const widget = dash.widgets.find((w) => w.id === widgetId);
    expect(widget).toBeTruthy();
    expect(widget!.workflowId).toBeTruthy();
    expect(widget!.sql).toBeUndefined();
  });

  it('UJ-13: 全部刷新后应渲染图表', async () => {
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

    const run = await invokeBackend<{ status: string; rowCount: number }>('run_dashboard_widget', {
      dashboardId: DASHBOARD_ID,
      widgetId,
    });
    expect(run.status).toBe('ok');
    expect(run.rowCount).toBeGreaterThan(0);
  });
});
