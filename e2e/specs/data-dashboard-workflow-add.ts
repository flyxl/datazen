import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows } from '../helpers.js';
import {
  E2E_DASHBOARD_PREFIX,
  chartConfig,
  cleanupDashboard,
  createEmptyDashboard,
  getSeededConnectionId,
  invokeBackend,
  invokeBackendExpectError,
  openDashboardFromMain,
} from '../helpers/data-dashboard.js';

const DASHBOARD_ID = `${E2E_DASHBOARD_PREFIX}-wf`;
const WORKFLOW_ID = `${E2E_DASHBOARD_PREFIX}-user-wf`;
const WIDGET_TITLE = 'E2E Workflow Tile';

describe('数据看板 Workflow 添加 (UJ-04, UJ-10)', () => {
  let mainWindow: string;
  let connectionId: string;
  let widgetId: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.pause(1000);
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
    await openDashboardFromMain(mainWindow);

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
