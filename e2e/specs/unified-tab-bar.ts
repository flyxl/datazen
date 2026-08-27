import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  connectSeededPgInWorkspace,
  openQueryTab,
  clickFirstTable,
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
  before(async () => {
    await connectSeededPgInWorkspace();
    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
    await browser.pause(1000);
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
    await captureJourneyStep('tab-bar-table-tab');
  });

  it('新建查询应创建查询 tab (UTB-003)', async () => {
    await openQueryTab();
    await browser.pause(500);

    const body = await $('body').getText();
    const hasQueryTab = body.includes(t('connWin.newQuery')) || body.includes('Query');
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
      await captureJourneyStep('tab-bar-switch');
    }
  });
});
