import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  connectSeededPgInWorkspace,
  openQueryTab,
  setEditorContent,
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
describe('统一 Tab Bar (UTB-001~UTB-008)', () => {
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

  // ── closing a tab whose query is still running ────────────────
  //
  // The contract is an *order*, not just an outcome: ask the user first, and
  // only after the statement has actually stopped may the tab (and the session
  // behind it) be released. Both directions are asserted below, because a prompt
  // that appears but does not gate anything is indistinguishable from no prompt.

  /** Tab count, read from the tab bar itself rather than from store internals. */
  const tabCount = () =>
    browser.execute(() => document.querySelectorAll('[data-testid="panel-tab"]').length);

  /** Close every tab, leaving the workspace empty for the case that follows. */
  const closeAllTabs = async () => {
    await browser.execute(() => {
      document.querySelectorAll('[data-testid="panel-tab-close"]').forEach((btn) => {
        if (btn instanceof HTMLElement) btn.click();
      });
    });
    await browser.waitUntil(async () => (await tabCount()) === 0, {
      timeout: 15000,
      timeoutMsg: 'tabs did not close while emptying the tab bar',
    });
  };

  /** Start a statement that outlives the test's next few actions. */
  const startLongRunningQuery = async () => {
    await openQueryTab();
    await setEditorContent('SELECT pg_sleep(30)');
    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.waitForDisplayed({ timeout: 10000 });
    await execBtn.click();
    const stopBtn = await $('[data-testid="editor-stop-button"]');
    await stopBtn.waitForDisplayed({
      timeout: 15000,
      timeoutMsg: 'the long-running query never started (stop button never appeared)',
    });
  };

  it('关闭正在执行的 tab 要先确认，确认后先停语句再关闭 (UTB-007)', async () => {
    await closeAllTabs();
    await startLongRunningQuery();
    expect(await tabCount()).toBe(1);

    // The user clicks the tab's X while the statement is still running. In-page
    // clicks are used throughout because the tab close button and the dialog
    // button both disappear as soon as they are acted on, and a protocol-level
    // click on a transient node has wedged the WebDriver session before.
    await browser.execute(() => {
      const btn = document.querySelector('[data-testid="panel-tab-close"]');
      if (btn instanceof HTMLElement) btn.click();
    });

    const ok = await $('[data-testid="confirm-dialog-ok"]');
    await ok.waitForDisplayed({
      timeout: 5000,
      timeoutMsg: 'closing a tab with a running query did not ask the user',
    });
    // Nothing was destroyed to make the prompt appear: the tab is still there,
    // and so is the running statement behind it.
    expect(await tabCount()).toBe(1);
    expect(
      await browser.execute(
        () => document.querySelector('[data-testid="editor-stop-button"]') != null,
      ),
    ).toBe(true);
    await captureJourneyStep('tab-close-running-ask');

    await browser.execute(() => {
      const btn = document.querySelector('[data-testid="confirm-dialog-ok"]');
      if (btn instanceof HTMLElement) btn.click();
    });

    // The tab can only disappear once the store saw the statement reach a
    // terminal state, which is exactly the order this feature is about.
    await browser.waitUntil(async () => (await tabCount()) === 0, {
      timeout: 20000,
      timeoutMsg: 'the tab did not close after the user confirmed stopping the query',
    });
    expect(
      await browser.execute(
        () => document.querySelector('[data-testid="editor-stop-button"]') == null,
      ),
    ).toBe(true);
    await captureJourneyStep('tab-close-running-done');
  });

  it('拒绝关闭正在执行的 tab 时，tab 与语句都必须保留 (UTB-008)', async () => {
    await closeAllTabs();
    await startLongRunningQuery();
    expect(await tabCount()).toBe(1);

    await browser.execute(() => {
      const btn = document.querySelector('[data-testid="panel-tab-close"]');
      if (btn instanceof HTMLElement) btn.click();
    });
    const cancel = await $('[data-testid="confirm-dialog-cancel"]');
    await cancel.waitForDisplayed({ timeout: 5000 });
    await browser.execute(() => {
      const btn = document.querySelector('[data-testid="confirm-dialog-cancel"]');
      if (btn instanceof HTMLElement) btn.click();
    });
    await browser.pause(500);

    // Declining changes nothing at all: same tab, same still-running statement.
    expect(await tabCount()).toBe(1);
    expect(
      await browser.execute(
        () => document.querySelector('[data-testid="editor-stop-button"]') != null,
      ),
    ).toBe(true);
    expect(
      await browser.execute(
        () => document.querySelector('[data-testid="confirm-dialog-ok"]') == null,
      ),
    ).toBe(true);
    await captureJourneyStep('tab-close-running-declined');

    // Leave the suite clean: stop the statement, then close the tab for real.
    await browser.execute(() => {
      const btn = document.querySelector('[data-testid="editor-stop-button"]');
      if (btn instanceof HTMLElement) btn.click();
    });
    await browser.waitUntil(
      async () =>
        browser.execute(() => document.querySelector('[data-testid="editor-stop-button"]') == null),
      { timeout: 20000, timeoutMsg: 'the cancelled query never reported a terminal state' },
    );
    await closeAllTabs();
  });
});
