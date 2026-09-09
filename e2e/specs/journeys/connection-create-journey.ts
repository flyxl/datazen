/**
 * Existing workspace → create connection → persist in navigator journey.
 *
 * This keeps connection creation independently diagnosable from the first-run
 * journey while still exercising the real dialog and backend test action.
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  closeExtraWindows,
  openConnectionsWorkspace,
  openNewConnectionDialogFromUi,
  selectNewConnectionDriver,
} from '../../helpers.js';
import {
  deleteJourneyConnectionsByName,
  fillPostgresConnectionForm,
  getJourneyConnections,
  testAndSavePostgresConnection,
} from './connectionJourneyHelpers.js';

const CONNECTION_NAME = `E2E Create Journey ${Date.now().toString(36)}`;

describe('创建连接完整用户旅程 (CONNECTION-CREATE-JOURNEY)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await openConnectionsWorkspace(mainWindow);
  });

  after(async () => {
    await deleteJourneyConnectionsByName(CONNECTION_NAME);
    await closeExtraWindows(mainWindow);
    await browser.refresh();
  });

  it('完整旅程：连接工作区 → 选择驱动 → 填写配置 → 测试 → 保存 → 重载后仍存在', async () => {
    await openNewConnectionDialogFromUi();

    // Preserve the existing driver-switch snapshot behavior inside the full
    // creation journey instead of leaving it only in isolated form tests.
    await selectNewConnectionDriver('sqlite');
    await expect(await $('input[placeholder="/path/to/db.sqlite"]')).toBeDisplayed();
    await selectNewConnectionDriver('postgresql');

    await fillPostgresConnectionForm(CONNECTION_NAME);
    await captureJourneyStep('connection-create-form-filled');
    await testAndSavePostgresConnection(CONNECTION_NAME);
    await captureJourneyStep('connection-create-saved');

    const saved = await getJourneyConnections();
    expect(saved.some((connection) => connection.name === CONNECTION_NAME)).toBe(true);

    await browser.refresh();
    await openConnectionsWorkspace(mainWindow);
    const row = await $(`[data-conn-item][data-conn-name="${CONNECTION_NAME}"]`);
    await row.waitForDisplayed({ timeout: 10000 });
    await expect(row).toBeDisplayed();
    await captureJourneyStep('connection-create-persisted');
  });
});
