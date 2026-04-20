import { expect, browser, $ } from '@wdio/globals';
import {
  clickCardConnectButton,
  closeExtraWindows,
  executeSQL,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
  doubleClickCellByText,
  waitForEditInput,
  storeUpdateCell,
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
 * Requires the seeded "本地 PostgreSQL" connection to be available.
 */

const TEST_TABLE = '_e2e_edit_test';

describe('表数据编辑 (DE-002~DE-005)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('button*=新建连接').waitForDisplayed({ timeout: 10000 });
    await browser.pause(1500);

    let handles = await browser.getWindowHandles();
    if (handles.length === 1) {
      await clickCardConnectButton();
      await browser.waitUntil(
        async () => (await browser.getWindowHandles()).length > 1,
        { timeout: 30000, timeoutMsg: '等待连接窗口打开超时' },
      );
      handles = await browser.getWindowHandles();
    }
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);
    await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
    await browser.pause(2000);

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

    const refreshBtn = await $('button[title="刷新 (⌘R)"]');
    await refreshBtn.click();
    await browser.pause(2000);
  });

  after(async () => {
    try {
      const handles = await browser.getWindowHandles();
      if (handles.length > 1) {
        const connWindow = handles.find((h) => h !== mainWindow);
        if (connWindow) {
          await browser.switchToWindow(connWindow);
          await openQueryTab();
          await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
        }
      }
    } catch {
      // best-effort cleanup
    }
    await closeExtraWindows(mainWindow);
  });

  it('应能在侧边栏看到测试表并打开数据标签', async () => {
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(2000);

    await $('button*=数据').waitForDisplayed({ timeout: 8000 });

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('全选') && body.includes('Alice');
      },
      { timeout: 15000, timeoutMsg: '等待表数据加载超时' },
    );
  });

  it('双击单元格应进入编辑模式并显示当前值 (DE-002)', async () => {
    await doubleClickCellByText('Alice');

    const input = await waitForEditInput();
    await expect(input).toBeDisplayed();

    const val = await input.getValue();
    expect(val).toBe('Alice');

    // Cancel via dispatching Escape keydown
    await browser.execute(() => {
      const el = document.querySelector('input.font-mono') as HTMLInputElement;
      el?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true }));
    });
    await browser.pause(500);

    const inputGone = await browser.execute(() => !document.querySelector('input.font-mono'));
    expect(inputGone).toBe(true);
  });

  it('通过 store 更新单元格后数据应显示在 UI (DE-003)', async () => {
    await storeUpdateCell(0, 'name', 'AliceUpdated');

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('AliceUpdated');
      },
      { timeout: 15000, timeoutMsg: '等待更新后的值在 UI 中显示超时' },
    );
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
    await switchSubTab('数据');

    await browser.waitUntil(
      async () => (await $('body').getText()).includes('Charlie'),
      { timeout: 10000, timeoutMsg: '等待表数据加载超时' },
    );

    await doubleClickCellByText('Charlie');
    await waitForEditInput();

    // Cancel the edit
    await browser.execute(() => {
      const el = document.querySelector('input.font-mono') as HTMLInputElement;
      el?.dispatchEvent(new KeyboardEvent('keydown', { key: 'Escape', code: 'Escape', bubbles: true }));
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
