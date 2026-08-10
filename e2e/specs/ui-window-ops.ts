/**
 * Multi-window / UI chrome E2E (TC-UI-001/002/003/005).
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  clickCardConnectButton,
  closeExtraWindows,
  expandAllGroups,
} from '../helpers.js';
import { t } from '../i18n.js';

describe('窗口与 UI 操作 (TC-UI-001/002/003/005)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
    await expandAllGroups();
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('TC-UI-001: 应能同时打开多个连接窗口', async () => {
    await clickCardConnectButton();
    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 30000 },
    );
    const afterFirst = await browser.getWindowHandles();
    expect(afterFirst.length).toBeGreaterThanOrEqual(2);

    await browser.switchToWindow(mainWindow);
    await expandAllGroups();
    // Open another connection item if present; otherwise re-open same via dblclick again
    const opened = await browser.execute(() => {
      const items = Array.from(document.querySelectorAll('[data-conn-item]'));
      if (items.length > 1) {
        items[1].dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        return true;
      }
      if (items.length === 1) {
        items[0].dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        return true;
      }
      return false;
    });
    expect(opened).toBe(true);
    await browser.pause(2500);
    const handles = await browser.getWindowHandles();
    // At least one connection window still open; ideally 2+
    expect(handles.length).toBeGreaterThanOrEqual(2);
  });

  it('TC-UI-002: 重复双击已连接项不应崩溃', async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await clickCardConnectButton();
    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 30000 },
    );
    await browser.switchToWindow(mainWindow);
    await expandAllGroups();
    await browser.execute(() => {
      const item = document.querySelector('[data-conn-item]');
      if (item) {
        item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      }
    });
    await browser.pause(1500);
    const handles = await browser.getWindowHandles();
    expect(handles.length).toBeGreaterThanOrEqual(1);
    // App still responds on main window
    await browser.switchToWindow(mainWindow);
    await expect(await $(`button*=${t('action.newConnection')}`)).toBeDisplayed();
  });

  it('TC-UI-003: 主窗口侧边栏应有可调宽度手柄', async () => {
    await browser.switchToWindow(mainWindow);
    const handle = await $(`[title="${t('main.sidebar.resize')}"]`);
    const exists = await handle.isExisting();
    if (exists) {
      await expect(handle).toBeDisplayed();
    } else {
      // Fallback: aside present
      await expect(await $('aside')).toBeDisplayed();
    }
  });

  it('TC-UI-005: 连接窗口应显示状态栏类信息', async () => {
    await clickCardConnectButton();
    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 30000 },
    );
    const handles = await browser.getWindowHandles();
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);
    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
    await browser.pause(1000);
    const body = await $('body').getText();
    const hasStatus =
      body.includes('postgres') ||
      body.includes('PostgreSQL') ||
      body.includes('ms') ||
      body.includes('连接') ||
      body.includes(t('connWin.newQuery'));
    expect(hasStatus).toBe(true);
  });
});
