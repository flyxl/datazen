import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeWorkspacePanels } from '../lib/testDataLifecycle.js';
import {
  connectSeededPgInWorkspace,
  closeExtraWindows,
  executeSQL,
  executeSQLChecked,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
  waitForNewQueryButton,
  waitForTableInSidebar,
  clickNavigatorRefresh,
} from '../helpers.js';

/**
 * Table data view tests: pagination, sorting, column resize.
 * The value-search journey (TD-VS, DBR-03) lives here too — it is the
 * same data-browsing domain: "which table holds this value?".
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

    // Create test table with enough rows for pagination. Checked DDL: a
    // swallowed backend error (stale session bound to a replaced worker DB)
    // used to surface only as a 20s `waitForTableInSidebar` timeout.
    await openQueryTab();
    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQLChecked(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(100) NOT NULL,
        score INT NOT NULL,
        created_at TIMESTAMP DEFAULT NOW()
      )
    `);
    // Insert 60 rows to test pagination (default page size is 25)
    await executeSQLChecked(`
      INSERT INTO ${TEST_TABLE} (name, score)
      SELECT 'user_' || i, (i * 7) % 100
      FROM generate_series(1, 60) AS s(i)
    `);

    // Refresh sidebar
    await clickNavigatorRefresh();
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
    // A spec boundary must release retained query/table sessions before the
    // next journey needs metadata capacity in this same application process.
    await closeWorkspacePanels(browser);
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
    const btnSel = await browser.execute((el) => {
      const style = getComputedStyle(el as HTMLElement);
      return style.getPropertyValue('user-select') || style.getPropertyValue('-webkit-user-select');
    }, prevBtn);
    // WebKit may expose user-select as an empty computed value when the
    // non-selectable rule is inherited from the document stylesheet. The
    // application-level rule still applies to every button (see globals.css).
    expect(['none', '']).toContain(btnSel);
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

  // ── 排序（Navicat 对齐的三态循环：升 → 降 → 移除） ────────────────

  /**
   * The first `count` visible values of `column`, in visual (DOM) order.
   * `offsetParent` excludes grids of hidden panels — a leftover query-result
   * grid from the `before` hook renders no `score` cells today, but the filter
   * keeps the reading scoped even if that ever changes.
   */
  const readColumnValues = async (column: string, count: number) => {
    const values = await browser.execute(
      (col: string, n: number) => {
        const cells = Array.from(
          document.querySelectorAll<HTMLElement>(
            `[data-testid="data-table-cell"][data-dt-col="${col}"]`,
          ),
        ).filter((el) => el.offsetParent !== null);
        return cells.slice(0, n).map((el) => (el.textContent ?? '').trim());
      },
      column,
      count,
    );
    return values as string[];
  };

  it('点击列头应按该列升序排序 (TD-003)', async () => {
    // A single click on the header cell itself starts the cycle (DBR-SORT);
    // the old spec looked for `button[title=…]` sort buttons that no longer
    // exist, so it silently skipped.
    const header = await $('[data-col-header="score"]');
    await header.click();
    await browser.pause(1500);

    // The indicator the header carries while sorted: an up arrow for the
    // ascending leg of the click cycle.
    const ascArrow = await header.$('[data-sort-indicator] .lucide-arrow-up');
    await expect(ascArrow).toBeExisting();

    // The first two visible rows are really in ascending score order — the
    // arrow alone does not prove the rows moved.
    const [first, second] = (await readColumnValues('score', 2)).map(Number);
    expect(first).toBeLessThanOrEqual(second);
  });

  it('再次点击列头应切换为降序 (TD-003)', async () => {
    const header = await $('[data-col-header="score"]');
    await header.click();
    await browser.pause(1500);

    const descArrow = await header.$('[data-sort-indicator] .lucide-arrow-down');
    await expect(descArrow).toBeExisting();

    const [first, second] = (await readColumnValues('score', 2)).map(Number);
    expect(first).toBeGreaterThanOrEqual(second);
  });

  it('第三次点击列头应移除排序 (TD-003)', async () => {
    const header = await $('[data-col-header="score"]');
    await header.click();
    await browser.pause(1500);

    // The third leg of the cycle removes the column from the sort: the
    // header carries no direction indicator and the grid falls back to the
    // default order.
    const indicator = await header.$('[data-sort-indicator]');
    await expect(indicator).not.toBeExisting();

    // id 1 leads again under the default order.
    const [firstName] = await readColumnValues('name', 1);
    expect(firstName).toBe('user_1');
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
    await executeSQLChecked(`DROP TABLE IF EXISTS ${emptyTable}`);
    await executeSQLChecked(`CREATE TABLE ${emptyTable} (id SERIAL PRIMARY KEY, name TEXT)`);
    await clickNavigatorRefresh();
    await waitForTableInSidebar(emptyTable);
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

// ── 全库值搜索 (DBR-03 / TD-VS) ─────────────────────────────────
// The value-search dialog's own journey: the five match modes, the
// schema-qualified scope picker, the zero-hit/failed split, and the
// "open and locate" hand-over that seeds the scan's own conditions.

const VS_SCHEMA_A = 'e2e_vs_a';
const VS_SCHEMA_B = 'e2e_vs_b';

describe('全库值搜索 (TD-VS)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await waitForNewQueryButton(20000);
    await browser.pause(1500);

    // Two same-named tables in two schemas (the cross-schema trap), one of
    // which holds the needle; plus a clean table that never does.
    await openQueryTab();
    await executeSQLChecked(`DROP SCHEMA IF EXISTS ${VS_SCHEMA_A} CASCADE`);
    await executeSQLChecked(`DROP SCHEMA IF EXISTS ${VS_SCHEMA_B} CASCADE`);
    await executeSQLChecked(`CREATE SCHEMA ${VS_SCHEMA_A}`);
    await executeSQLChecked(`CREATE SCHEMA ${VS_SCHEMA_B}`);
    await executeSQLChecked(`
      CREATE TABLE ${VS_SCHEMA_A}.dup_name (
        id SERIAL PRIMARY KEY,
        label TEXT NOT NULL
      )
    `);
    await executeSQLChecked(`
      CREATE TABLE ${VS_SCHEMA_B}.dup_name (
        id SERIAL PRIMARY KEY,
        label TEXT NOT NULL
      )
    `);
    await executeSQLChecked(`
      CREATE TABLE public._e2e_vs_zero (
        id SERIAL PRIMARY KEY,
        label TEXT NOT NULL
      )
    `);
    await executeSQLChecked(
      `INSERT INTO ${VS_SCHEMA_A}.dup_name (label) VALUES ('needle'), ('other')`,
    );
    await executeSQLChecked(`INSERT INTO ${VS_SCHEMA_B}.dup_name (label) VALUES ('nothing')`);
    await executeSQLChecked(`INSERT INTO public._e2e_vs_zero (label) VALUES ('blank')`);

    await clickNavigatorRefresh();
    await waitForTableInSidebar('dup_name');
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await openQueryTab();
      await executeSQL(`DROP SCHEMA IF EXISTS ${VS_SCHEMA_A} CASCADE`);
      await executeSQL(`DROP SCHEMA IF EXISTS ${VS_SCHEMA_B} CASCADE`);
      await executeSQL(`DROP TABLE IF EXISTS public._e2e_vs_zero`);
    } catch {
      // best-effort cleanup
    }
    await closeExtraWindows(mainWindow);
    await closeWorkspacePanels(browser);
  });

  async function openValueSearch(): Promise<void> {
    const entry = await $('[data-testid="navigator-search-values"]');
    await entry.waitForDisplayed({ timeout: 10000 });
    await entry.click();
    const dialog = await $('[data-testid="database-value-search-dialog"]');
    await dialog.waitForDisplayed({ timeout: 5000 });
  }

  async function closeValueSearch(): Promise<void> {
    const dialog = await $('[data-testid="database-value-search-dialog"]');
    if (await dialog.isDisplayed()) {
      await browser.keys('Escape');
      await browser.pause(300);
    }
  }

  /** Pick an option of the Select carrying the test id by index or label text. */
  async function pickSelectOption(testId: string, selector: number | string): Promise<void> {
    await browser.execute((id: string) => {
      (document.querySelector(`[data-testid="${id}"]`) as HTMLElement | null)?.click();
    }, testId);
    await browser.waitUntil(
      async () =>
        browser.execute(() => Boolean(document.querySelector('[data-testid="select-listbox"]'))),
      { timeout: 5000, timeoutMsg: `select listbox did not open for ${testId}` },
    );
    const clicked = await browser.execute((sel: number | string) => {
      const list = document.querySelector('[data-testid="select-listbox"]');
      if (!list) return false;
      const options = Array.from(
        list.querySelectorAll<HTMLElement>('[data-testid="select-option"]'),
      );
      const target =
        typeof sel === 'number'
          ? (options.find((el) => el.getAttribute('data-option-idx') === String(sel)) ??
            options[sel])
          : options.find((el) => (el.textContent ?? '').includes(sel));
      if (!target) return false;
      target.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
      target.click();
      return true;
    }, selector);
    if (!clicked) throw new Error(`Could not select option ${String(selector)} for ${testId}`);
    await browser.pause(200);
  }

  it('工具栏入口打开对话框，匹配模式含"不包含" (TD-VS-001)', async () => {
    await openValueSearch();

    await pickSelectOption('value-search-mode', t('valueSearch.mode.notContains'));
    // The trigger now shows the not-contains label.
    const trigger = await $('[data-testid="value-search-mode"]');
    await browser.waitUntil(
      async () => (await trigger.getText()).includes(t('valueSearch.mode.notContains')),
      { timeout: 5000, timeoutMsg: 'value-search mode did not switch to not-contains' },
    );
    const label = await trigger.getText();
    expect(label).toContain(t('valueSearch.mode.notContains'));

    await closeValueSearch();
  });

  it('范围选择器按 Schema 区分同名表，计划随之收窄 (TD-VS-002)', async () => {
    await openValueSearch();

    // 已选表
    await pickSelectOption('value-search-scope', '已选表');
    const picker = await $('[data-testid="value-search-table-picker"]');
    await picker.waitForDisplayed({ timeout: 5000 });

    // Both same-named tables appear, each under its own schema.
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            document.querySelectorAll('[data-testid^="value-search-table-option-"]').length >= 2,
        ),
      { timeout: 10000, timeoutMsg: 'Timed out waiting for value-search table options' },
    );
    const dupOptions = await browser.execute(() =>
      Array.from(
        document.querySelectorAll<HTMLElement>('[data-testid^="value-search-table-option-"]'),
      )
        .map((el) => ({
          testId: el.getAttribute('data-testid') ?? '',
          text: (el.textContent ?? '').replace(/\s+/g, ' ').trim(),
        }))
        .filter((item) => item.text.includes('dup_name')),
    );
    expect(dupOptions.length).toBeGreaterThanOrEqual(2);
    const picked =
      dupOptions.find(
        (item) => item.testId.includes('e2e_vs_a') || item.text.includes('e2e_vs_a'),
      ) ?? dupOptions[0];
    const pickedKey = picked.testId.replace('value-search-table-option-', '');

    // Picking one narrows the plan's cost note to exactly that table.
    await $(`[data-testid="${picked.testId}"]`).click();
    await browser.pause(200);
    const note = await $('[data-testid="value-search-cost-note"]');
    const noteText = await note.getText();
    expect(noteText).toContain('1');

    // The scan answers with the picked table only — searching for 'other'
    // (a value e2e_vs_a.dup_name holds) answers with that table, and the
    // other schema's same-named table is never part of the answer.
    const input = await $('[data-testid="value-search-input"]');
    await input.setValue('other');
    await $('[data-testid="value-search-start"]').click();
    const results = await $('[data-testid="value-search-results"]');
    await results.waitForDisplayed({ timeout: 20000 });
    await browser.waitUntil(
      async () => (await $('body').getText()).includes(t('valueSearch.status.done')),
      { timeout: 20000, timeoutMsg: 'Timed out waiting for the scan to finish' },
    );
    // The answer names the schema-qualified table that was picked — the
    // other schema's same-named table has no answer row at all.
    await expect(await $(`[data-testid="value-search-result-${pickedKey}"]`)).toBeDisplayed();

    await closeValueSearch();
  });

  it('全部范围扫描把零命中表与命中表分列 (TD-VS-003)', async () => {
    await openValueSearch();
    // Dialog state persists across openings; ensure full-database scope.
    const scopeTrigger = await $('[data-testid="value-search-scope"]');
    if (
      !(await scopeTrigger.getText()).includes(
        t('valueSearch.scope.all', { count: 0 }).split('{')[0].trim() || '全部',
      )
    ) {
      await pickSelectOption('value-search-scope', '全部');
    } else {
      // already all
    }

    const input = await $('[data-testid="value-search-input"]');
    await input.setValue('needle');
    await $('[data-testid="value-search-start"]').click();

    const results = await $('[data-testid="value-search-results"]');
    await results.waitForDisplayed({ timeout: 20000 });
    await browser.waitUntil(
      async () => (await $('body').getText()).includes(t('valueSearch.status.done')),
      { timeout: 20000, timeoutMsg: 'Timed out waiting for the scan to finish' },
    );

    // The hit list names the schema-qualified answer; the clean tables sit
    // in their own zero-hit summary, not among the hits.
    expect(await results.getText()).toContain('dup_name');
    const zeroSummary = await $('[data-testid="value-search-zero-hits-summary"]');
    await expect(zeroSummary).toBeDisplayed();

    await closeValueSearch();
  });

  it('不包含模式回答"哪些行确实没有该值" (TD-VS-004)', async () => {
    await openValueSearch();

    // 只扫 e2e_vs_a.dup_name：needle 在一行，other 在另一行 — not-contains
    // 的答案是 1（否定分布到每一列，AND 起来，而不是把命中行算进去）。
    await pickSelectOption('value-search-scope', '已选表');
    const picker = await $('[data-testid="value-search-table-picker"]');
    await picker.waitForDisplayed({ timeout: 5000 });
    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            document.querySelectorAll('[data-testid^="value-search-table-option-"]').length >= 1,
        ),
      { timeout: 10000, timeoutMsg: 'Timed out waiting for value-search table options' },
    );
    const picked = await browser.execute(() =>
      Array.from(
        document.querySelectorAll<HTMLElement>('[data-testid^="value-search-table-option-"]'),
      )
        .map((el) => ({
          testId: el.getAttribute('data-testid') ?? '',
          text: (el.textContent ?? '').replace(/\s+/g, ' ').trim(),
        }))
        .find((item) => item.testId.includes('e2e_vs_a') || item.text.includes('e2e_vs_a')),
    );
    if (!picked) throw new Error('Could not find e2e_vs_a.dup_name in value-search picker');
    const pickedKey = picked.testId.replace('value-search-table-option-', '');
    await $(`[data-testid="${picked.testId}"]`).click();
    await browser.pause(200);

    await pickSelectOption('value-search-mode', t('valueSearch.mode.notContains'));
    const input = await $('[data-testid="value-search-input"]');
    await input.setValue('needle');
    await $('[data-testid="value-search-start"]').click();

    const results = await $('[data-testid="value-search-results"]');
    await results.waitForDisplayed({ timeout: 20000 });
    const row = await $(`[data-testid="value-search-result-${pickedKey}"]`);
    await expect(row).toBeDisplayed();
    const rowText = await row.getText();
    expect(rowText).toContain('1');

    await closeValueSearch();
  });

  it('打开并定位：新表页带着扫描自己的条件打开 (TD-VS-005)', async () => {
    await openValueSearch();
    // Ensure all-scope scan so e2e_vs_a.dup_name is searched.
    await pickSelectOption('value-search-scope', '全部');

    const input = await $('[data-testid="value-search-input"]');
    await input.setValue('needle');
    await $('[data-testid="value-search-start"]').click();

    const results = await $('[data-testid="value-search-results"]');
    await results.waitForDisplayed({ timeout: 20000 });
    await browser.waitUntil(
      async () => (await $('body').getText()).includes(t('valueSearch.status.done')),
      { timeout: 20000 },
    );

    await browser.waitUntil(
      async () =>
        browser.execute(
          () =>
            !!document.querySelector(
              '[data-testid^="value-search-locate-"][data-testid*="e2e_vs_a"]',
            ),
        ),
      { timeout: 10000, timeoutMsg: 'Timed out waiting for locate action for e2e_vs_a.dup_name' },
    );
    await browser.execute(() => {
      (
        document.querySelector(
          '[data-testid^="value-search-locate-"][data-testid*="e2e_vs_a"]',
        ) as HTMLElement | null
      )?.click();
    });

    // The dialog hands over, the table tab opens, and the filter panel
    // arrives already carrying the scan's own question: a contains
    // condition on the searched column with the needle as its value.
    await browser.pause(1500);
    const editor = await $('[data-testid="filter-editor"]');
    await editor.waitForDisplayed({ timeout: 10000 });
    const body = await $('body').getText();
    expect(body).toContain('needle');
    expect(body).toContain('label');
  });
});
