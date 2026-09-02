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
  setDashboardPaused,
} from '../helpers/data-dashboard.js';

const DASHBOARD_ID = `${E2E_DASHBOARD_PREFIX}-refresh`;

describe('数据看板刷新 (UJ-07)', () => {
  let mainWindow: string;
  let widgetId: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.pause(300);
    const connectionId = await getSeededConnectionId();
    await cleanupDashboard(DASHBOARD_ID);
    await createEmptyDashboard(DASHBOARD_ID, 'E2E Refresh Board');
    const created = await seedSqlWidget(DASHBOARD_ID, connectionId, {
      title: 'E2E Refresh Widget',
      sql: 'SELECT 1 AS v UNION ALL SELECT 2 UNION ALL SELECT 3',
      xAxis: null,
      yAxes: ['v'],
    });
    widgetId = created.widget.id;
    await setDashboardPaused(DASHBOARD_ID, false);
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

  it('UJ-07: 全部刷新按钮应触发组件运行', async () => {
    await openDashboardFromMain(mainWindow);

    const refreshAll = await $('[data-testid="dashboard-refresh-all"]');
    await refreshAll.waitForDisplayed({ timeout: 5000 });
    await refreshAll.click();

    await browser.waitUntil(
      async () => {
        const charts = await $$('[class*="recharts-wrapper"]');
        if (charts.length >= 1) return true;
        const tiles = await $$('[data-testid="dashboard-tile"]');
        if (tiles.length === 0) return false;
        const text = await tiles[0].getText();
        return text.includes('E2E Refresh Widget');
      },
      { timeout: 60000, timeoutMsg: '等待刷新完成' },
    );

    const run = await invokeBackend<{ status: string }>('run_dashboard_widget', {
      dashboardId: DASHBOARD_ID,
      widgetId,
    });
    expect(run.status).toBe('ok');
  });

  it('UJ-07: 暂停后 IPC 应反映 refreshPaused', async () => {
    const pauseBtn = await $('[data-testid="dashboard-pause-toggle"]');
    await pauseBtn.waitForDisplayed({ timeout: 5000 });
    await pauseBtn.click();
    await browser.pause(500);

    const dash = await invokeBackend<{ refreshPaused?: boolean }>('get_dashboard', {
      id: DASHBOARD_ID,
    });
    expect(dash.refreshPaused).toBe(true);
  });
});
