/** Create a connection from the normal workspace and immediately use it. */
import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  closeExtraWindows,
  dblclickConnByExactName,
  executeSQL,
  openConnectionsWorkspace,
  openNewConnectionDialogFromUi,
  openQueryTab,
  waitForConnectionToolbar,
} from '../../helpers.js';
import {
  deleteJourneyConnectionsByName,
  fillPostgresConnectionForm,
  testAndSavePostgresConnection,
} from './connectionJourneyHelpers.js';

const CONNECTION_NAME = `E2E Create Query ${Date.now().toString(36)}`;

describe('新建连接后执行 SQL 完整用户旅程 (CONNECTION-QUERY-JOURNEY)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await openConnectionsWorkspace(mainWindow);
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await deleteJourneyConnectionsByName(CONNECTION_NAME);
    await browser.refresh();
  });

  it('完整旅程：新建连接 → 测试并保存 → 打开连接 → 新建查询 → 执行 SQL', async () => {
    await openNewConnectionDialogFromUi();
    await fillPostgresConnectionForm(CONNECTION_NAME);
    await testAndSavePostgresConnection(CONNECTION_NAME);
    await captureJourneyStep('connection-query-created');

    expect(await dblclickConnByExactName(CONNECTION_NAME)).toBe(true);
    await waitForConnectionToolbar();
    await expect(await $('[data-testid="home-quick-new-query"]')).toBeDisplayed();
    await captureJourneyStep('connection-query-connected');

    await openQueryTab();
    await executeSQL('SELECT 7 AS created_connection_value');
    const result = await $('[data-testid="result-workspace-table"]');
    await result.waitForDisplayed({ timeout: 15000 });
    const resultText = await result.getText();
    expect(resultText).toContain('created_connection_value');
    expect(resultText).toContain('7');
    await captureJourneyStep('connection-query-result');
  });
});
