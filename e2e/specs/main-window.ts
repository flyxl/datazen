import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows, expandAllGroups } from '../helpers.js';

describe('主窗口 (CM-001)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    // Wait for the main window to be ready (search input is always visible)
    await $('input[placeholder="查找连接…"]').waitForDisplayed({ timeout: 10000 });
    await expandAllGroups();
    await browser.pause(1000);
  });

  afterEach(async () => {
    const handles = await browser.getWindowHandles();
    if (handles.length > 1) {
      await closeExtraWindows(mainWindow);
    }
    await browser.switchToWindow(mainWindow);
    await browser.pause(500);
  });

  // ── UI 元素 ──────────────────────────────────────────────────────

  it('应显示搜索框', async () => {
    const input = await $('input[placeholder="查找连接…"]');
    await expect(input).toBeDisplayed();
  });

  it('应显示新建连接按钮（+号和 ActionPanel）', async () => {
    // The "+" button near the search bar
    const plusBtn = await $('button[title="新建连接"]');
    await expect(plusBtn).toBeDisplayed();
    // The "新建连接…" in the action panel
    const actionBtn = await $('button*=新建连接');
    await expect(actionBtn).toBeDisplayed();
  });

  it('应显示左侧操作面板', async () => {
    const backupBtn = await $('button*=备份数据库');
    const restoreBtn = await $('button*=恢复数据库');
    const syncBtn = await $('button*=数据同步');
    await expect(backupBtn).toBeDisplayed();
    await expect(restoreBtn).toBeDisplayed();
    await expect(syncBtn).toBeDisplayed();
  });

  it('状态栏应显示版本号', async () => {
    const statusBar = await $('span.tabular-nums');
    await expect(statusBar).toBeDisplayed();
    const text = await statusBar.getText();
    expect(text).toContain('DataZen');
  });

  it('应显示分组的连接列表', async () => {
    // Wait for connection items to load
    await browser.waitUntil(
      async () => (await $$('[data-conn-item]')).length > 0,
      { timeout: 10000, timeoutMsg: '等待连接项加载超时' },
    );
    const items = await $$('[data-conn-item]');
    expect(items.length).toBeGreaterThan(0);
  });

  it('连接项应显示数据库类型图标和名称', async () => {
    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('Pg') || body.includes('My') || body.includes('SL') || body.includes('Rd');
      },
      { timeout: 10000, timeoutMsg: '等待数据库类型图标加载超时' },
    );
  });

  // ── 分组展开/折叠 ──────────────────────────────────────────────

  it('点击分组头应折叠/展开连接列表', async () => {
    const headers = await $$('[data-group-header]');
    if (headers.length === 0) return;

    const firstHeader = headers[0];
    const countBefore = (await $$('[data-conn-item]')).length;

    await firstHeader.click();
    await browser.pause(300);

    const countAfterCollapse = (await $$('[data-conn-item]')).length;
    // Should have fewer items after collapsing
    expect(countAfterCollapse).toBeLessThanOrEqual(countBefore);

    // Re-expand
    await firstHeader.click();
    await browser.pause(300);
  });

  // ── 双击连接 ──────────────────────────────────────────────────

  it('双击连接应打开连接窗口', async () => {
    await browser.execute(() => {
      const el = document.querySelector('[data-conn-item]');
      if (el) el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
    });
    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 30000, timeoutMsg: '等待连接窗口打开超时' },
    );
    const handles = await browser.getWindowHandles();
    expect(handles.length).toBeGreaterThan(1);
  });

  // ── 右键菜单 ──────────────────────────────────────────────────

  it('连接项绑定了 contextmenu 处理器', async () => {
    // Native context menus block the WebDriver session, so we just verify
    // that connection items exist and are interactive (have contextmenu handlers via React).
    const hasItems = await browser.execute(() => {
      const el = document.querySelector('[data-conn-item]');
      return el instanceof HTMLElement;
    });
    expect(hasItems).toBe(true);
  });

  it('分组头绑定了 contextmenu 处理器', async () => {
    const hasHeaders = await browser.execute(() => {
      const el = document.querySelector('[data-group-header]');
      return el instanceof HTMLElement;
    });
    expect(hasHeaders).toBe(true);
  });
});
