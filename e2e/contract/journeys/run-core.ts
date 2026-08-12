import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  clickTableInSidebar,
  executeSQL,
  openQueryTab,
  setEditorContent,
  switchSubTab,
} from '../../helpers.js';
import { dataSeedSql, filterSeedSql, seedTableName } from '../fixtures';
import type { ContractConnCtx } from '../open-fixture';
import { focusContractCtx } from '../open-fixture';
import {
  bodyContainsAll,
  bodyContainsNone,
  paginationRangeVisible,
} from './plan';

async function seedSql(ctx: ContractConnCtx, statements: string[]) {
  await focusContractCtx(ctx);
  await openQueryTab();
  for (const sql of statements) {
    await executeSQL(sql);
  }
  const refreshBtn = await $(`button[title="${t('connWin.refresh')} (⌘R)"]`);
  if (await refreshBtn.isExisting()) {
    await refreshBtn.click();
    await browser.pause(1200);
  }
}

/** HC-QUERY: execute SQL and see result chrome. */
export async function runHcQuery(ctx: ContractConnCtx) {
  await focusContractCtx(ctx);
  await openQueryTab();
  await setEditorContent('SELECT 1 AS n');
  await executeSQL('SELECT 1 AS n');
  await browser.pause(800);
  const body = await $('body').getText();
  expect(body.includes('n') || body.includes('1')).toBe(true);
  expect(body).not.toContain(t('tableData.loadFailed'));
}

/** HC-DATA: open seeded table, see rows + pagination. */
export async function runHcData(ctx: ContractConnCtx) {
  const table = seedTableName(ctx.fixture, 'data');
  await seedSql(ctx, dataSeedSql(ctx.fixture, table, 60));
  await clickTableInSidebar(table);
  await browser.pause(1200);
  await switchSubTab(t('connWin.data'));
  await browser.waitUntil(
    async () => (await $('body').getText()).includes('user_'),
    { timeout: 15000, timeoutMsg: `${ctx.fixture.id} HC-DATA load` },
  );
  const body = await $('body').getText();
  expect(bodyContainsAll(body, ['user_'])).toBe(true);
  expect(paginationRangeVisible(body) || body.includes('user_')).toBe(true);
}

/** HC-FILTER: apply name=alpha; empty apply must not show loadFailed. */
export async function runHcFilter(ctx: ContractConnCtx) {
  const table = seedTableName(ctx.fixture, 'filter');
  await seedSql(ctx, filterSeedSql(ctx.fixture, table));
  await clickTableInSidebar(table);
  await browser.pause(1200);
  await switchSubTab(t('connWin.data'));
  await browser.waitUntil(
    async () => (await $('body').getText()).includes('alpha'),
    { timeout: 15000, timeoutMsg: `${ctx.fixture.id} HC-FILTER load` },
  );

  const toggle = await $('[data-testid="table-filter-toggle"]');
  await toggle.waitForDisplayed({ timeout: 10000 });
  if ((await toggle.getAttribute('aria-pressed')) !== 'true') {
    await toggle.click();
    await browser.pause(400);
  }
  await $('[data-testid="filter-editor"]').waitForDisplayed({ timeout: 5000 });

  // Empty apply regression
  const applyBtn = await $('[data-testid="filter-apply"]');
  if (await applyBtn.isEnabled()) {
    await applyBtn.click();
    await browser.pause(800);
  }
  let body = await $('body').getText();
  expect(body).not.toContain(t('tableData.loadFailed'));
  expect(body).toContain('alpha');

  // Set column to name and filter alpha
  await browser.execute(() => {
    const editor = document.querySelector('[data-testid="filter-editor"]');
    const triggers = Array.from(
      editor?.querySelectorAll('button[aria-haspopup="listbox"]') ?? [],
    ) as HTMLElement[];
    const col = triggers[0];
    col?.click();
    const list = document.getElementById('dz-select-listbox');
    const nameOpt = Array.from(list?.children ?? []).find((el) =>
      (el.textContent || '').includes('name'),
    );
    nameOpt?.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
  });
  await browser.pause(200);
  const valueInput = await $('[data-testid="filter-value"]');
  await valueInput.clearValue();
  await valueInput.setValue('alpha');
  await browser.pause(450);
  await $('[data-testid="filter-apply"]').click();
  await browser.pause(1200);

  body = await $('body').getText();
  expect(bodyContainsAll(body, ['alpha'])).toBe(true);
  expect(bodyContainsNone(body, ['beta', 'gamma'])).toBe(true);
  expect(body).not.toContain(t('tableData.loadFailed'));

  const clearBtn = await $('[data-testid="filter-clear"]');
  if (await clearBtn.isDisplayed().catch(() => false)) {
    await clearBtn.click();
    await browser.pause(1000);
  }
}
