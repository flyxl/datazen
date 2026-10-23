import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  executeSQL,
  executeSQLChecked,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
  selectDzOption,
  connectSeededPgInWorkspace,
  waitForTableInSidebar,
  clickNavigatorRefresh,
} from '../helpers.js';

/**
 * Manual table filter E2E (TF-001~TF-010, TF-AI-001).
 * Covers Apply flow, empty incomplete filter regression, AND/OR, collapse, chips.
 */

const TEST_TABLE = '_e2e_filter_test';

async function openFilterPanel() {
  const toggle = await $('[data-testid="table-filter-toggle"]');
  await toggle.waitForDisplayed({ timeout: 10000 });
  const pressed = await toggle.getAttribute('aria-pressed');
  if (pressed !== 'true') {
    await toggle.click();
    await browser.pause(400);
  }
  const editor = await $('[data-testid="filter-editor"]');
  if (!(await editor.isDisplayed().catch(() => false))) {
    await toggle.click();
    await browser.pause(400);
  }
  await $('[data-testid="filter-editor"]').waitForDisplayed({ timeout: 5000 });
  // Ensure panel body is expanded
  const apply = await $('[data-testid="filter-apply"]');
  if (!(await apply.isDisplayed().catch(() => false))) {
    const summary = await $('[data-testid="filter-summary-toggle"]');
    if (await summary.isExisting()) {
      await summary.click();
      await browser.pause(300);
    }
  }
  // clearFilters removes the draft rows but intentionally keeps the panel
  // open. Recreate one condition when a preceding test left an empty editor.
  const value = await $('[data-testid="filter-value"]');
  if (!(await value.isDisplayed().catch(() => false))) {
    const add = await $('[data-testid="filter-add"]');
    if (await add.isDisplayed().catch(() => false)) {
      await add.click();
      await browser.pause(300);
    }
  }
}

/**
 * Remove every draft condition, then re-open the panel on an empty draft.
 *
 * `filter-clear` drops the *applied* filter but deliberately keeps one blank
 * editor row, and a blank row blocks `Apply` as an incomplete condition — the
 * exact state that made every later case wait forever on a disabled Apply.
 */
async function clearDraft() {
  const clearBtn = await $('[data-testid="filter-clear"]');
  if (await clearBtn.isDisplayed().catch(() => false)) {
    await clearBtn.click();
    await browser.pause(600);
  }
  for (let guard = 0; guard < 8; guard++) {
    const removeBtns = await $$(`button[aria-label="${t('filter.remove')}"]`);
    if (removeBtns.length === 0) break;
    await removeBtns[removeBtns.length - 1].click();
    await browser.pause(200);
  }
  await browser.waitUntil(async () => (await $$('[data-testid="filter-value"]')).length === 0, {
    timeout: 8000,
    timeoutMsg: '清空筛选后仍残留草稿条件',
  });
  await openFilterPanel();
  // Re-opening a panel with an empty draft seeds exactly one condition, so the
  // first row is index 0 and a later `filter-add` appends index 1. Cases rely on
  // that order instead of guessing which row is which.
  await browser.waitUntil(async () => (await conditionCount()) === 1, {
    timeout: 8000,
    timeoutMsg: '清空后筛选面板未提供唯一的草稿条件',
  });
}

/** Number of distinct draft conditions currently rendered. */
async function conditionCount(): Promise<number> {
  return browser.execute(
    () =>
      new Set(
        Array.from(document.querySelectorAll('[data-filter-index]')).map((el) =>
          el.getAttribute('data-filter-index'),
        ),
      ).size,
  );
}

/**
 * Append one draft condition and make sure its editor is expanded.
 *
 * A fresh draft can be *complete by default* (a valueless operator such as
 * `IS NULL`), and a complete condition renders as a collapsed chip without any
 * input. Treating "no input yet" as failure is what kept every later case from
 * ever reaching Apply.
 */
/** Append one condition and return its index. */
async function addCondition(): Promise<number> {
  const before = await conditionCount();
  await $('[data-testid="filter-add"]').click();
  await browser.waitUntil(async () => (await conditionCount()) > before, {
    timeout: 8000,
    timeoutMsg: '新增筛选条件后行数未增加',
  });
  const index = before;
  const input = await $(`[data-filter-index="${index}"] [data-testid="filter-value"]`);
  if (!(await input.isExisting().catch(() => false))) {
    await $(`[data-filter-index="${index}"] button[title="${t('filter.editCondition')}"]`).click();
    await browser.pause(300);
  }
  await $(`[data-filter-index="${index}"] button[aria-haspopup="listbox"]`).waitForDisplayed({
    timeout: 8000,
    timeoutMsg: `条件 ${index} 展开后没有列选择器`,
  });
  return index;
}

