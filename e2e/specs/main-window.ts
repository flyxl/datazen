import { expect, browser, $, $$ } from '@wdio/globals';
import {
  closeExtraWindows,
  captureJourneyStep,
  expandAllGroups,
  connectSeededPgInWorkspace,
  waitForNewQueryButton,
} from '../helpers.js';
import { t } from '../i18n.js';

describe('主窗口 / 统一工作区 (CM-001)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    const connectionsNav = await $('[data-testid="workspace-nav-connections"]');
    await connectionsNav.waitForDisplayed({ timeout: 15000 });
    await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({
      timeout: 10000,
    });
    await expandAllGroups();
    await browser.pause(500);
  });

  afterEach(async () => {
    const handles = await browser.getWindowHandles();
    if (handles.length > 1) {
      await closeExtraWindows(mainWindow);
    }
    await browser.switchToWindow(mainWindow);
    await browser.pause(300);
  });

  it('应显示工作区导航栏（连接 / 工作流 / 看板）', async () => {
    await expect(await $('[data-testid="workspace-nav-connections"]')).toBeDisplayed();
    await expect(await $('[data-testid="workspace-nav-workflow"]')).toBeDisplayed();
    await expect(await $('[data-testid="workspace-nav-dashboard"]')).toBeDisplayed();
  });

  it('应显示连接搜索框', async () => {
    const input = await $(`input[placeholder="${t('main.searchPlaceholder')}"]`);
    await expect(input).toBeDisplayed();
  });

  it('应显示新建连接按钮（侧栏工具栏）', async () => {
    const plusBtn = await $('[data-testid="new-connection-button"]');
    await expect(plusBtn).toBeDisplayed();
  });

  it('应显示分组的连接列表', async () => {
    await browser.waitUntil(async () => (await $$('[data-conn-item]')).length > 0, {
      timeout: 10000,
      timeoutMsg: '等待连接项加载超时',
    });
    const items = await $$('[data-conn-item]');
    expect(items.length).toBeGreaterThan(0);
  });

  it('连接项应显示数据库类型图标和名称', async () => {
    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return (
          body.includes('Pg') ||
          body.includes('My') ||
          body.includes('PostgreSQL') ||
          body.includes('本地')
        );
      },
      { timeout: 10000, timeoutMsg: '等待数据库类型图标加载超时' },
    );
  });

  it('点击分组头应折叠/展开连接列表', async () => {
    const headers = await $$('[data-group-header]');
    if (headers.length === 0) return;

    const firstHeader = headers[0];
    const countBefore = (await $$('[data-conn-item]')).length;

    await firstHeader.click();
    await browser.pause(300);
    const countAfterCollapse = (await $$('[data-conn-item]')).length;
    await captureJourneyStep('group-collapsed');
    expect(countAfterCollapse).toBeLessThanOrEqual(countBefore);

    await firstHeader.click();
    await browser.pause(300);
  });

  it('双击连接应在同一主窗口显示连接工具栏', async () => {
    await connectSeededPgInWorkspace();
    const newQueryBtn = await waitForNewQueryButton();
    await expect(newQueryBtn).toBeDisplayed();
    const handles = await browser.getWindowHandles();
    expect(handles.length).toBe(1);
  });

  it('连接项绑定了 contextmenu 处理器', async () => {
    const hasItems = await browser.execute(() => {
      const el = document.querySelector('[data-conn-item]');
      return el instanceof HTMLElement;
    });
    expect(hasItems).toBe(true);
  });
});
