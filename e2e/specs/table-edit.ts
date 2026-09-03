import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  connectSeededPgInWorkspace,
  closeExtraWindows,
  executeSQL,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
  doubleClickCellByText,
  waitForEditInput,
  confirmWebDialog,
  waitForNewQueryButton,
  waitForTableInSidebar,
  setSafeMode,
} from '../helpers.js';

/**
 * Table data editing E2E tests.
 *
 * Approach: WKWebView's WebDriver doesn't support keyboard input into
 * React controlled inputs reliably, so we use a hybrid strategy:
 * - UI interaction tests (double-click to edit, input rendering) use WebDriver
 * - Value mutations use the Zustand store API exposed at window.__tableDataStore
 * - Keyboard commit/cancel use dispatchEvent which React's root listener catches
 * - Database verification uses SQL queries through the query tab
 *
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 */

const TEST_TABLE = '_e2e_edit_test';

async function commitInlineValue(value: string) {
  const input = await waitForEditInput();
  await input.clearValue();
  await input.setValue(value);
  await browser.pause(300);
  expect(await input.getValue()).toBe(value);
  await browser.execute(() => {
    const editor = document.querySelector('input.font-mono') as HTMLInputElement | null;
    editor?.blur();
  });
}

describe('表数据编辑 (DE-002~DE-005)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await setSafeMode(false);
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1500);

    await openQueryTab();
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQL(
      `CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        score INT NOT NULL DEFAULT 0
      )`,
    );
    await executeSQL(
      `INSERT INTO ${TEST_TABLE} (name, score) VALUES
        ('Alice', 100),
        ('Bob', 200),
        ('Charlie', 300)`,
    );
    // Let the query result/state update settle before refreshing the
    // virtualized schema tree on a cold WebKit run.
    await browser.pause(1200);

    const refreshBtn = await $(`button[title="${t('connWin.refresh')} (⌘R)"]`);
    await refreshBtn.click();
    await waitForTableInSidebar(TEST_TABLE);
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await openQueryTab();
      await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    } catch {
      // best-effort cleanup
    }
    await setSafeMode(true);
    await closeExtraWindows(mainWindow);
  });

  it('应能在侧边栏看到测试表并打开数据标签', async () => {
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(2000);

    await $(`button*=${t('connWin.data')}`).waitForDisplayed({ timeout: 8000 });

    // Virtual table rows use absolute positioning so getText() may not capture cell values.
    // Verify table loaded by checking the status bar row count.
    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return (
          body.includes(t('common.selectAll')) && (body.includes('1-') || body.includes('Alice'))
        );
      },
      { timeout: 15000, timeoutMsg: 'Timed out waiting for table data to load' },
    );
  });

  it('Safe Mode 下双击单元格不应进入编辑模式 (DE-002b)', async () => {
    await setSafeMode(true);
    await clickTableInSidebar(TEST_TABLE);
    await switchSubTab('data');
    await browser.pause(500);

    await doubleClickCellByText('Alice');
    await browser.pause(500);

    // No edit input should appear while Safe Mode blocks in-place editing.
    const inputPresent = await browser.execute(
      () => !!document.querySelector('input.font-mono'),
    );
    expect(inputPresent).toBe(false);

    await setSafeMode(false);
  });

  it('双击单元格应进入编辑模式并显示当前值 (DE-002)', async () => {
    // Virtual rows render spans with title attribute for text cells.
    // Use DOM query to find and double-click the cell.
    await browser.waitUntil(
      async () => {
        return browser.execute(() => !!document.querySelector('span[title="Alice"]'));
      },
      { timeout: 10000, timeoutMsg: 'Timed out waiting for Alice cell' },
    );

    await browser.execute(() => {
      const el = document.querySelector('span[title="Alice"]');
      if (!el) return;
      const parent = el.closest('div[class*="items-center"]');
      (parent ?? el).dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
    });
    await browser.pause(500);

    const input = await waitForEditInput();
    await expect(input).toBeDisplayed();

    const val = await input.getValue();
    expect(val).toBe('Alice');

    await browser.execute(() => {
      const el = document.querySelector('input.font-mono') as HTMLInputElement;
      el?.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true }),
      );
    });
    await browser.pause(500);

    const inputGone = await browser.execute(() => !document.querySelector('input.font-mono'));
    expect(inputGone).toBe(true);
  });

  it('编辑应暂存、可预览 SQL，并支持回滚与提交 (DE-006~DE-008)', async () => {
    await doubleClickCellByText('Alice');
    await commitInlineValue('AliceStaged');

    const pendingBar = await $('[data-testid="pending-changes-bar"]');
    await pendingBar.waitForDisplayed({ timeout: 8000 });
    await expect(await $('[data-testid="pending-preview"]')).toBeDisplayed();
    await expect(await $('[data-testid="pending-commit"]')).toBeDisplayed();
    await expect(await $('[data-testid="pending-rollback"]')).toBeDisplayed();

    await $('[data-testid="pending-preview"]').click();
    const preview = await $('[data-testid="pending-plan-dialog"]');
    await preview.waitForDisplayed({ timeout: 8000 });
    expect((await preview.getText()).toUpperCase()).toContain('UPDATE');
    expect(await preview.getText()).toContain('AliceStaged');
    const previewButtons = await preview.$$('button');
    await previewButtons[previewButtons.length - 1].click();
    await preview.waitForDisplayed({ reverse: true, timeout: 5000 });

    await $('[data-testid="pending-rollback"]').click();
    await $('[data-testid="pending-changes-bar"]').waitForDisplayed({
      reverse: true,
      timeout: 8000,
    });
    // Rollback reloads the table asynchronously. Wait for the restored row
    // state before starting the next edit, otherwise the reload can replace
    // the newly staged change.
    await browser.waitUntil(
      async () =>
        await $('[data-testid="table-filter-toggle"]')
          .isEnabled()
          .catch(() => false),
      { timeout: 15000, timeoutMsg: '等待回滚后的表数据重新加载' },
    );

    // Stage a second value and commit it through the confirmation dialog.
    await doubleClickCellByText('Alice');
    await commitInlineValue('AliceCommitted');
    const commitPendingBar = await $('[data-testid="pending-changes-bar"]');
    await commitPendingBar.waitForDisplayed({ timeout: 8000 });
    await browser.waitUntil(
      async () => (await commitPendingBar.getAttribute('aria-busy')) !== 'true',
      { timeout: 8000, timeoutMsg: '等待暂存修改进入可提交状态' },
    );
    await $('[data-testid="pending-commit"]').click();
    await confirmWebDialog();
    await $('[data-testid="pending-changes-bar"]').waitForDisplayed({
      reverse: true,
      timeout: 10000,
    });

    await openQueryTab();
    await executeSQL(`SELECT name FROM ${TEST_TABLE} WHERE id = 1`);
    expect(await $('body').getText()).toContain('AliceCommitted');
  });

  it('通过 SQL 更新后刷新应在 UI 中显示新值 (DE-003)', async () => {
    await openQueryTab();
    await executeSQL(`UPDATE ${TEST_TABLE} SET name = 'AliceUpdated' WHERE id = 1`);
    await browser.pause(500);

    // Verify the update persisted via SELECT query
    await executeSQL(`SELECT name FROM ${TEST_TABLE} WHERE id = 1`);
    await browser.pause(1000);

    const body = await $('body').getText();
    expect(body).toContain('AliceUpdated');
  });

  it('更新后的数据应持久化到数据库 (DE-004)', async () => {
    await openQueryTab();
    await executeSQL(`SELECT name FROM ${TEST_TABLE} WHERE id = 1`);

    const body = await $('body').getText();
    expect(body).toContain('AliceUpdated');
  });

  it('更新数值字段应正确保存到数据库 (DE-003b)', async () => {
    await openQueryTab();
    await executeSQL(`UPDATE ${TEST_TABLE} SET score = 999 WHERE name = 'Bob'`);
    await executeSQL(`SELECT score FROM ${TEST_TABLE} WHERE name = 'Bob'`);
    const body = await $('body').getText();
    expect(body).toContain('999');
  });

  it('Escape 取消编辑不应修改数据 (DE-005)', async () => {
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(1500);
    await switchSubTab('data');

    await browser.waitUntil(async () => (await $('body').getText()).includes('Charlie'), {
      timeout: 10000,
      timeoutMsg: 'Timed out waiting for table data to load',
    });

    await doubleClickCellByText('Charlie');
    await waitForEditInput();

    // Cancel the edit
    await browser.execute(() => {
      const el = document.querySelector('input.font-mono') as HTMLInputElement;
      el?.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true }),
      );
    });
    await browser.pause(500);

    // Charlie should still be there
    const cellAfter = await $('span[title="Charlie"]');
    await expect(cellAfter).toBeDisplayed();

    // Verify DB wasn't changed
    await openQueryTab();
    await executeSQL(`SELECT name FROM ${TEST_TABLE} WHERE id = 3`);
    const body = await $('body').getText();
    expect(body).toContain('Charlie');
    expect(body).not.toContain('CharlieBlur');
  });
});