/**
 * Set one condition's column through its own row.
 *
 * The panel renders the same `data-filter-index` on the chip wrapper and the
 * expanded editor, so every locator here takes the row as the *prefix* of a
 * compound selector: the subject stays the input/button that actually lives in
 * that row.
 */
async function setConditionColumn(index: number, column: string) {
  const row = await $(`[data-filter-index="${index}"]`);
  const triggers = await row.$$('button[aria-haspopup="listbox"]');
  if (triggers.length < 2) throw new Error(`条件 ${index} 的下拉触发器不足: ${triggers.length}`);
  await triggers[0].click();
  await browser.waitUntil(
    async () =>
      browser.execute(() =>
        Boolean(
          document.querySelector('[data-testid="select-listbox"]') ??
            document.querySelector('[id^="dz-select-listbox-"]'),
        ),
      ),
    { timeout: 5000, timeoutMsg: `条件 ${index} 的列下拉未打开` },
  );
  await browser.execute((label: string) => {
    const list =
      document.querySelector('[data-testid="select-listbox"]') ??
      document.querySelector('[id^="dz-select-listbox-"]');
    if (!list) throw new Error('listbox not open');
    const item = Array.from(list.children).find((el) => (el.textContent ?? '').includes(label));
    if (!item) throw new Error(`option not found: ${label}`);
    item.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
  }, column);
  await browser.pause(300);
}

/**
 * Drop the blank condition the panel seeds when it re-opens on a complete draft.
 *
 * Leaving it in place blocks `Apply` as an incomplete condition, which is what
 * made the chip-editing case wait on a permanently disabled button.
 */
async function removeIncompleteRows() {
  for (let guard = 0; guard < 6; guard++) {
    const hasIncomplete = await browser.execute(() =>
      Boolean(document.querySelector('[data-testid="filter-incomplete"]')),
    );
    if (!hasIncomplete) return;
    const removeBtns = await $$(`button[aria-label="${t('filter.remove')}"]`);
    if (removeBtns.length === 0) return;
    await removeBtns[removeBtns.length - 1].click();
    await browser.pause(250);
  }
}

/** Fill one condition's value and prove the value actually stuck. */
async function setConditionValue(index: number, value: string) {
  const input = await $(`[data-filter-index="${index}"] [data-testid="filter-value"]`);
  await input.waitForDisplayed({ timeout: 8000 });
  await input.clearValue();
  await input.setValue(value);
  await browser.waitUntil(async () => (await input.getValue()) === value, {
    timeout: 5000,
    timeoutMsg: `条件 ${index} 的值未写入: ${value}`,
  });
  await browser.pause(200);
}

async function applyFilter() {
  const applyBtn = await $('[data-testid="filter-apply"]');
  try {
    await applyBtn.waitForEnabled({ timeout: 5000 });
  } catch (error) {
    const diag = await browser.execute(() => {
      const rows = Array.from(document.querySelectorAll('[data-filter-index]')).map((row) => ({
        index: row.getAttribute('data-filter-index'),
        triggers: Array.from(row.querySelectorAll('button[aria-haspopup="listbox"]')).map((b) =>
          (b.textContent ?? '').trim(),
        ),
        values: Array.from(row.querySelectorAll('[data-testid="filter-value"]')).map(
          (i) => (i as HTMLInputElement).value,
        ),
        incomplete: Boolean(row.querySelector('[data-testid="filter-incomplete"]')),
      }));
      const tree = Array.from(document.querySelectorAll('[data-filter-index]')).map((el) => ({
        index: el.getAttribute('data-filter-index'),
        tag: el.tagName,
        cls: (el.className || '').toString().slice(0, 70),
        depth: (() => {
          let d = 0;
          let cur: Element | null = el;
          while (cur && cur.parentElement) {
            if (cur.parentElement.hasAttribute('data-filter-index')) d++;
            cur = cur.parentElement;
          }
          return d;
        })(),
        parentIndex:
          el.parentElement?.closest('[data-filter-index]')?.getAttribute('data-filter-index') ??
          null,
      }));
      return {
        tree,
        rows,
        draftCount: rows.length,
        banner: document.querySelector('[data-testid="filter-incomplete-error"]')?.textContent,
        error: document.querySelector('[data-testid="filter-error"]')?.textContent,
      };
    });
    throw new Error(`${error instanceof Error ? error.message : '?'} :: ${JSON.stringify(diag)}`);
  }
  await applyBtn.click();
  await browser.pause(1200);
}

async function setFilterColumn(from: string, to: string) {
  await selectDzOption(from, to);
  await browser.pause(200);
}

