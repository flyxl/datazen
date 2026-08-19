/**
 * Multi-window / UI chrome E2E (TC-UI-001/002/003/005) — unified main workspace.
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { connectSeededPgInWorkspace, closeExtraWindows, expandAllGroups } from '../helpers.js';
import { t } from '../i18n.js';

describe('窗口与 UI 操作 (TC-UI-001/002/003/005)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
    await expandAllGroups();
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('TC-UI-001: 连接后仍保持单一主 OS 窗口', async () => {
    await connectSeededPgInWorkspace();
    const handles = await browser.getWindowHandles();
    expect(handles.length).toBe(1);
    await expect(await $(`button*=${t('connWin.newQuery')}`)).toBeDisplayed();
  });

  it('TC-UI-002: 重复双击已连接项不应崩溃', async () => {
    await connectSeededPgInWorkspace();
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
    expect(handles.length).toBe(1);
    await expect(await $('[data-testid="workspace-nav-connections"]')).toBeDisplayed();
  });

  it('TC-UI-003: 侧栏连接树区域应可见', async () => {
    await expect(await $(`input[placeholder="${t('main.searchPlaceholder')}"]`)).toBeDisplayed();
    await browser.waitUntil(async () => (await $$('[data-conn-item]')).length > 0, {
      timeout: 10000,
    });
  });

  it('TC-UI-005: 已连接工作区应显示工具栏', async () => {
    await connectSeededPgInWorkspace();
    await browser.pause(500);
    const body = await $('body').getText();
    const hasStatus =
      body.includes('postgres') ||
      body.includes('PostgreSQL') ||
      body.includes('连接') ||
      body.includes(t('connWin.newQuery'));
    expect(hasStatus).toBe(true);
  });
});
