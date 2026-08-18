import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  clickCardConnectButton,
  clickFirstTable,
  openQueryTab,
  waitForSchemaTreeLoaded,
} from '../helpers.js';

/**
 * Unified Tab Bar architecture tests.
 * Verifies that the ContentView renders panels from panelStore correctly,
 * including tab bar rendering, toolbar adaptation, and cross-panel switching.
 *
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 */
describe('统一 Tab Bar (UTB-001~UTB-006)', () => {
  let mainWindow: string;

  before(async () => {
    let handles = await browser.getWindowHandles();
    const connHandle = handles.find((h) => h.startsWith('connection'));
    mainWindow =
      handles.find((h) => h === 'main') ?? handles.find((h) => !h.startsWith('connection')) ?? '';

    if (connHandle) {
      await browser.switchToWindow(connHandle);
    } else {
      await browser.switchToWindow(mainWindow || handles[0]);
      await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
      await browser.pause(1500);
      await clickCardConnectButton();
      await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
        timeout: 30000,
      });
      handles = await browser.getWindowHandles();
      const newConn =
        handles.find((h) => h.startsWith('connection')) ?? handles.find((h) => h !== mainWindow)!;
      await browser.switchToWindow(newConn);
    }

    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
    await browser.pause(2000);
  });

  it('工具栏应显示新建查询按钮 (UTB-001)', async () => {
    const newQueryBtn = await $(`button*=${t('connWin.newQuery')}`);
    await expect(newQueryBtn).toBeDisplayed();
  });

  it('点击表后应在 tab bar 中显示 tab (UTB-002)', async () => {
    await waitForSchemaTreeLoaded();
    const tableName = await clickFirstTable();
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body).toContain(tableName);
  });

  it('新建查询应创建查询 tab (UTB-003)', async () => {
    await openQueryTab();
    await browser.pause(500);

    const body = await $('body').getText();
    const hasQueryTab = body.includes(t('connWin.newQuery')) || body.includes('Query');
    expect(hasQueryTab).toBe(true);
  });

  it('关闭 tab 后 tab bar 更新 (UTB-004)', async () => {
    const tabsBefore = await browser.execute(() => {
      const closeBtns = document.querySelectorAll('[data-testid="panel-tab-close"]');
      return closeBtns.length;
    });

    if (tabsBefore > 0) {
      await browser.execute(() => {
        const closeBtn = document.querySelector('[data-testid="panel-tab-close"]') as HTMLElement;
        if (closeBtn) closeBtn.click();
      });
      await browser.pause(500);

      const tabsAfter = await browser.execute(() => {
        const closeBtns = document.querySelectorAll('[data-testid="panel-tab-close"]');
        return closeBtns.length;
      });
      expect(tabsAfter).toBeLessThan(tabsBefore);
    }
  });

  it('空状态应显示提示文本 (UTB-005)', async () => {
    await browser.execute(() => {
      const closeBtns = document.querySelectorAll('[data-testid="panel-tab-close"]');
      closeBtns.forEach((btn) => (btn as HTMLElement).click());
    });
    await browser.pause(500);

    const hasEmptyState = await browser.execute(() => {
      return document.body.innerText.includes('⌘N');
    });
    expect(hasEmptyState).toBe(true);
  });

  it('多个 tab 可以来回切换 (UTB-006)', async () => {
    const tableName = await clickFirstTable();
    await browser.pause(500);
    await openQueryTab();
    await browser.pause(500);

    const tabs = await $$('[data-testid="panel-tab"]');
    expect(tabs.length).toBeGreaterThanOrEqual(2);

    if (tabs.length >= 2) {
      await tabs[0].click();
      await browser.pause(300);
      const body1 = await $('body').getText();
      expect(body1).toContain(tableName);
    }
  });
});
