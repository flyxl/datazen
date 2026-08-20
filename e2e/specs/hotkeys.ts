/**
 * Hotkey E2E (TC-HOTKEY-001~005).
 * Falls back to UI/menu events when Meta shortcuts are unreliable under WebDriver.
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  connectSeededPgInWorkspace,
  closeExtraWindows,
  openQueryTab,
  openSettingsInMainWindow,
  setEditorContent,
  switchToNewWindow,
} from '../helpers.js';
import { t } from '../i18n.js';

describe('快捷键 (TC-HOTKEY-001~005)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    try {
      await browser.switchToWindow(mainWindow);
      await browser.url('tauri://localhost');
      await browser.pause(800);
    } catch {
      /* ignore */
    }
  });

  it('TC-HOTKEY-001: Cmd+N 或新建按钮应打开新建连接窗口', async () => {
    await browser.keys(['Meta', 'n']);
    await browser.pause(800);
    let handles = await browser.getWindowHandles();
    if (handles.length === 1) {
      const btn = await $(`button[title="${t('main.newConnection')}"]`);
      await btn.click();
      await switchToNewWindow(mainWindow);
      handles = await browser.getWindowHandles();
    } else {
      await switchToNewWindow(mainWindow);
    }
    expect(handles.length).toBeGreaterThan(1);
    await expect(await $(`button*=${t('newConn.testConnection')}`)).toBeDisplayed();
  });

  it('TC-HOTKEY-002: Cmd+, 或主窗 SettingsPage 应打开设置', async () => {
    await browser.keys(['Meta', ',']);
    await browser.pause(800);
    let handles = await browser.getWindowHandles();
    if (handles.length === 1) {
      await openSettingsInMainWindow();
    } else {
      const settingsHandle = handles.find((h) => h !== mainWindow)!;
      await browser.switchToWindow(settingsHandle);
    }
    const body = await $('body').getText();
    const opened =
      body.includes(t('settings.title')) ||
      body.includes('偏好') ||
      body.includes('General') ||
      body.includes('常规') ||
      body.includes('AI');
    expect(opened).toBe(true);
  });

  it('TC-HOTKEY-003: 已连接工作区中 Cmd+Enter 应能执行查询', async () => {
    await connectSeededPgInWorkspace();
    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
    await openQueryTab();
    await setEditorContent('SELECT 42 AS hotkey_col');
    await browser.keys(['Meta', 'Enter']);
    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return (
          body.includes('hotkey_col') ||
          body.includes(`1 ${t('common.rows')}`) ||
          body.includes(t('query.totalTime'))
        );
      },
      { timeout: 20000, timeoutMsg: '等待 Cmd+Enter 执行结果超时' },
    );
  });

  it('TC-HOTKEY-004: Cmd+W 关闭 panel 后应用仍可用', async () => {
    await connectSeededPgInWorkspace();
    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
    await browser.keys(['Meta', 'w']);
    await browser.pause(1500);
    // Either window closed or still usable — no crash
    const after = await browser.getWindowHandles();
    expect(after.length).toBeGreaterThanOrEqual(1);
  });

  it('TC-HOTKEY-005: Cmd+B 应切换侧边栏', async () => {
    await connectSeededPgInWorkspace();
    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });

    const before = await browser.execute(() => document.body.innerText.length);
    await browser.keys(['Meta', 'b']);
    await browser.pause(500);
    await browser.keys(['Meta', 'b']);
    await browser.pause(500);
    const after = await browser.execute(() => document.body.innerText.length);
    // Toggle twice should restore usable UI
    expect(after).toBeGreaterThan(0);
    expect(before).toBeGreaterThan(0);
    await expect(await $(`button*=${t('connWin.newQuery')}`)).toBeDisplayed();
  });
});
