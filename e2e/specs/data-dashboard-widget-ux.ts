import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows } from '../helpers.js';
import { t } from '../i18n.js';
import {
  E2E_DASHBOARD_PREFIX,
  cleanupDashboard,
  createEmptyDashboard,
  getSeededConnectionId,
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
    const connectionId = await getSeededConnectionId();
    await cleanupDashboard(DASHBOARD_ID);
    await createEmptyDashboard(DASHBOARD_ID, 'E2E UX Board');
    await seedSqlWidget(DASHBOARD_ID, connectionId, {
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
    // Wait until a successful run paints chart (or table if already toggled).
    await browser.waitUntil(
      async () => {
        const chart = await $('[data-testid="dashboard-tile-chart"]');
        const table = await $('[data-testid="dashboard-tile-table"]');
        return (await chart.isExisting()) || (await table.isExisting());
      },
      { timeout: 20000, timeoutMsg: '等待组件数据渲染' },
    );
  });

  it('UJ-05: 应能切换图表/表格视图', async () => {
    const tableBtn = await $('[data-testid="widget-view-table"]');
    await tableBtn.waitForDisplayed({ timeout: 5000 });
    await tableBtn.click();

    await browser.waitUntil(
      async () => (await $('[data-testid="dashboard-tile-table"]')).isDisplayed(),
      { timeout: 10000, timeoutMsg: '等待表格视图' },
    );

    // 表格视图的底部导出按钮必须可见且可交互（数据量较大时也不得被容器裁剪）。
    const exportBtn = await $(
      `[data-testid="dashboard-tile-table"] button[title="${t('export.export')}"]`,
    );
    await exportBtn.waitForDisplayed({ timeout: 5000 });
    await exportBtn.click();
    const exportDialog = await $(`=${t('export.title')}`);
    await exportDialog.waitForDisplayed({ timeout: 5000 });
    await browser.keys('Escape');
    await browser.pause(300);

    const chartBtn = await $('[data-testid="widget-view-chart"]');
    await chartBtn.click();
    await browser.waitUntil(
      async () => (await $('[data-testid="dashboard-tile-chart"]')).isDisplayed(),
      { timeout: 10000, timeoutMsg: '等待图表视图' },
    );
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

    const sqlEditor = await $('[data-testid="dashboard-sql-editor"]');
    if (await sqlEditor.isExisting()) {
      await expect(await $('[data-testid="dashboard-sql-editor"] .cm-editor')).toBeDisplayed();
    }

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
