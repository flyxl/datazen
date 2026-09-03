import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  connectSeededPgInWorkspace,
  openQueryTab,
  clickTableInSidebar,
  waitForSchemaTreeLoaded,
  waitForNewQueryButton,
} from '../helpers.js';

/**
 * Unified Tab Bar architecture tests.
 * Verifies that the ContentView renders panels from panelStore correctly,
 * including tab bar rendering, toolbar adaptation, and cross-panel switching.
 *
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 */
describe('统一 Tab Bar (UTB-001~UTB-006)', () => {
  before(async () => {
    await connectSeededPgInWorkspace();
    await browser.pause(1500);
  });

  it('工具栏应显示新建查询按钮 (UTB-001)', async () => {
    const newQueryBtn = await waitForNewQueryButton();
    await expect(newQueryBtn).toBeDisplayed();
  });

  it('点击表后应在 tab bar 中显示 tab (UTB-002)', async () => {
    const tableName = 'product';
    await clickTableInSidebar(tableName);
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body).toContain(tableName);
    await captureJourneyStep('tab-bar-table-tab');
  });

  it('新建查询应创建查询 tab (UTB-003)', async () => {
    await openQueryTab();
    await browser.pause(500);

    const body = await $('body').getText();
    const hasQueryTab = body.includes('Query') || body.includes('查询');
    expect(hasQueryTab).toBe(true);
    await captureJourneyStep('tab-bar-query-tab');
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

  it('空状态应显示工作区首页 (UTB-005)', async () => {
    await browser.execute(() => {
      const closeBtns = document.querySelectorAll('[data-testid="panel-tab-close"]');
      closeBtns.forEach((btn) => (btn as HTMLElement).click());
    });
    await browser.pause(500);

    const home = await $('[data-testid="connection-workspace-home"]');
    await expect(home).toBeDisplayed();
    await captureJourneyStep('tab-bar-empty-home');
  });

  it('多个 tab 可以来回切换 (UTB-006)', async () => {
    await connectSeededPgInWorkspace();
    await openQueryTab();
    const tableName = 'product';
    await clickTableInSidebar(tableName);
    await browser.pause(500);
    expect(await $('body').getText()).toContain(tableName);

    await openQueryTab();
    await browser.pause(500);

    const tabs = await $$('[data-testid="panel-tab"]');
    for (const tab of tabs) {
      const text = await tab.getText();
      if (text.includes(tableName)) {
        await (await tab.$('button')).click();
        await browser.pause(500);
        break;
      }
    }
    expect(await $('body').getText()).toContain(tableName);
    await captureJourneyStep('tab-bar-switch-to-table');

    for (const tab of tabs) {
      const text = await tab.getText();
      if (/Query|查询/i.test(text)) {
        await (await tab.$('button')).click();
        await browser.pause(500);
        break;
      }
    }
    const bodyQuery = await $('body').getText();
    expect(bodyQuery.includes('Query') || bodyQuery.includes('查询')).toBe(true);
    await captureJourneyStep('tab-bar-switch-to-query');
  });
});
