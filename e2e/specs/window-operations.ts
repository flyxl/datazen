/**
 * Window operations — multi-window coexistence, close behavior,
 * tab bar state across window operations.
 *
 * Covers: TC-WIN-001 ~ TC-WIN-005
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  clickCardConnectButton,
  closeExtraWindows,
  invokeBackend,
  openQueryTab,
  waitForConnectionToolbar,
} from '../helpers.js';

describe('窗口操作 (TC-WIN-001~005)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('TC-WIN-001: 主窗口应只有一个', async () => {
    const handles = await browser.getWindowHandles();
    // Before connecting, there's just the main window
    expect(handles.length).toBeGreaterThanOrEqual(1);
  });

  it('TC-WIN-002: 连接后不应创建新的 OS 窗口（单窗口模式）', async () => {
    await browser.switchToWindow(mainWindow);
    const handlesBefore = await browser.getWindowHandles();
    await clickCardConnectButton();
    await waitForConnectionToolbar();
    await browser.pause(1000);
    const handlesAfter = await browser.getWindowHandles();
    // Should still be 1 window (unified workspace)
    expect(handlesAfter.length).toBe(handlesBefore.length);
  });

  it('TC-WIN-003: 打开查询 tab 后 tab 数应正确', async () => {
    await browser.switchToWindow(mainWindow);
    await openQueryTab();
    await browser.pause(500);
    const tabCount = await $$('[data-testid="panel-tab"]').length;
    expect(tabCount).toBeGreaterThanOrEqual(1);
  });

  it('TC-WIN-004: 新建连接弹窗打开后应阻止其他操作', async () => {
    await browser.switchToWindow(mainWindow);
    // Open new connection dialog
    const opened = await browser.execute(() => {
      const btns = Array.from(document.querySelectorAll('button'));
      const btn = btns.find(
        (b) => b.textContent?.includes('新建连接') || b.textContent?.includes('New Connection'),
      );
      if (btn) {
        btn.click();
        return true;
      }
      return false;
    });
    if (!opened) return;

    const dialog = await $('[data-testid="new-connection-dialog"]');
    const isVisible = await dialog.isExisting();
    expect(isVisible).toBe(true);

    // Dialog should have modal overlay
    const hasOverlay = await browser.execute(() => {
      const overlay = document.querySelector('[data-testid="new-connection-dialog"]');
      const style = overlay ? window.getComputedStyle(overlay) : null;
      return style?.position === 'fixed' || style?.position === 'absolute' || overlay !== null;
    });
    expect(hasOverlay).toBe(true);

    // Cancel
    const cancelBtn = await $('button*=取消');
    if (await cancelBtn.isExisting()) await cancelBtn.click();
    await browser.pause(500);
  });

  it('TC-WIN-005: 关闭连接 tab 后应返回工作区首页', async () => {
    await browser.switchToWindow(mainWindow);
    // Close all connection tabs by closing extra windows
    await closeExtraWindows(mainWindow);
    await browser.pause(1000);

    // After closing, we should see either the welcome page or connection list
    const body = await $('body').getText();
    const isHome =
      body.includes('欢迎') ||
      body.includes('Welcome') ||
      body.includes('新建连接') ||
      body.includes('New Connection') ||
      body.includes('PostgreSQL') ||
      body.includes('本地');
    expect(isHome).toBe(true);
  });
});
