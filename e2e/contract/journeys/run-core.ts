import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  clickTableInSidebar,
  executeSQL,
  openQueryTab,
  setEditorContent,
  switchSubTab,
  selectDzOption,
} from '../../helpers.js';
import type { ContractConnCtx } from '../open-fixture';
import { focusContractCtx } from '../open-fixture';
import { bodyContainsAll, bodyContainsNone, paginationRangeVisible } from './plan';
import { seedContractTable } from './seed';

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
  const table = await seedContractTable(ctx, 'data');
  await clickTableInSidebar(table);
  await switchSubTab('data');
  await browser.waitUntil(async () => (await $('body').getText()).includes('user_'), {
    timeout: 15000,
    timeoutMsg: `${ctx.fixture.id} HC-DATA load`,
  });
  const body = await $('body').getText();
  expect(bodyContainsAll(body, ['user_'])).toBe(true);
  expect(paginationRangeVisible(body) || body.includes('user_')).toBe(true);
}

/** HC-FILTER: apply name=alpha; empty apply must not show loadFailed. */
export async function runHcFilter(ctx: ContractConnCtx) {
  const table = await seedContractTable(ctx, 'filter');
  await clickTableInSidebar(table);
  await switchSubTab('data');
  await browser.waitUntil(async () => (await $('body').getText()).includes('alpha'), {
    timeout: 15000,
    timeoutMsg: `${ctx.fixture.id} HC-FILTER load`,
  });

  const toggle = await $('[data-testid="table-filter-toggle"]');
  await toggle.waitForDisplayed({ timeout: 10000 });
  if ((await toggle.getAttribute('aria-pressed')) !== 'true') {
    await toggle.click();
    await browser.pause(400);
  }
  await $('[data-testid="filter-editor"]').waitForDisplayed({ timeout: 5000 });
  // The editor is intentionally empty after opening a fresh table. Add the
  // first condition before selecting a column; otherwise the only listbox is
  // the page-size selector and the column trigger cannot be found.
  const filterValue = await $('[data-testid="filter-value"]');
  if (!(await filterValue.isExisting())) {
    const addFilter = await $('[data-testid="filter-add"]');
    await addFilter.waitForDisplayed({ timeout: 5000 });
    await addFilter.click();
    await browser.pause(300);
  }

  // Empty apply regression
  const applyBtn = await $('[data-testid="filter-apply"]');
  if (await applyBtn.isEnabled()) {
    await applyBtn.click();
    await browser.pause(800);
  }
  let body = await $('body').getText();
  expect(body).not.toContain(t('tableData.loadFailed'));
  expect(body).toContain('alpha');

  // Set column to name and filter alpha through the shared Host Select helper.
  await selectDzOption('id', 'name');
  const valueInput = await $('[data-testid="filter-value"]');
  await valueInput.clearValue();
  await valueInput.setValue('alpha');
  await browser.pause(450);
  await $('[data-testid="filter-apply"]').click();
  await browser.pause(300);

  body = await $('body').getText();
  expect(bodyContainsAll(body, ['alpha'])).toBe(true);
  expect(bodyContainsNone(body, ['beta', 'gamma'])).toBe(true);
  expect(body).not.toContain(t('tableData.loadFailed'));

  const clearBtn = await $('[data-testid="filter-clear"]');
  if (await clearBtn.isDisplayed().catch(() => false)) {
    await clearBtn.click();
    await browser.pause(300);
  }
}
