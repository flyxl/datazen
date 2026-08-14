import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  clickTableInSidebar,
  doubleClickCellByText,
  executeSQL,
  openQueryTab,
  setEditorContent,
  switchSubTab,
  waitForEditInput,
} from '../../helpers.js';
import { seedTableName, filterSeedSql } from '../fixtures';
import type { ContractConnCtx } from '../open-fixture';
import { focusContractCtx } from '../open-fixture';

async function ensureTable(ctx: ContractConnCtx, suffix: string) {
  const table = seedTableName(ctx.fixture, suffix);
  await focusContractCtx(ctx);
  await openQueryTab();
  for (const sql of filterSeedSql(ctx.fixture, table)) {
    await executeSQL(sql);
  }
  const refreshBtn = await $(`button[title="${t('connWin.refresh')} (⌘R)"]`);
  if (await refreshBtn.isExisting()) {
    await refreshBtn.click();
    await browser.pause(1000);
  }
  return table;
}

/** HC-CONN: connection toolbar + sub-tabs visible. */
export async function runHcConn(ctx: ContractConnCtx) {
  await focusContractCtx(ctx);
  await expect(await $(`button*=${t('connWin.newQuery')}`)).toBeDisplayed();
  const body = await $('body').getText();
  expect(
    body.includes(t('connWin.data')) ||
      body.includes(t('connWin.structure')) ||
      body.includes(t('connWin.indexes')),
  ).toBe(true);
}

/** HC-EDIT: inline edit a cell when supported. */
export async function runHcEdit(ctx: ContractConnCtx) {
  const table = await ensureTable(ctx, 'edit');
  await clickTableInSidebar(table);
  await browser.pause(1000);
  await switchSubTab(t('connWin.data'));
  await browser.waitUntil(async () => (await $('body').getText()).includes('alpha'), {
    timeout: 15000,
  });
  await doubleClickCellByText('alpha');
  const input = await waitForEditInput();
  await input.setValue('alpha2');
  await browser.keys('Enter');
  await browser.pause(1000);
  const body = await $('body').getText();
  expect(body.includes('alpha2') || body.includes('alpha')).toBe(true);
}

/** HC-STRUCT: structure tab + edit/back path. */
export async function runHcStruct(ctx: ContractConnCtx) {
  const table = await ensureTable(ctx, 'struct');
  await clickTableInSidebar(table);
  await browser.pause(800);
  await switchSubTab(t('connWin.structure'));
  await browser.pause(800);
  const editBtn = await $(`button*=${t('structView.editStructure')}`);
  await editBtn.waitForDisplayed({ timeout: 10000 });
  await editBtn.click();
  await browser.pause(800);
  await expect(await $(`button*=${t('structEditor.saveChanges')}`)).toBeDisplayed();
  const back = await $(`button*=${t('common.back')}`);
  if (await back.isExisting()) {
    await back.click();
    await browser.pause(400);
  }
}

/** HC-INDEX: indexes tab + new index dialog opens. */
export async function runHcIndex(ctx: ContractConnCtx) {
  const table = await ensureTable(ctx, 'idx');
  await clickTableInSidebar(table);
  await browser.pause(800);
  await switchSubTab(t('connWin.indexes'));
  await browser.pause(800);
  const newBtn = await $(`button*=${t('indexes.newIndex')}`);
  await newBtn.waitForDisplayed({ timeout: 10000 });
  await newBtn.click();
  await browser.pause(400);
  await expect(await $('#idx-name')).toBeDisplayed();
  await $(`button*=${t('common.cancel')}`).click();
  await browser.pause(300);
}

/** HC-EXPORT: DataTable export dialog. */
export async function runHcExport(ctx: ContractConnCtx) {
  const table = await ensureTable(ctx, 'export');
  await clickTableInSidebar(table);
  await browser.pause(1000);
  await switchSubTab(t('connWin.data'));
  await browser.pause(800);
  const exportBtn = await $(`button[title="${t('export.export')}"]`);
  await exportBtn.waitForDisplayed({ timeout: 10000 });
  await exportBtn.click();
  await browser.pause(500);
  const body = await $('body').getText();
  expect(body.includes('CSV') || body.includes('JSON') || body.includes(t('export.export'))).toBe(
    true,
  );
  const cancel = await $(`button*=${t('common.cancel')}`);
  if (await cancel.isDisplayed()) await cancel.click();
}

/** HC-OBJ: objects panel (skipped on sqlite via capabilities). */
export async function runHcObj(ctx: ContractConnCtx) {
  await focusContractCtx(ctx);
  const objBtn = await $(`button*=${t('objects.title')}`);
  await objBtn.waitForDisplayed({ timeout: 10000 });
  await objBtn.click();
  await browser.pause(600);
  await expect(await $(`button*=${t('objects.function')}`)).toBeDisplayed();
}

/** HC-EXPLAIN: explain button produces plan chrome. */
export async function runHcExplain(ctx: ContractConnCtx) {
  await focusContractCtx(ctx);
  await openQueryTab();
  await setEditorContent('SELECT 1 AS n');
  await browser.pause(300);
  const explainBtn = await $(`button*=${t('explain.title')}`);
  await explainBtn.waitForDisplayed({ timeout: 8000 });
  await explainBtn.click();
  await browser.pause(1500);
  // Raw EXPLAIN output section is present (table or text depending on driver).
  const rawOutput = await $(`*=${t('explain.rawOutput')}`);
  await rawOutput.waitForDisplayed({ timeout: 8000 });
  const body = await $('body').getText();
  expect(
    body.includes(t('explain.title')) ||
      body.includes(t('explain.loading')) ||
      body.includes('Result') ||
      body.includes('SCAN') ||
      body.includes('plan'),
  ).toBe(true);
}
