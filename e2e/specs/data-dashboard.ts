import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows } from '../helpers.js';
import { t } from '../i18n.js';
import {
  E2E_DASHBOARD_PREFIX,
  chartConfig,
  cleanupDashboard,
  createEmptyDashboard,
  getSeededConnectionId,
  invokeBackend,
  invokeBackendExpectError,
  openDashboardFromMain,
  openSettingsWindow,
  seedSqlWidget,
  setDashboardPaused,
} from '../helpers/data-dashboard.js';

describe('Data Dashboard', () => {
  describe('数据看板入口 (UJ-01, UJ-11)', () => {
    const DASHBOARD_ID = `${E2E_DASHBOARD_PREFIX}-entry`;
    let mainWindow: string;

    before(async () => {
      mainWindow = await browser.getWindowHandle();
      await browser.pause(300);
      await cleanupDashboard(DASHBOARD_ID);
      await createEmptyDashboard(DASHBOARD_ID, 'E2E Entry Board');
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

    it('UJ-01: 主窗口导航应打开嵌入看板面板（无 dashboard-dialog）', async () => {
      await openDashboardFromMain(mainWindow);

      const dialog = await $('[data-testid="dashboard-dialog"]');
      expect(await dialog.isExisting()).toBe(false);

      const root = await $('[data-testid="dashboard-panel"]');
      await expect(root).toBeDisplayed();
    });

    it('UJ-11: 设置窗口不应包含监控/Monitor 分区', async () => {
      await openSettingsWindow();

      const monitorNav = await $('[data-testid="settings-nav-monitor"]');
      expect(await monitorNav.isExisting()).toBe(false);

      const body = await $('body').getText();
      expect(body.includes('监控') || body.includes('Monitor')).toBe(false);
    });
  });

  describe('数据看板刷新 (UJ-07)', () => {
    const DASHBOARD_ID = `${E2E_DASHBOARD_PREFIX}-refresh`;
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

  describe('数据看板 SQL 添加 (UJ-03, UJ-13)', () => {
    const DASHBOARD_ID = `${E2E_DASHBOARD_PREFIX}-sql`;
    const WIDGET_TITLE = 'E2E SQL Metric';
    let mainWindow: string;
    let connectionId: string;
    let widgetId: string;

    before(async () => {
      mainWindow = await browser.getWindowHandle();
      await browser.pause(300);
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

  describe('数据看板面板 (UJ-02, UJ-08)', () => {
    const BOARD_A = `${E2E_DASHBOARD_PREFIX}-boards-a`;
    const BOARD_B = `${E2E_DASHBOARD_PREFIX}-boards-b`;
    let mainWindow: string;

    before(async () => {
      mainWindow = await browser.getWindowHandle();
      await browser.pause(300);
      await cleanupDashboard(BOARD_A);
      await cleanupDashboard(BOARD_B);
      await createEmptyDashboard(BOARD_A, 'E2E Board A');
      await setDashboardPaused(BOARD_A, true);
    });

    after(async () => {
      try {
        await browser.switchToWindow(mainWindow);
        await cleanupDashboard(BOARD_A);
        await cleanupDashboard(BOARD_B);
      } catch {
        /* ok */
      }
      await closeExtraWindows(mainWindow);
    });

    it('UJ-02: 应显示 Tab、新建面板并删除面板', async () => {
      await openDashboardFromMain(mainWindow);

      const tabA = await $(`[data-testid="dashboard-tab"][data-dashboard-id="${BOARD_A}"]`);
      await tabA.waitForDisplayed({ timeout: 10000 });

      const addTab = await $('[data-testid="dashboard-tab-add"]');
      await addTab.click();
      await browser.pause(800);

      const tabs = await $$('[data-testid="dashboard-tab"]');
      expect(tabs.length).toBeGreaterThanOrEqual(2);

      const list = await invokeBackend<{ id: string; name: string }[]>('list_dashboards');
      expect(list.length).toBeGreaterThanOrEqual(2);

      const deletePanel = await $('[data-testid="dashboard-delete-panel"]');
      await deletePanel.waitForDisplayed({ timeout: 5000 });
      await deletePanel.click();

      // The confirmation is rendered in a portal. Use its E2E locator instead
      // of searching all buttons by translated text; the latter can hit a
      // hidden/stale button and leave the dashboard untouched.
      const confirmDelete = await $('[data-testid="dashboard-delete-confirm"]');
      await confirmDelete.waitForDisplayed({ timeout: 5000 });
      await confirmDelete.waitForClickable({ timeout: 5000 });
      await confirmDelete.click();

      await browser.waitUntil(
        async () => {
          const current = await invokeBackend<{ id: string }[]>('list_dashboards');
          return current.length === list.length - 1;
        },
        {
          timeout: 10000,
          interval: 200,
          timeoutMsg: 'dashboard deletion did not complete in the backend',
        },
      );
      const listAfter = await invokeBackend<{ id: string }[]>('list_dashboards');
      expect(listAfter.length).toBe(list.length - 1);
    });

    it('UJ-08: 应能切换面板级暂停状态', async () => {
      await browser.switchToWindow(mainWindow);
      await openDashboardFromMain(mainWindow);

      const pauseBtn = await $('[data-testid="dashboard-pause-toggle"]');
      await pauseBtn.waitForDisplayed({ timeout: 5000 });

      // `DashboardPanel` initializes monitorPaused asynchronously from
      // loadDashboard. Wait for both sources of truth before the first click;
      // otherwise the button can briefly represent "pause" while the backend
      // is already paused, making the first click a no-op.
      await browser.waitUntil(
        async () => {
          const dash = await invokeBackend<{ refreshPaused?: boolean }>('get_dashboard', {
            id: BOARD_A,
          });
          return (
            dash.refreshPaused === true &&
            (await pauseBtn.getAttribute('title')) === '恢复本看板定时'
          );
        },
        {
          timeout: 10000,
          interval: 200,
          timeoutMsg: 'dashboard pause state did not load into the UI',
        },
      );

      await pauseBtn.click();
      await browser.waitUntil(
        async () => {
          const dash = await invokeBackend<{ refreshPaused?: boolean }>('get_dashboard', {
            id: BOARD_A,
          });
          return (
            dash.refreshPaused === false &&
            (await pauseBtn.getAttribute('title')) === '暂停本看板定时'
          );
        },
        {
          timeout: 10000,
          interval: 200,
          timeoutMsg: 'dashboard resume state did not propagate to the UI',
        },
      );
      const dash = await invokeBackend<{ refreshPaused?: boolean }>('get_dashboard', { id: BOARD_A });
      expect(dash.refreshPaused).toBe(false);

      await pauseBtn.click();
      await browser.waitUntil(
        async () => {
          const dashAgain = await invokeBackend<{ refreshPaused?: boolean }>('get_dashboard', {
            id: BOARD_A,
          });
          return (
            dashAgain.refreshPaused === true &&
            (await pauseBtn.getAttribute('title')) === '恢复本看板定时'
          );
        },
        {
          timeout: 10000,
          interval: 200,
          timeoutMsg: 'dashboard pause state did not propagate to the UI',
        },
      );
      const dashAgain = await invokeBackend<{ refreshPaused?: boolean }>('get_dashboard', {
        id: BOARD_A,
      });
      expect(dashAgain.refreshPaused).toBe(true);
    });
  });

  describe('数据看板组件 UX (UJ-05, UJ-06, UJ-09)', () => {
    const DASHBOARD_ID = `${E2E_DASHBOARD_PREFIX}-ux`;
    let mainWindow: string;

    before(async () => {
      mainWindow = await browser.getWindowHandle();
      await browser.pause(300);
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
      const exportDialog = await $('[data-testid="data-export-dialog"]');
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

  describe('数据看板 Workflow 添加 (UJ-04, UJ-10)', () => {
    const DASHBOARD_ID = `${E2E_DASHBOARD_PREFIX}-wf`;
    const WORKFLOW_ID = `${E2E_DASHBOARD_PREFIX}-user-wf`;
    const WIDGET_TITLE = 'E2E Workflow Tile';
    let mainWindow: string;
    let connectionId: string;
    let widgetId: string;

    before(async () => {
      mainWindow = await browser.getWindowHandle();
      await browser.pause(300);
      connectionId = await getSeededConnectionId();

      try {
        await invokeBackend('workflow_delete', { workflowId: WORKFLOW_ID });
      } catch {
        /* ok */
      }

      await cleanupDashboard(DASHBOARD_ID);
      await createEmptyDashboard(DASHBOARD_ID, 'E2E Workflow Board');

      await invokeBackend('workflow_save', {
        workflow: {
          id: WORKFLOW_ID,
          name: 'E2E User Workflow',
          description: 'Dashboard E2E user workflow',
          variables: [],
          connection: connectionId,
          visibility: 'user',
          steps: [
            {
              type: 'query',
              id: 'q1',
              sql: "SELECT 'X' AS label, 42 AS value",
              connection: connectionId,
            },
          ],
        },
      });

      const created = await invokeBackend<{ widget: { id: string } }>('create_widget_from_workflow', {
        params: {
          dashboardId: DASHBOARD_ID,
          workflowId: WORKFLOW_ID,
          title: WIDGET_TITLE,
          viewMode: 'chart',
          chartConfig: chartConfig('label', ['value']),
        },
      });
      widgetId = created.widget.id;
    });

    after(async () => {
      try {
        await browser.switchToWindow(mainWindow);
        await cleanupDashboard(DASHBOARD_ID);
        await invokeBackend('workflow_delete', { workflowId: WORKFLOW_ID });
      } catch {
        /* ok */
      }
      await closeExtraWindows(mainWindow);
    });

    it('UJ-04: 从 Workflow 创建组件并显示磁贴', async () => {
      await openDashboardFromMain(mainWindow, DASHBOARD_ID);

      await browser.waitUntil(async () => (await $$('[data-testid="dashboard-tile"]')).length >= 1, {
        timeout: 10000,
        timeoutMsg: '等待 workflow widget tile',
      });

      const body = await $('body').getText();
      expect(body).toContain(WIDGET_TITLE);

      const dash = await invokeBackend<{ widgets: { id: string; workflowId: string }[] }>(
        'get_dashboard',
        { id: DASHBOARD_ID },
      );
      const widget = dash.widgets.find((w) => w.id === widgetId);
      expect(widget?.workflowId).toBe(WORKFLOW_ID);
    });

    it('UJ-10: 被看板引用的 Workflow 删除应失败', async () => {
      const err = await invokeBackendExpectError('workflow_delete', { workflowId: WORKFLOW_ID });
      expect(err.toLowerCase()).toMatch(/referenced|引用|workflow/);
    });
  });
});
