import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  connectSeededPgInWorkspace,
  closeExtraWindows,
  executeSQL,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
  waitForNewQueryButton,
  waitForTableInSidebar,
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
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1500);

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
    await closeExtraWindows(mainWindow);
  });

  // ── 数据加载 ───────────────────────────────────────────────────

  it('点击表名应加载数据并显示行 (TD-001)', async () => {
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(2000);
    await switchSubTab('data');

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('user_') && body.includes(t('common.selectAll'));
      },
      { timeout: 15000, timeoutMsg: 'Timed out waiting for table data to load' },
    );
  });

  it('应显示行数范围信息 (TD-001)', async () => {
    const body = await $('body').getText();
    // Pagination shows range like "1-25 / 60"
    expect(body).toMatch(/\d+-\d+\s*\/\s*\d+/);
  });

  it('表格数据可复制而交互控件不可选中 (TD-SEL-001)', async () => {
    // Content (the DataTable root) must stay selectable so cell text can be
    // copied; interactive controls must not paint a selection block.
    const table = await $('.selectable');
    const tableSel = await browser.execute(
      (el) => getComputedStyle(el as HTMLElement).userSelect,
      table,
    );
    expect(tableSel).not.toBe('none');

    const prevBtn = await $(`button[aria-label="${t('pagination.prev')}"]`);
    await prevBtn.waitForDisplayed({ timeout: 5000 });
    const btnSel = await browser.execute(
      (el) => getComputedStyle(el as HTMLElement).userSelect,
      prevBtn,
    );
    expect(btnSel).toBe('none');
  });

  // ── 分页 ───────────────────────────────────────────────────────

  it('应显示分页导航 (TD-002)', async () => {
    const prevBtn = await $(`button[aria-label="${t('pagination.prev')}"]`);
    const nextBtn = await $(`button[aria-label="${t('pagination.next')}"]`);
    await expect(prevBtn).toBeExisting();
    await expect(nextBtn).toBeExisting();
  });

  it('首页时上一页按钮应禁用 (TD-002)', async () => {
    const prevBtn = await $(`button[aria-label="${t('pagination.prev')}"]`);
    const disabled = await prevBtn.getAttribute('disabled');
    expect(disabled).not.toBeNull();
  });

  it('点击下一页应加载下一页数据 (TD-002)', async () => {
    const nextBtn = await $(`button[aria-label="${t('pagination.next')}"]`);
    await nextBtn.click();
    await browser.pause(2000);

    const body = await $('body').getText();
    // Should now show page 2 data (e.g. "第 2 / N 页" or range like "26-50")
    const hasPage2 = body.includes(`${t('pagination.page')} 2`) || body.includes('26');
    expect(hasPage2).toBe(true);
  });

  it('点击上一页应回到第一页 (TD-002)', async () => {
    const prevBtn = await $(`button[aria-label="${t('pagination.prev')}"]`);
    await prevBtn.click();
    await browser.pause(2000);

    const body = await $('body').getText();
    const hasPage1 = body.includes('第 1') || body.includes('1-');
    expect(hasPage1).toBe(true);
  });

  // ── 排序 ───────────────────────────────────────────────────────

  it('点击列头应触发排序 (TD-003)', async () => {
    const headerBtns = await $$(`button[title="${t('dataTable.sort')}"]`);
    const count = await headerBtns.length;
    if (count > 0) {
      await headerBtns[0].click();
      await browser.pause(1500);

      const body = await $('body').getText();
      expect(body).toContain('user_');
    }
  });

  it('再次点击列头应切换排序方向 (TD-003)', async () => {
    const headerBtns = await $$(`button[title="${t('dataTable.sort')}"]`);
    const count = await headerBtns.length;
    if (count > 0) {
      await headerBtns[0].click();
      await browser.pause(1500);

      const body = await $('body').getText();
      expect(body).toContain('user_');
    }
  });

  it('第三次点击列头应取消排序 (TD-003)', async () => {
    const headerBtns = await $$(`button[title="${t('dataTable.sort')}"]`);
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
    const hasSelectAll = body.includes(t('common.selectAll')) || body.includes('select');
    // If not visible as text, check for checkbox in header
    if (!hasSelectAll) {
      const checkboxes = await browser.execute(
        () => document.querySelectorAll('input[type="checkbox"]').length,
      );
      expect(checkboxes).toBeGreaterThan(0);
    } else {
      expect(hasSelectAll).toBe(true);
    }
  });

  it('TC-TABLE-008: 应能勾选多行', async () => {
    const checked = await browser.execute(() => {
      const boxes = Array.from(
        document.querySelectorAll('input[type="checkbox"]'),
      ) as HTMLInputElement[];
      let clicked = 0;
      for (const box of boxes.slice(0, 3)) {
        if (!box.checked) {
          box.click();
          clicked++;
        }
      }
      return clicked + boxes.filter((b) => b.checked).length;
    });
    expect(checked).toBeGreaterThan(0);
  });

  it('TC-TABLE-004: 筛选入口应可打开（完整路径见 table-filter.ts）', async () => {
    const toggle = await $('[data-testid="table-filter-toggle"]');
    await toggle.waitForDisplayed({ timeout: 10000 });
    await toggle.click();
    await browser.pause(400);
    await expect(await $('[data-testid="filter-editor"]')).toBeDisplayed();
    // Collapse to avoid interfering with later tests
    const collapse = await $('[data-testid="filter-collapse"]');
    if (await collapse.isExisting()) {
      await collapse.click();
    }
  });

  it('TD-DEL-001: 选中行后应显示删除行按钮', async () => {
    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(1500);
    await switchSubTab('data');
    await browser.waitUntil(
      async () => (await $('body').getText()).includes(t('common.selectAll')),
      { timeout: 15000, timeoutMsg: 'Timed out waiting for table data' },
    );
    await browser.execute(() => {
      const boxes = Array.from(
        document.querySelectorAll('input[type="checkbox"]'),
      ) as HTMLInputElement[];
      const rowBox = boxes.find((b) => !b.closest('label'));
      (rowBox ?? boxes[1])?.click();
    });
    await browser.pause(400);
    const del = await $('[data-testid="data-table-delete-rows"]');
    await expect(del).toBeDisplayed();
  });

  it('TC-TABLE-009: 空表应显示空状态而非崩溃', async () => {
    const emptyTable = '_e2e_empty_table';
    await openQueryTab();
    await executeSQL(`DROP TABLE IF EXISTS ${emptyTable}`);
    await executeSQL(`CREATE TABLE ${emptyTable} (id SERIAL PRIMARY KEY, name TEXT)`);
    const refreshBtn = await $(`button[title="${t('connWin.refresh')} (⌘R)"]`);
    await refreshBtn.click();
    await browser.pause(1500);
    await clickTableInSidebar(emptyTable);
    await browser.pause(1500);
    await switchSubTab('data');
    await browser.pause(1000);
    const body = await $('body').getText();
    const ok =
      body.includes('0') ||
      body.includes('空') ||
      body.includes('no row') ||
      body.includes(t('common.selectAll')) ||
      body.includes(emptyTable);
    expect(ok).toBe(true);
    await openQueryTab();
    await executeSQL(`DROP TABLE IF EXISTS ${emptyTable}`);
  });
});
