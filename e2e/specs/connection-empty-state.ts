/**
 * Connection workspace empty-state guidance (select connection / CTAs).
 *
 * Covers the improved ConnectionWorkspaceHome empty states:
 * - connections exist but none is active
 * - primary CTAs open new-connection / import dialogs
 * - after connect, quick actions appear
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import {
  closeExtraWindows,
  closeNewConnectionDialogFromUi,
  connectSeededPgInWorkspace,
  expandAllGroups,
  openConnectionsWorkspace,
  waitForNewConnectionDialog,
} from '../helpers.js';
import { t } from '../i18n.js';

async function ensureConnectionsHome() {
  await openConnectionsWorkspace();
  await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
  await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({
    timeout: 10000,
  });
  await expandAllGroups();
}

describe('连接工作区空状态 (EMPTY)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await ensureConnectionsHome();
    await browser.pause(500);
  });

  afterEach(async () => {
    try {
      if (await $('[data-testid="new-connection-dialog"]').isExisting()) {
        await closeNewConnectionDialogFromUi();
      }
    } catch {
      /* dialog already closed */
    }
    try {
      // Close any residual share/import dialog
      await browser.keys('Escape');
      await browser.pause(200);
    } catch {
      /* ignore */
    }
    const handles = await browser.getWindowHandles();
    if (handles.length > 1) await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await ensureConnectionsHome();
    await browser.pause(300);
  });

  it('EMPTY-001: 有连接但未选中时应显示选择连接引导', async () => {
    await browser.waitUntil(async () => (await $$('[data-conn-item]')).length > 0, {
      timeout: 10000,
      timeoutMsg: '等待侧栏连接项',
    });

    const home = await $('[data-testid="connection-workspace-home"]');
    await home.waitForDisplayed({ timeout: 10000 });
    await expect(home).toBeDisplayed();

    const body = await $('body').getText();
    expect(body).toContain(t('connWin.home.selectConnectionTitle'));
    expect(body).toContain(t('connWin.home.selectConnectionHint'));
    expect(body).toContain(t('connWin.home.selectConnectionTip'));

    await expect(await $('[data-testid="empty-new-connection-button"]')).toBeDisplayed();
    await expect(await $('[data-testid="empty-import-connections-button"]')).toBeDisplayed();

    // Should not show connected quick actions yet
    expect(await $('[data-testid="home-quick-new-query"]').isExisting()).toBe(false);
  });

  it('EMPTY-002: 空状态「新建连接」应打开新建连接弹窗', async () => {
    const btn = await $('[data-testid="empty-new-connection-button"]');
    await btn.waitForDisplayed({ timeout: 10000 });
    await btn.click();
    await waitForNewConnectionDialog();
    await expect(await $('[data-testid="new-connection-dialog"]')).toBeDisplayed();
    await closeNewConnectionDialogFromUi();
  });

  it('EMPTY-003: 空状态「导入连接」应打开导入弹窗', async () => {
    const btn = await $('[data-testid="empty-import-connections-button"]');
    await btn.waitForDisplayed({ timeout: 10000 });
    await btn.click();

    await browser.waitUntil(
      async () => {
        const pathInput = await $('[data-testid="import-data-path"]');
        if (await pathInput.isExisting()) return true;
        const body = await $('body').getText();
        return (
          body.includes(t('connShare.importFormatsHint')) ||
          body.includes(t('connShare.importAction')) ||
          body.includes(t('common.importConnections'))
        );
      },
      { timeout: 10000, timeoutMsg: '等待导入连接弹窗打开' },
    );

    await browser.keys('Escape');
    await browser.pause(400);
  });

  it('EMPTY-004: 连接成功后应显示快捷操作首页', async () => {
    await connectSeededPgInWorkspace();

    const home = await $('[data-testid="connection-workspace-home"]');
    await home.waitForDisplayed({ timeout: 15000 });
    await expect(home).toBeDisplayed();
    await expect(await $('[data-testid="home-quick-new-query"]')).toBeDisplayed();

    const body = await $('body').getText();
    expect(body).toContain(t('connWin.home.quickActions'));
    // Select-connection empty state should be gone
    expect(body).not.toContain(t('connWin.home.selectConnectionTitle'));
  });
});
