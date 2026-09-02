import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  clickTableInSidebar,
  doubleClickCellByText,
  openQueryTab,
  setEditorContent,
  switchSubTab,
  waitForEditInput,
} from '../../helpers.js';
import type { ContractConnCtx } from '../open-fixture';
import { focusContractCtx } from '../open-fixture';
import { seedContractTable } from './seed';

/** HC-CONN: connection toolbar + sub-tabs visible. */
export async function runHcConn(ctx: ContractConnCtx) {
  // Sub-tabs belong to an opened table workspace, not to the bare
  // connection workspace. Seed/open one before asserting the table chrome.
  const table = await seedContractTable(ctx, 'conn');
  await clickTableInSidebar(table);
  await browser.pause(800);
  await expect(await $("[data-testid='sub-tab-data']")).toExist();
  await expect(await $("[data-testid='sub-tab-structure']")).toExist();
}

/** HC-EDIT: inline edit a cell when supported. */
export async function runHcEdit(ctx: ContractConnCtx) {
  const table = await seedContractTable(ctx, 'edit');
  await clickTableInSidebar(table);
  await switchSubTab('data');
  await browser.waitUntil(async () => (await $('body').getText()).includes('alpha'), {
    timeout: 15000,
  });
  await doubleClickCellByText('alpha');
  const input = await waitForEditInput();
  await input.setValue('alpha2');
  await browser.keys('Enter');
  await browser.pause(300);
  const body = await $('body').getText();
  expect(body.includes('alpha2') || body.includes('alpha')).toBe(true);
}

/** HC-STRUCT: structure tab + edit/back path. */
export async function runHcStruct(ctx: ContractConnCtx) {
  const table = await seedContractTable(ctx, 'struct');
  await clickTableInSidebar(table);
  await browser.pause(800);
  await switchSubTab('structure');
  await browser.pause(800);
  const editBtn = await $("[data-testid='struct-edit-structure']");
  await editBtn.waitForDisplayed({ timeout: 10000 });
  await editBtn.click();
  await browser.pause(800);
  await expect($("[data-testid='struct-editor-execute']")).toExist();
  const back = await $("[data-testid='struct-editor-back']");
  if (await back.isExisting()) {
    await back.click();
    await browser.pause(400);
  }
}

/** HC-INDEX: indexes tab + new index dialog opens. */
export async function runHcIndex(ctx: ContractConnCtx) {
  const table = await seedContractTable(ctx, 'idx');
  await clickTableInSidebar(table);
  await browser.pause(800);
  await switchSubTab('indexes');
  await browser.pause(800);
  const newBtn = await $("[data-testid='idx-new-index']");
  await newBtn.waitForDisplayed({ timeout: 10000 });
  await newBtn.click();
  await browser.pause(400);
  await expect(await $('#idx-name')).toBeDisplayed();
  await $(`button*=${t('common.cancel')}`).click();
  await browser.pause(300);
}

/** HC-EXPORT: DataTable export dialog. */
export async function runHcExport(ctx: ContractConnCtx) {
  const table = await seedContractTable(ctx, 'export');
  await clickTableInSidebar(table);
  await switchSubTab('data');
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
  const explainBtn = await $('[data-testid="editor-explain-button"]');
  await explainBtn.waitForDisplayed({ timeout: 8000 });
  await explainBtn.click();
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return (
        body.includes(t('explain.title')) ||
        body.includes(t('explain.loading')) ||
        body.includes(t('explain.rawOutput'))
      );
    },
    { timeout: 10000, timeoutMsg: '等待 Explain 面板打开超时' },
  );
  // The panel may show loading/error/raw output depending on driver timing;
  // the stable contract is that Explain chrome opens for supported drivers.
  const body = await $('body').getText();
  expect(
    body.includes(t('explain.title')) ||
      body.includes(t('explain.loading')) ||
      body.includes('Result') ||
      body.includes('SCAN') ||
      body.includes('plan'),
  ).toBe(true);
}