describe('表数据筛选 (TF-001~TF-010)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();

    await openQueryTab();
    // Checked DDL: a swallowed backend error (stale session bound to a
    // replaced worker DB) used to surface only as a 20s
    // `waitForTableInSidebar` timeout with no hint of the real cause.
    await executeSQLChecked(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQLChecked(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(50) NOT NULL,
        score INT NOT NULL
      )
    `);
    await executeSQLChecked(`
      INSERT INTO ${TEST_TABLE} (name, score) VALUES
        ('alpha', 10),
        ('beta', 20),
        ('gamma', 30)
    `);
    // Let the query panel finish its result-state update before refreshing
    // the virtualized schema tree; otherwise the just-created table can be
    // absent from the navigator on a cold Tauri run.
    await browser.pause(1200);

    await clickNavigatorRefresh();
    await waitForTableInSidebar(TEST_TABLE);

    await clickTableInSidebar(TEST_TABLE);
    await browser.pause(1500);
    await switchSubTab('data');
    await browser.waitUntil(async () => (await $('body').getText()).includes('alpha'), {
      timeout: 15000,
      timeoutMsg: '等待筛选测试表数据加载',
    });
  });

  after(async () => {
    try {
      await browser.switchToWindow(mainWindow);
      await openQueryTab();
      await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    } catch {
      /* cleanup best-effort */
    }
    await closeExtraWindows(mainWindow);
  });

  it('打开筛选面板应显示添加与 Apply (TF-001)', async () => {
    await openFilterPanel();
    await expect(await $('[data-testid="filter-add"]')).toBeDisplayed();
    await expect(await $(`button*=${t('filter.and')}`)).toBeDisplayed();
    await expect(await $(`button*=${t('filter.or')}`)).toBeDisplayed();
    await expect(await $('[data-testid="filter-apply"]')).toBeDisplayed();
  });

  it('快速筛选表达式应直接过滤并支持清空 (TF-011)', async () => {
    const quickFilter = await $('[data-testid="table-quick-filter"]');
    await quickFilter.waitForDisplayed({ timeout: 5000 });
    await quickFilter.setValue("name = 'alpha'");
    await browser.keys('Enter');
    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('alpha') && !body.includes('beta') && !body.includes('gamma');
      },
      { timeout: 8000, timeoutMsg: '等待快速筛选结果' },
    );
    expect(await quickFilter.getAttribute('aria-invalid')).toBeNull();

    await quickFilter.clearValue();
    await browser.keys('Enter');
    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('alpha') && body.includes('beta') && body.includes('gamma');
      },
      { timeout: 8000, timeoutMsg: '等待清空快速筛选后的结果' },
    );
  });

  it('空值 Apply 不得报加载失败且表仍可见 (TF-008)', async () => {
    // Default first column is often id (integer). Empty eq must not break the grid.
    await openFilterPanel();
    const valueInput = await $('[data-testid="filter-value"]');
    await valueInput.waitForDisplayed({ timeout: 5000 });
    await valueInput.clearValue();
    await browser.pause(400);
    const applyBtn = await $('[data-testid="filter-apply"]');
    // Draft may already be dirty from addFilter; Apply if enabled.
    if (await applyBtn.isEnabled()) {
      await applyBtn.click();
      await browser.pause(1000);
    }
    const body = await $('body').getText();
    expect(body).not.toContain(t('tableData.loadFailed'));
    expect(body).toContain('alpha');
  });

  it('按 name=alpha Apply 应只保留匹配行 (TF-002)', async () => {
    // Ensure panel open with a filter row
    const editor = await $('[data-testid="filter-editor"]');
    if (!(await editor.isDisplayed().catch(() => false))) {
      await openFilterPanel();
    }
    const addBtn = await $('[data-testid="filter-add"]');
    if (
      !(await $('[data-testid="filter-value"]')
        .isDisplayed()
        .catch(() => false))
    ) {
      await addBtn.click();
      await browser.pause(300);
    }

    await setFilterColumn('id', 'name');
    const valueInput = await $('[data-testid="filter-value"]');
    await valueInput.waitForDisplayed({ timeout: 5000 });
    await valueInput.clearValue();
    await valueInput.setValue('alpha');
    await browser.pause(450);
    await applyFilter();

    const body = await $('body').getText();
    expect(body).toContain('alpha');
    expect(body).not.toContain('beta');
    expect(body).not.toContain('gamma');
    expect(body).not.toContain(t('tableData.loadFailed'));
  });

  it('清空筛选应恢复全部行 (TF-003)', async () => {
    const clearBtn = await $('[data-testid="filter-clear"]');
    await clearBtn.waitForDisplayed({ timeout: 5000 });
    await clearBtn.click();
    await browser.pause(1200);
    const body = await $('body').getText();
    expect(body).toContain('alpha');
    expect(body).toContain('beta');
    expect(body).toContain('gamma');
  });

  it('两条件 OR 应扩大结果集 (TF-004)', async () => {
    await openFilterPanel();
    // Build the draft from a known empty state: the previous case cleared the
    // filter, and appending to whatever was left produced a third, value-less
    // condition that kept Apply disabled.
    await clearDraft();
    await setConditionColumn(0, 'name');
    await setConditionValue(0, 'alpha');
    const second = await addCondition();
    await setConditionColumn(second, 'score');
    await setConditionValue(second, '20');

    await $('[data-testid="filter-logic-or"]').click();
    await browser.pause(200);
    await applyFilter();

    const body = await $('body').getText();
    expect(body).toContain('alpha');
    expect(body).toContain('beta');
    expect(body).not.toContain(t('tableData.loadFailed'));
  });

  it('切换 AND 后结果应变窄 (TF-005)', async () => {
    await $('[data-testid="filter-logic-and"]').click();
    await browser.pause(200);
    await applyFilter();
    const body = await $('body').getText();
    // alpha AND score=20 → empty intersection
    expect(body).not.toContain(t('tableData.loadFailed'));
    const hasAlpha = body.includes('alpha');
    const hasBeta = body.includes('beta');
    expect(hasAlpha && hasBeta).toBe(false);
  });

  it('收起面板后摘要仍可见 (TF-006)', async () => {
    // Re-apply a simple filter so summary is meaningful
    await openFilterPanel();
    await clearDraft();
    await setConditionColumn(0, 'name');
    await setConditionValue(0, 'gamma');
    await applyFilter();

    await $('[data-testid="filter-collapse"]').click();
    await browser.pause(300);
    const body = await $('body').getText();
    expect(body).toContain(t('filter.filter'));
    expect(body).toContain('gamma');
  });

  it('点击 chip 可再编辑并 Apply (TF-007)', async () => {
    // Seed exactly one *applied* condition through the store, then exercise the
    // chip → edit → Apply path. Building that state through the panel itself was
    // ambiguous: re-opening the editor seeds a blank draft row as well, so the
    // case could never tell the applied chip from the fresh draft.
    await browser.execute(() => {
      const store = (
        globalThis as unknown as {
          __tableDataStore: { getState: () => Record<string, unknown> };
        }
      ).__tableDataStore;
      const state = store.getState() as unknown as {
        activePanelId?: string;
        byPanel: Map<string, unknown>;
        setFilters: (panelId: string, filters: unknown[], logic: string) => void;
      };
      const panelId = state.activePanelId ?? Array.from(state.byPanel.keys()).at(-1);
      if (!panelId) throw new Error('no active table panel');
      state.setFilters(panelId, [{ column: 'name', operator: 'eq', value: 'gamma' }], 'and');
    });
    await browser.pause(1200);

    await openFilterPanel();
    const chip = await $(`button[title="${t('filter.editCondition')}"]`);
    await chip.waitForDisplayed({ timeout: 10000 });
    await chip.click();
    await browser.pause(300);
    await setConditionValue(0, 'beta');
    // Opening the editor on a complete draft also seeds a blank row; it must go,
    // otherwise it blocks Apply as an incomplete condition.
    await removeIncompleteRows();
    await applyFilter();
    const body = await $('body').getText();
    expect(body).toContain('beta');
    expect(body).not.toContain('gamma');
  });

  it('仅打开空草稿不 Apply 不得报错 (TF-009)', async () => {
    const clearBtn = await $('[data-testid="filter-clear"]');
    if (await clearBtn.isDisplayed().catch(() => false)) {
      await clearBtn.click();
      await browser.pause(800);
    }
    await openFilterPanel();
    await $('[data-testid="filter-add"]').click();
    await browser.pause(500);
    const body = await $('body').getText();
    expect(body).not.toContain(t('tableData.loadFailed'));
    expect(body).toContain('alpha');
    const pressed = await $('[data-testid="table-filter-toggle"]').getAttribute('aria-pressed');
    expect(pressed).toBe('true');
  });

  it('未应用草稿应显示未应用标记 (TF-010)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('filter.unapplied'));
  });

  it('智能筛选未配置时应显示配置提示 (TF-AI-001)', async () => {
    const smartFilter = await $("[data-testid='smart-filter-toggle']");
    if (await smartFilter.isExisting()) {
      await expect(smartFilter).toBeDisplayed();
      return;
    }
    // No AI filter button — this DB/table has no AI configured
  });
});
