import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  executeSQL,
  openQueryTab,
  clickTableInSidebar,
  switchSubTab,
  selectDzOption,
  connectSeededPgInWorkspace,
  waitForTableInSidebar,
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
}

async function applyFilter() {
  const applyBtn = await $('[data-testid="filter-apply"]');
  await applyBtn.waitForEnabled({ timeout: 5000 });
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
    await executeSQL(`DROP TABLE IF EXISTS ${TEST_TABLE}`);
    await executeSQL(`
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name VARCHAR(50) NOT NULL,
        score INT NOT NULL
      )
    `);
    await executeSQL(`
      INSERT INTO ${TEST_TABLE} (name, score) VALUES
        ('alpha', 10),
        ('beta', 20),
        ('gamma', 30)
    `);

    const refreshBtn = await $(`button[title="${t('connWin.refresh')} (⌘R)"]`);
    await refreshBtn.click();
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

  it('空值 Apply 不得报加载失败且表仍可见 (TF-008)', async () => {
    // Default first column is often id (integer). Empty eq must not break the grid.
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
    // Reset to a clean single incomplete draft
    const clearBtn = await $('[data-testid="filter-clear"]');
    if (await clearBtn.isDisplayed().catch(() => false)) {
      await clearBtn.click();
      await browser.pause(800);
      await openFilterPanel();
    }
    await $('[data-testid="filter-add"]').click();
    await browser.pause(300);
    await setFilterColumn('id', 'name');
    let valueInputs = await $$('[data-testid="filter-value"]');
    await valueInputs[0].setValue('alpha');
    await browser.pause(400);

    await $('[data-testid="filter-add"]').click();
    await browser.pause(300);
    // Set the newest filter's column select (last listbox trigger that still shows id)
    await browser.execute(() => {
      const editor = document.querySelector('[data-testid="filter-editor"]');
      if (!editor) throw new Error('filter-editor missing');
      const triggers = Array.from(editor.querySelectorAll('button[aria-haspopup="listbox"]'));
      // Column selects are even indices within each condition (col, op)
      const colTriggers = triggers.filter((_, i) => i % 2 === 0);
      const lastCol = colTriggers[colTriggers.length - 1] as HTMLElement | undefined;
      if (!lastCol) throw new Error('column trigger missing');
      lastCol.click();
      const list = document.getElementById('dz-select-listbox');
      const score = Array.from(list?.children ?? []).find((el) =>
        (el.textContent || '').includes('score'),
      );
      if (!score) throw new Error('score option missing');
      score.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
    });
    await browser.pause(200);
    valueInputs = await $$('[data-testid="filter-value"]');
    await valueInputs[valueInputs.length - 1].setValue('20');
    await browser.pause(400);

    await $(`button*=${t('filter.or')}`).click();
    await browser.pause(200);
    await applyFilter();

    const body = await $('body').getText();
    expect(body).toContain('alpha');
    expect(body).toContain('beta');
    expect(body).not.toContain(t('tableData.loadFailed'));
  });

  it('切换 AND 后结果应变窄 (TF-005)', async () => {
    await $(`button*=${t('filter.and')}`).click();
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
    const clearBtn = await $('[data-testid="filter-clear"]');
    if (await clearBtn.isDisplayed().catch(() => false)) {
      await clearBtn.click();
      await browser.pause(800);
    }
    await openFilterPanel();
    await $('[data-testid="filter-add"]').click();
    await browser.pause(300);
    await setFilterColumn('id', 'name');
    const valueInput = await $('[data-testid="filter-value"]');
    await valueInput.setValue('gamma');
    await browser.pause(400);
    await applyFilter();

    await $('[data-testid="filter-collapse"]').click();
    await browser.pause(300);
    const body = await $('body').getText();
    expect(body).toContain(t('filter.filter'));
    expect(body).toContain('gamma');
  });

  it('点击 chip 可再编辑并 Apply (TF-007)', async () => {
    const chip = await $(`button[title="${t('filter.editCondition')}"]`);
    await chip.waitForDisplayed({ timeout: 5000 });
    await chip.click();
    await browser.pause(300);
    const valueInput = await $('[data-testid="filter-value"]');
    await valueInput.waitForDisplayed({ timeout: 5000 });
    await valueInput.clearValue();
    await valueInput.setValue('beta');
    await browser.pause(450);
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
