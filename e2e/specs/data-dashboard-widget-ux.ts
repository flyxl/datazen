import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows } from '../helpers.js';
import {
  E2E_DASHBOARD_PREFIX,
  cleanupDashboard,
  createEmptyDashboard,
  getSeededConfigId,
  invokeBackend,
  openDashboardFromMain,
  seedSqlWidget,
} from '../helpers/data-dashboard.js';

const DASHBOARD_ID = `${E2E_DASHBOARD_PREFIX}-ux`;

describe('数据看板组件 UX (UJ-05, UJ-06, UJ-09)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.pause(1000);
    const configId = await getSeededConfigId();
    await cleanupDashboard(DASHBOARD_ID);
    await createEmptyDashboard(DASHBOARD_ID, 'E2E UX Board');
    await seedSqlWidget(DASHBOARD_ID, configId, {
      title: 'E2E UX Widget',
      sql: "SELECT 'A' AS category, 10 AS amount UNION ALL SELECT 'B', 20",
      xAxis: 'category',
      yAxes: ['amount'],
    });
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

  beforeEach(async () => {
    await browser.switchToWindow(mainWindow);
    await openDashboardFromMain(mainWindow);

    const refreshAll = await $('[data-testid="dashboard-refresh-all"]');
    await refreshAll.waitForDisplayed({ timeout: 5000 });
    await refreshAll.click();
    await browser.waitUntil(async () => (await $$('[data-testid="dashboard-tile"]')).length >= 1, {
      timeout: 15000,
      timeoutMsg: '等待 tile 渲染',
    });
  });

  it('UJ-05: 应能切换图表/表格视图', async () => {
    const toggle = await $('[data-testid="dashboard-view-toggle"]');
    await toggle.waitForDisplayed({ timeout: 5000 });

    const buttons = await toggle.$$('button');
    expect(buttons.length).toBe(2);

    await buttons[1].click();
    await browser.pause(800);

    await browser.waitUntil(
      async () => {
        const rows = await $$('table tbody tr');
        return rows.length >= 1;
      },
      { timeout: 10000, timeoutMsg: '等待表格视图' },
    );

    await buttons[0].click();
    await browser.pause(500);
  });

  it('UJ-06: 应打开组件编辑抽屉', async () => {
    const editBtns = await $$('[data-testid="dashboard-tile-edit"]');
    expect(editBtns.length).toBeGreaterThanOrEqual(1);
    await editBtns[0].click();

    const drawer = await $('[data-testid="widget-editor-drawer"]');
    await drawer.waitForDisplayed({ timeout: 5000 });
    const text = await drawer.getText();
    expect(
      text.includes('编辑组件') ||
        text.includes('Edit widget') ||
        text.includes('SQL') ||
        text.includes('dashboard.editWidget'),
    ).toBe(true);

    await browser.keys('Escape');
    await browser.pause(400);
  });

  it('UJ-09: 应打开运行历史抽屉', async () => {
    const historyBtns = await $$('[data-testid="dashboard-tile-history"]');
    expect(historyBtns.length).toBeGreaterThanOrEqual(1);
    await historyBtns[0].click();

    const drawer = await $('[data-testid="run-history-drawer"]');
    await drawer.waitForDisplayed({ timeout: 5000 });
    await browser.waitUntil(
      async () => {
        const text = await drawer.getText();
        return (
          text.includes('运行历史') ||
          text.includes('Run history') ||
          text.includes('ok') ||
          text.length > 10
        );
      },
      { timeout: 10000, timeoutMsg: '等待历史抽屉内容' },
    );

    await browser.keys('Escape');
    await browser.pause(400);
  });
});
