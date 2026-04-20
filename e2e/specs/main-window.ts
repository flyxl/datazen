import { expect, browser, $, $$ } from '@wdio/globals';
import { closeExtraWindows } from '../helpers.js';

describe('主窗口 (CM-001)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('button*=新建连接').waitForDisplayed({ timeout: 10000 });
    await browser.pause(1000);
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
  });

  // ── UI 元素 ──────────────────────────────────────────────────────

  it('应显示新建连接按钮', async () => {
    const btn = await $('button*=新建连接');
    await expect(btn).toBeDisplayed();
  });

  it('应显示搜索框', async () => {
    const input = await $('input[placeholder="搜索连接…"]');
    await expect(input).toBeDisplayed();
  });

  it('应显示视图切换按钮（卡片/列表）', async () => {
    const gridBtn = await $('button[title="卡片视图"]');
    const listBtn = await $('button[title="列表视图"]');
    await expect(gridBtn).toBeDisplayed();
    await expect(listBtn).toBeDisplayed();
  });

  it('应显示侧边栏分组面板', async () => {
    const allBtn = await $('button*=全部');
    await expect(allBtn).toBeDisplayed();
  });

  it('状态栏应显示版本号', async () => {
    const statusBar = await $('span.tabular-nums');
    await expect(statusBar).toBeDisplayed();
    const text = await statusBar.getText();
    expect(text).toContain('DataZen');
  });

  it('连接卡片应显示名称和类型', async () => {
    await browser.waitUntil(
      async () => (await $('body').getText()).includes('PostgreSQL'),
      { timeout: 10000, timeoutMsg: '等待连接卡片加载超时' },
    );
    const badge = await $('span*=PostgreSQL');
    await expect(badge).toBeDisplayed();
  });

  it('连接卡片应显示编辑、复制、删除按钮', async () => {
    await expect(await $('button[title="编辑连接"]')).toBeDisplayed();
    await expect(await $('button[title="复制连接"]')).toBeDisplayed();
    await expect(await $('button[title="删除连接"]')).toBeDisplayed();
  });

  it('连接卡片应显示最后连接时间', async () => {
    const text = await $('body').getText();
    expect(text).toContain('最后连接');
  });

  // ── 视图切换 ────────────────────────────────────────────────────

  it('切换到列表视图应改变布局', async () => {
    const listBtn = await $('button[title="列表视图"]');
    await listBtn.click();
    await browser.pause(500);

    // 列表视图下卡片仍可见
    await browser.waitUntil(
      async () => (await $('body').getText()).includes('PostgreSQL'),
      { timeout: 5000 },
    );
    const badge = await $('span*=PostgreSQL');
    await expect(badge).toBeDisplayed();
  });

  it('切换回卡片视图应恢复', async () => {
    const gridBtn = await $('button[title="卡片视图"]');
    await gridBtn.click();
    await browser.pause(500);

    const badge = await $('span*=PostgreSQL');
    await expect(badge).toBeDisplayed();
  });

  // ── 连接/断开流程 ──────────────────────────────────────────────

  it('点击连接按钮应打开连接窗口', async () => {
    // Find the PostgreSQL card specifically to avoid test connections
    const cards = await $$('.group.relative');
    let clicked = false;
    for (const card of cards) {
      const cardText = await card.getText();
      if (!cardText.includes('PostgreSQL')) continue;
      const btns = await card.$$('button');
      for (const btn of btns) {
        const text = (await btn.getText()).trim();
        if (text === '连接') {
          await btn.click();
          clicked = true;
          break;
        }
      }
      if (clicked) break;
    }
    if (!clicked) return;

    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 30000, timeoutMsg: '等待连接窗口打开超时' },
    );
    const handles = await browser.getWindowHandles();
    expect(handles.length).toBeGreaterThan(1);

    const connWin = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWin);
    await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
    await expect(await $('button*=新建查询')).toBeDisplayed();
  });

  it('主窗口连接状态应更新为已连接', async () => {
    await browser.switchToWindow(mainWindow);
    await browser.pause(1000);

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('已连接') || body.includes('断开');
      },
      { timeout: 10000, timeoutMsg: '等待连接状态更新超时' },
    );
  });

  it('点击断开按钮应关闭连接', async () => {
    await browser.switchToWindow(mainWindow);
    const buttons = await $$('button');
    for (const btn of buttons) {
      const text = (await btn.getText()).trim();
      if (text === '断开') {
        await btn.click();
        break;
      }
    }
    await browser.pause(2000);

    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length === 1,
      { timeout: 10000, timeoutMsg: '等待连接窗口关闭超时' },
    );
    const handles = await browser.getWindowHandles();
    expect(handles.length).toBe(1);
  });
});
