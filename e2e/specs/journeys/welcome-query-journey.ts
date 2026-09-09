/**
 * Welcome → connection → first query user journey.
 *
 * The sample SQLite wizard is not implemented yet, so this covers the
 * currently available first-run path: create a PostgreSQL connection from
 * Welcome, test/save it, connect, and execute the first query.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  clickCardConnectButton,
  closeNewConnectionDialogFromUi,
  closeExtraWindows,
  executeSQL,
  openConnectionsWorkspace,
  openQueryTab,
  waitForConnectionToolbar,
  waitForNewConnectionDialog,
} from '../../helpers.js';
import {
  deleteAllJourneyConnections,
  deleteJourneyConnectionsByName,
  fillPostgresConnectionForm,
  restoreDefaultJourneyConnection,
  testAndSavePostgresConnection,
} from './connectionJourneyHelpers.js';

const JOURNEY_NAME = `E2E Welcome Query ${Date.now().toString(36)}`;

describe('欢迎页→连接→首条查询完整用户旅程 (WELCOME-QUERY-JOURNEY)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await deleteAllJourneyConnections();
    await browser.execute(() => location.reload());
    await $('[data-testid="welcome-page"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    try {
      await closeExtraWindows(mainWindow);
      await deleteJourneyConnectionsByName(JOURNEY_NAME);
      await restoreDefaultJourneyConnection();
      await browser.execute(() => location.reload());
    } catch {
      /* best effort; global E2E lifecycle restores the default connection */
    }
  });

  it('完整旅程：首次安装欢迎页 → 取消建连 → 创建首个连接 → 查看连接 → 执行首条查询', async () => {
    await expect(await $('[data-testid="welcome-page"]')).toBeDisplayed();
    const welcomeText = await $('[data-testid="welcome-page"]').getText();
    expect(welcomeText).toContain(t('welcome.title'));
    expect(welcomeText).toContain(t('welcome.feature.connections.title'));
    await expect(await $('[data-testid="welcome-import-connection"]')).toBeDisplayed();
    await captureJourneyStep('welcome-query-welcome-visible');

    await $('[data-testid="welcome-create-connection"]').click();
    await waitForNewConnectionDialog();
    await closeNewConnectionDialogFromUi();
    await expect(await $('[data-testid="welcome-page"]')).toBeDisplayed();
    await captureJourneyStep('welcome-query-create-cancelled');

    await $('[data-testid="welcome-create-connection"]').click();
    await waitForNewConnectionDialog();
    await fillPostgresConnectionForm(JOURNEY_NAME);
    await testAndSavePostgresConnection(JOURNEY_NAME);
    await captureJourneyStep('welcome-query-connection-tested');

    await openConnectionsWorkspace(mainWindow);
    const workspaceHome = await $('[data-testid="connection-workspace-home"]');
    await workspaceHome.waitForDisplayed({ timeout: 10000 });
    expect(await workspaceHome.getText()).toContain(JOURNEY_NAME);
    await captureJourneyStep('welcome-query-connection-saved');

    expect(await clickCardConnectButton(JOURNEY_NAME)).toBe(true);
    await waitForConnectionToolbar();
    expect(await $('[data-testid="connection-workspace-home"]').getText()).toContain(JOURNEY_NAME);
    await expect(await $('[data-testid="home-quick-new-query"]')).toBeDisplayed();
    await captureJourneyStep('welcome-query-connection-viewed');

    await openQueryTab();
    await executeSQL('SELECT 1 AS first_activation_value');

    const result = await $('[data-testid="result-workspace-table"]');
    await result.waitForDisplayed({ timeout: 15000 });
    const resultText = await result.getText();
    expect(resultText).toContain('first_activation_value');
    expect(resultText).toContain('1');
    await captureJourneyStep('welcome-query-first-query-success');
  });
});
