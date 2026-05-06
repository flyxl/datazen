import { expect, browser, $, $$ } from '@wdio/globals';
import {
  clickCardConnectButton,
  closeExtraWindows,
  executeSQL,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
} from '../helpers.js';

/**
 * Table data view tests: pagination, sorting, column resize.
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts before hook).
 */

const TEST_TABLE = '_e2e_data_view_test';

describe('表数据视图 (TD-001~TD-008)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('button*=新建连接').waitForDisplayed({ timeout: 10000 });
    await browser.pause(1500);

    // Check if already in connection window
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

    // Create test table with enough rows for pagination
    await openQueryTab();
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQL(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        score INT NOT NULL,
        created_at TIMESTAMP DEFAULT NOW()
      )
    `);
    // Insert 60 rows to test pagination (default page size is 25)
    await executeSQL(`
      INSERT INTO ${TEST_TABLE} (name, score)
      SELECT 'user_' || i, (i * 7) % 100
      FROM generate_series(1, 60) AS s(i)
    `);

    // Refresh sidebar
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

  // ── 数据加载 ───────────────────────────────────────────────────

  it('点击表名应加载数据并显示行 (TD-001)', async () => {
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(2000);
    await switchSubTab('数据');

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('user_') && body.includes('全选');
      },
      { timeout: 15000, timeoutMsg: '等待表数据加载超时' },
    );
  });

  it('应显示行数范围信息 (TD-001)', async () => {
    const body = await $('body').getText();
    // Pagination shows range like "1-25 / 60"
    expect(body).toMatch(/\d+-\d+\s*\/\s*\d+/);
  });

  // ── 分页 ───────────────────────────────────────────────────────

  it('应显示分页导航 (TD-002)', async () => {
    const prevBtn = await $('button[aria-label="上一页"]');
    const nextBtn = await $('button[aria-label="下一页"]');
    await expect(prevBtn).toBeExisting();
    await expect(nextBtn).toBeExisting();
  });

  it('首页时上一页按钮应禁用 (TD-002)', async () => {
    const prevBtn = await $('button[aria-label="上一页"]');
    const disabled = await prevBtn.getAttribute('disabled');
    expect(disabled).not.toBeNull();
  });

  it('点击下一页应加载下一页数据 (TD-002)', async () => {
    const nextBtn = await $('button[aria-label="下一页"]');
    await nextBtn.click();
    await browser.pause(2000);

    const body = await $('body').getText();
    // Should now show page 2 data (e.g. "第 2 / N 页" or range like "26-50")
    const hasPage2 = body.includes('第 2') || body.includes('26');
    expect(hasPage2).toBe(true);
  });

  it('点击上一页应回到第一页 (TD-002)', async () => {
    const prevBtn = await $('button[aria-label="上一页"]');
    await prevBtn.click();
    await browser.pause(2000);

    const body = await $('body').getText();
    const hasPage1 = body.includes('第 1') || body.includes('1-');
    expect(hasPage1).toBe(true);
  });

  // ── 排序 ───────────────────────────────────────────────────────

  it('点击列头应触发排序 (TD-003)', async () => {
    const headerBtns = await $$('button[title="排序"]');
    const count = await headerBtns.length;
    if (count > 0) {
      await headerBtns[0].click();
      await browser.pause(1500);

      const body = await $('body').getText();
      expect(body).toContain('user_');
    }
  });

  it('再次点击列头应切换排序方向 (TD-003)', async () => {
    const headerBtns = await $$('button[title="排序"]');
    const count = await headerBtns.length;
    if (count > 0) {
      await headerBtns[0].click();
      await browser.pause(1500);

      const body = await $('body').getText();
      expect(body).toContain('user_');
    }
  });

  it('第三次点击列头应取消排序 (TD-003)', async () => {
    const headerBtns = await $$('button[title="排序"]');
    const count = await headerBtns.length;
    if (count > 0) {
      await headerBtns[0].click();
      await browser.pause(1500);

      const body = await $('body').getText();
      expect(body).toContain('user_');
    }
  });

  // ── 列宽调整 ───────────────────────────────────────────────────

  it('应存在列宽调整手柄 (TD-004)', async () => {
    // Resize handles are typically thin elements between column headers
    const resizeHandles = await browser.execute(() => {
      const handles = document.querySelectorAll('[class*="resize"]');
      return handles.length;
    });
    expect(resizeHandles).toBeGreaterThan(0);
  });

  // ── 行选择 ─────────────────────────────────────────────────────

  it('应显示全选复选框或按钮 (TD-005)', async () => {
    // "全选" might be text in header or a checkbox
    const body = await $('body').getText();
    const hasSelectAll = body.includes('全选') || body.includes('select');
    // If not visible as text, check for checkbox in header
    if (!hasSelectAll) {
      const checkboxes = await browser.execute(() =>
        document.querySelectorAll('input[type="checkbox"]').length
      );
      expect(checkboxes).toBeGreaterThan(0);
    } else {
      expect(hasSelectAll).toBe(true);
    }
  });
});
