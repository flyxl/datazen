/** Connect to an existing connection and browse its user-visible workspace. */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  clickTableInSidebar,
  closeExtraWindows,
  connectSeededPgInWorkspace,
  openConnectionsWorkspace,
  switchSubTab,
  waitForSchemaTreeLoaded,
} from '../../helpers.js';

describe('查看连接完整用户旅程 (CONNECTION-BROWSE-JOURNEY)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await openConnectionsWorkspace(mainWindow);
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('完整旅程：连接列表 → 连接首页 → Schema 树 → 表数据/结构/索引/外键/DDL', async () => {
    const disconnectedHome = await $('[data-testid="connection-workspace-home"]');
    await disconnectedHome.waitForDisplayed({ timeout: 10000 });
    expect(await disconnectedHome.getText()).toContain(t('connWin.home.selectConnectionTitle'));
    await captureJourneyStep('connection-browse-list-home');

    await connectSeededPgInWorkspace();
    const connectedHome = await $('[data-testid="connection-workspace-home"]');
    await connectedHome.waitForDisplayed({ timeout: 15000 });
    expect(await connectedHome.getText()).toContain('本地 PostgreSQL');
    expect(await connectedHome.getText()).toContain(t('connWin.home.status.connected'));
    await expect(await $('[data-testid="home-quick-new-query"]')).toBeDisplayed();
    await captureJourneyStep('connection-browse-connected-home');

    await waitForSchemaTreeLoaded();
    const tableName = 'product';
    await clickTableInSidebar(tableName);
    const dataTab = await $('[data-testid="sub-tab-data"]');
    await dataTab.waitForDisplayed({ timeout: 15000 });
    await expect(dataTab).toBeDisplayed();
    await captureJourneyStep('connection-browse-table-data');

    for (const tabId of ['structure', 'indexes', 'foreignKeys', 'ddl']) {
      await switchSubTab(tabId);
      await expect(await $(`[data-testid="sub-tab-${tabId}"]`)).toBeDisplayed();
      await captureJourneyStep(`connection-browse-${tabId}`);
    }

    expect(await $('body').getText()).toContain(tableName);
    await switchSubTab('data');
    await captureJourneyStep('connection-browse-return-data');
  });
});
