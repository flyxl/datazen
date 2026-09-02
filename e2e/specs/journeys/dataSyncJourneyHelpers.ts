/**
 * Shared helpers for Data Sync WDIO user journeys (PG / MySQL).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  connectConfig,
  executeQuery,
  invokeBackend,
  parseQueryRows,
  queryScalar,
  selectDzOptionInWrap,
  withSafeModeOff,
} from '../../helpers.js';

export type SyncDriverKind = 'postgresql' | 'mysql';

export interface SyncJourneyFixture {
  stamp: string;
  srcId: string;
  tgtId: string;
  srcName: string;
  tgtName: string;
  table: string;
  srcDatabase: string;
  tgtDatabase: string;
  driver: SyncDriverKind;
  screenshotPrefix: string;
}

type DataSyncWizardStep = 'endpoints' | 'setup' | 'objects' | 'compare' | 'preview' | 'result';

const DATA_SYNC_STEP_INDEX: Record<DataSyncWizardStep, number> = {
  endpoints: 0,
  setup: 1,
  objects: 2,
  compare: 3,
  preview: 4,
  result: 5,
};

export function pgConfig(id: string, name: string, database: string) {
  return {
    id,
    name,
    databaseType: 'postgresql',
    host: process.env.E2E_PG_HOST || '127.0.0.1',
    port: Number(process.env.E2E_PG_PORT) || 5432,
    username: process.env.E2E_PG_USER || 'postgres',
    password: process.env.E2E_PG_PASSWORD || '',
    database,
    sslMode: 'disable',
  };
}

export function mysqlConfig(id: string, name: string, database: string) {
  return {
    id,
    name,
    databaseType: 'mysql',
    host: process.env.E2E_MYSQL_HOST || '127.0.0.1',
    port: Number(process.env.E2E_MYSQL_PORT) || 3306,
    username: process.env.E2E_MYSQL_USER || 'root',
    password: process.env.E2E_MYSQL_PASSWORD || '',
    database,
    sslMode: 'disable',
  };
}

export function createFixture(driver: SyncDriverKind, stamp: string): SyncJourneyFixture {
  const tag = driver === 'postgresql' ? 'pg' : 'my';
  return {
    stamp,
    driver,
    srcId: `e2e_ds_j_${tag}_src_${stamp}`,
    tgtId: `e2e_ds_j_${tag}_tgt_${stamp}`,
    srcName: `DS-J-${tag.toUpperCase()}-Src-${stamp}`,
    tgtName: `DS-J-${tag.toUpperCase()}-Tgt-${stamp}`,
    table: `e2e_ds_j_${tag}_${stamp}`,
    srcDatabase: driver === 'postgresql' ? 'datazen_sync_src' : 'datazen_sync_mysql_src',
    tgtDatabase: driver === 'postgresql' ? 'datazen_sync_tgt' : 'datazen_sync_mysql_tgt',
    screenshotPrefix: `ds-journey-${tag}`,
  };
}

export async function openDataSyncWindow() {
  await browser.url('tauri://localhost/window.html?window=data-sync');
  await browser.pause(1500);
  await $('[data-testid="data-sync-window"]').waitForDisplayed({ timeout: 10000 });
  await waitForDataSyncStep('endpoints');
}

async function waitForDataSyncStep(step: DataSyncWizardStep) {
  await browser.waitUntil(
    async () =>
      (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-step')) === step,
    { timeout: 15000, timeoutMsg: `data-sync wizard did not reach ${step} step` },
  );
}

async function clickDataSyncNext() {
  const next = await $('[data-testid="data-sync-next"]');
  await next.waitForEnabled({ timeout: 15000 });
  await next.click();
}

async function clickDataSyncBack() {
  const back = await $('[data-testid="data-sync-back"]');
  await back.waitForEnabled({ timeout: 15000 });
  await back.click();
}

async function waitForDataSyncNotBusy(timeoutMsg: string) {
  await browser.waitUntil(
    async () => {
      const state = await $('[data-testid="data-sync-window"]').getAttribute('data-sync-state');
      return state !== 'inspecting' && state !== 'comparing' && state !== 'executing';
    },
    { timeout: 120000, timeoutMsg },
  );
}

export async function advanceDataSyncToSetup() {
  await waitForDataSyncStep('endpoints');
  await clickDataSyncNext();
  await waitForDataSyncStep('setup');
}

export async function inspectDataSyncObjects() {
  await waitForDataSyncStep('setup');
  await clickDataSyncNext();
  await waitForDataSyncStep('objects');
  await waitForDataSyncNotBusy('data-sync inspection did not finish');
  const err = await $('[data-testid="data-sync-error"]');
  if (await err.isDisplayed().catch(() => false)) {
    throw new Error(`inspection error: ${await err.getText()}`);
  }
}

export async function compareDataSyncObjects() {
  await waitForDataSyncStep('objects');
  await clickDataSyncNext();
  await waitForDataSyncStep('compare');
  await waitForDataSyncNotBusy('data-sync compare did not finish');
  const err = await $('[data-testid="data-sync-error"]');
  if (await err.isDisplayed().catch(() => false)) {
    throw new Error(`compare error: ${await err.getText()}`);
  }
  await $('[data-testid="data-sync-summary"]').waitForDisplayed({ timeout: 15000 });
}

export async function advanceDataSyncToPreview() {
  await waitForDataSyncStep('compare');
  await clickDataSyncNext();
  await waitForDataSyncStep('preview');
}

export async function moveDataSyncBackTo(step: DataSyncWizardStep) {
  const targetIndex = DATA_SYNC_STEP_INDEX[step];
  while (true) {
    const current = (await $('[data-testid="data-sync-window"]').getAttribute(
      'data-sync-step',
    )) as DataSyncWizardStep;
    if (current === step) return;
    if (DATA_SYNC_STEP_INDEX[current] <= targetIndex) {
      throw new Error(`data-sync wizard is already before ${step}: ${current}`);
    }
    await clickDataSyncBack();
    await waitForDataSyncStep(
      Object.entries(DATA_SYNC_STEP_INDEX).find(
        ([, index]) => index === DATA_SYNC_STEP_INDEX[current] - 1,
      )?.[0] as DataSyncWizardStep,
    );
  }
}

export async function captureStep(label: string) {
  await captureJourneyStep(label, 0, true);
}

export async function dismissOkDialog() {
  const ok = await $(`button*=${t('common.ok')}`);
  if (await ok.isDisplayed().catch(() => false)) {
    await ok.click();
    await browser.waitUntil(
      async () =>
        !(await $('[data-testid="data-sync-error"]')
          .isDisplayed()
          .catch(() => false)),
      { timeout: 5000, timeoutMsg: 'error dialog did not close' },
    );
    await browser.pause(200);
  }
}

export async function dismissCancelDialog() {
  const cancel = await $(`button*=${t('common.cancel')}`);
  if (await cancel.isDisplayed()) {
    await cancel.click();
    await browser.pause(200);
  }
}

export async function saveFixtureConnections(f: SyncJourneyFixture) {
  const cfg = f.driver === 'postgresql' ? pgConfig : mysqlConfig;
  await invokeBackend('save_connection', { config: cfg(f.srcId, f.srcName, f.srcDatabase) });
  await invokeBackend('save_connection', { config: cfg(f.tgtId, f.tgtName, f.tgtDatabase) });
}

export async function seedFixtureTable(f: SyncJourneyFixture) {
  const srcSession = await connectConfig(f.srcId);
  const tgtSession = await connectConfig(f.tgtId);
  const ddl =
    f.driver === 'postgresql'
      ? `CREATE TABLE ${f.table} (id int PRIMARY KEY, name text NOT NULL)`
      : `CREATE TABLE ${f.table} (id int PRIMARY KEY, name varchar(64) NOT NULL)`;

  await withSafeModeOff(async () => {
    await executeQuery(srcSession, `DROP TABLE IF EXISTS ${f.table}`);
    await executeQuery(tgtSession, `DROP TABLE IF EXISTS ${f.table}`);
    await executeQuery(srcSession, ddl);
    await executeQuery(tgtSession, ddl);
    // src: 5 rows; row 2 differs (UPDATE); rows 4–5 missing on tgt (INSERT)
    await executeQuery(
      srcSession,
      `INSERT INTO ${f.table} (id, name) VALUES (1,'a'),(2,'B-new'),(3,'c'),(4,'d'),(5,'e')`,
    );
    await executeQuery(
      tgtSession,
      `INSERT INTO ${f.table} (id, name) VALUES (1,'a'),(2,'b'),(3,'c')`,
    );
  });
}

export async function cleanupFixture(f: SyncJourneyFixture | undefined) {
  if (!f) return;
  try {
    const srcSession = await connectConfig(f.srcId);
    const tgtSession = await connectConfig(f.tgtId);
    await withSafeModeOff(async () => {
      await executeQuery(srcSession, `DROP TABLE IF EXISTS ${f.table}`);
      await executeQuery(tgtSession, `DROP TABLE IF EXISTS ${f.table}`);
    });
  } catch {
    /* ok */
  }
  for (const id of [f.srcId, f.tgtId]) {
    try {
      await invokeBackend('delete_connection', { id });
    } catch {
      /* ok */
    }
  }
}

export async function selectFixtureEndpoints(f: SyncJourneyFixture) {
  await selectDzOptionInWrap('data-sync-source', f.srcName);
  await selectDzOptionInWrap('data-sync-target', f.tgtName);
  await browser.pause(1500);
  await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
  await expect(await $('[data-testid="data-sync-target-database"]')).toBeDisplayed();
}

export async function runCompare(f: SyncJourneyFixture) {
  await advanceDataSyncToSetup();

  const insertOpt = await $('[data-testid="data-sync-option-insert"]');
  if (!(await insertOpt.isSelected())) {
    await insertOpt.click();
    await browser.pause(200);
  }
  const updateOpt = await $('[data-testid="data-sync-option-update"]');
  if (!(await updateOpt.isSelected())) {
    await updateOpt.click();
    await browser.pause(200);
  }

  await inspectDataSyncObjects();
  await compareDataSyncObjects();
  await browser.pause(500);

  const rows = await browser.execute((tableName: string) => {
    const els = document.querySelectorAll('[data-testid="data-sync-mapping-row"]');
    return Array.from(els).some((el) => (el.textContent || '').includes(tableName));
  }, f.table);
  expect(rows).toBe(true);
}

export async function runPostCompareReviewBranches(f: SyncJourneyFixture) {
  const search = await $('input[type="search"]');
  await search.waitForDisplayed({ timeout: 10000 });
  await search.setValue(f.table);
  await browser.pause(300);
  expect(await $('body').getText()).toContain(f.table);

  const insertFilter = await $(`button*=${t('sync.filter.insert')}`);
  await insertFilter.click();
  await browser.pause(300);
  expect(await $('body').getText()).toContain(f.table);

  const updateFilter = await $(`button*=${t('sync.filter.update')}`);
  await updateFilter.click();
  await browser.pause(300);
  expect(await $('body').getText()).toContain(f.table);

  const deleteFilter = await $(`button*=${t('sync.filter.delete')}`);
  await deleteFilter.click();
  await browser.pause(300);

  const unchangedFilter = await $(`button*=${t('sync.filter.unchanged')}`);
  await unchangedFilter.click();
  await browser.pause(300);

  const allFilter = await $(`button*=${t('sync.filter.all')}`);
  await allFilter.click();
  await browser.pause(200);

  await search.setValue('');
  await browser.pause(200);

  const copyBtn = await $('[data-testid="data-sync-copy-report"]');
  await expect(copyBtn).toBeDisplayed();
  await copyBtn.click();
  await browser.pause(200);

  const rowDiff = await $('[data-testid="data-sync-row-diff"]');
  if (await rowDiff.isDisplayed().catch(() => false)) {
    await expect(rowDiff).toBeDisplayed();
  }

  const toggledOff = await browser.execute((tableName: string) => {
    const rows = document.querySelectorAll('[data-testid="data-sync-mapping-row"]');
    for (const row of rows) {
      if ((row.textContent || '').includes(tableName)) {
        const cb = row.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
        cb?.click();
        return true;
      }
    }
    return false;
  }, f.table);
  expect(toggledOff).toBe(true);
  await browser.pause(400);
  await expect(await $('[data-testid="data-sync-next"]')).toBeDisabled();

  await moveDataSyncBackTo('objects');
  await browser.execute((tableName: string) => {
    const rows = document.querySelectorAll('[data-testid="data-sync-mapping-row"]');
    for (const row of rows) {
      if ((row.textContent || '').includes(tableName)) {
        const cb = row.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
        cb?.click();
      }
    }
  }, f.table);
  await browser.pause(300);
  await compareDataSyncObjects();
  await advanceDataSyncToPreview();
  await expect(await $('[data-testid="data-sync-preview"]')).toBeDisplayed();

  await moveDataSyncBackTo('setup');
  const insertOpt = await $('[data-testid="data-sync-option-insert"]');
  if (await insertOpt.isSelected()) {
    await insertOpt.click();
    await browser.pause(300);
  }
  await inspectDataSyncObjects();
  await compareDataSyncObjects();
  await advanceDataSyncToPreview();
  await expect(await $('[data-testid="data-sync-start-disabled"]')).toBeDisplayed();

  await moveDataSyncBackTo('setup');
  const insertOptAgain = await $('[data-testid="data-sync-option-insert"]');
  await insertOptAgain.click();
  await browser.pause(300);
  await inspectDataSyncObjects();
  await compareDataSyncObjects();
  await advanceDataSyncToPreview();
  await browser.pause(500);
}

export async function runExecuteAndVerify(f: SyncJourneyFixture) {
  const start = await $('[data-testid="data-sync-start"]');
  await start.waitForClickable({ timeout: 20000 });
  await start.click();
  await browser.waitUntil(
    async () => {
      const cancel = await $('[data-testid="data-sync-cancel"]');
      return !(await cancel.isDisplayed().catch(() => false));
    },
    { timeout: 120000, timeoutMsg: 'execute did not finish' },
  );
  const err = await $('[data-testid="data-sync-error"]');
  if (await err.isDisplayed().catch(() => false)) {
    throw new Error(`execute error: ${await err.getText()}`);
  }
  const doneBanner = await $('[data-testid="data-sync-execute-done"]');
  await expect(doneBanner).toBeDisplayed();
  expect(await doneBanner.getText()).toContain(t('sync.executeDone'));
  const syncState = await $('[data-testid="data-sync-window"]').getAttribute('data-sync-state');
  expect(syncState).toBe('done');
  await expect(await $('[data-testid="data-sync-summary"]')).toBeDisplayed();

  const tgtSession = await connectConfig(f.tgtId);
  await browser.waitUntil(
    async () => {
      const countRows = await executeQuery(
        tgtSession,
        f.driver === 'postgresql'
          ? `SELECT count(*)::int AS c FROM ${f.table}`
          : `SELECT count(*) AS c FROM ${f.table}`,
      );
      return queryScalar(countRows, 'c') === 5;
    },
    { timeout: 30000, interval: 1000, timeoutMsg: `target ${f.table} row count did not reach 5` },
  );

  const nameRows = await executeQuery(tgtSession, `SELECT name FROM ${f.table} WHERE id = 2`);
  const names = parseQueryRows(nameRows);
  expect(String(names[0]?.[0])).toContain('B-new');
}

/** After initial sync: add tgt-only row, re-compare, enable delete, confirm execute. */
export async function runExecuteDeleteConfirmBranch(f: SyncJourneyFixture) {
  const tgtSession = await connectConfig(f.tgtId);
  await withSafeModeOff(async () => {
    await executeQuery(tgtSession, `INSERT INTO ${f.table} (id, name) VALUES (6,'extra')`);
  });

  await moveDataSyncBackTo('setup');
  const deleteOpt = await $('[data-testid="data-sync-option-delete"]');
  await deleteOpt.click();
  await browser.pause(400);
  const enableBtn = await $(`button*=${t('sync.enableDelete')}`);
  await enableBtn.waitForDisplayed({ timeout: 5000 });
  await enableBtn.click();
  await browser.pause(300);
  expect(await deleteOpt.isSelected()).toBe(true);

  await inspectDataSyncObjects();
  await compareDataSyncObjects();
  await captureStep(`${f.screenshotPrefix}-16-delete-recompared`);

  const deleteFilter = await $(`button*=${t('sync.filter.delete')}`);
  await deleteFilter.click();
  await browser.pause(300);

  await browser.execute((tableName: string) => {
    const rows = document.querySelectorAll('[data-testid="data-sync-mapping-row"]');
    for (const row of rows) {
      if ((row.textContent || '').includes(tableName)) {
        (row as HTMLElement).click();
      }
    }
  }, f.table);
  await browser.pause(400);

  const selectAllDelete = await $(`button*=${t('sync.selectAllDelete')}`);
  await selectAllDelete.waitForDisplayed({ timeout: 10000 });
  await selectAllDelete.click();
  await browser.pause(300);

  await advanceDataSyncToPreview();
  const start = await $('[data-testid="data-sync-start"]');
  await start.waitForClickable({ timeout: 20000 });
  await start.click();
  await browser.pause(400);
  expect(await $('body').getText()).toContain(t('sync.executeDeleteTitle'));
  await captureStep(`${f.screenshotPrefix}-17-delete-execute-confirm`);
  const clicked = await browser.execute((label: string) => {
    const buttons = Array.from(document.querySelectorAll('button')).reverse();
    const button = buttons.find((btn) => (btn.textContent || '').includes(label));
    if (!button) return false;
    button.click();
    return true;
  }, t('sync.execute'));
  expect(clicked).toBe(true);

  await browser.waitUntil(
    async () => {
      const cancel = await $('[data-testid="data-sync-cancel"]');
      return !(await cancel.isDisplayed().catch(() => false));
    },
    { timeout: 120000, timeoutMsg: 'delete execute did not finish' },
  );

  await browser.waitUntil(
    async () => {
      const rows = await executeQuery(
        tgtSession,
        f.driver === 'postgresql'
          ? `SELECT count(*)::int AS c FROM ${f.table}`
          : `SELECT count(*) AS c FROM ${f.table}`,
      );
      return queryScalar(rows, 'c') === 5;
    },
    { timeout: 30000, interval: 1000, timeoutMsg: 'row count after delete execute not 5' },
  );

  const orphan = await executeQuery(
    tgtSession,
    `SELECT count(*) AS c FROM ${f.table} WHERE id = 6`,
  );
  expect(queryScalar(orphan, 'c')).toBe(0);
}

/** Validation branches reachable before a successful compare (driver-agnostic UI). */
export async function runPreCompareValidationBranches(
  f: SyncJourneyFixture,
  sameEndpointId: string,
  sameEndpointName: string,
) {
  await openDataSyncWindow();
  await captureStep(`${f.screenshotPrefix}-01-window-open`);

  const next = await $('[data-testid="data-sync-next"]');
  await expect(next).toBeDisabled();
  await captureStep(`${f.screenshotPrefix}-04-select-both`);

  await selectDzOptionInWrap('data-sync-source', sameEndpointName);
  await selectDzOptionInWrap('data-sync-target', sameEndpointName);
  await browser.pause(800);
  await advanceDataSyncToSetup();

  const deleteOpt = await $('[data-testid="data-sync-option-delete"]');
  await deleteOpt.click();
  await browser.pause(400);
  expect(await $('body').getText()).toContain(t('sync.deleteConfirmTitle'));
  await captureStep(`${f.screenshotPrefix}-02-delete-confirm`);
  await dismissCancelDialog();
  expect(await deleteOpt.isSelected()).toBe(false);

  const updateOpt = await $('[data-testid="data-sync-option-update"]');
  await updateOpt.click();
  await browser.pause(300);
  await captureStep(`${f.screenshotPrefix}-03-options-toggled`);

  await next.click();
  await browser.pause(500);
  const errSame = await $('[data-testid="data-sync-error"]');
  await expect(errSame).toBeDisplayed();
  expect(await errSame.getText()).toContain(t('sync.cannotSameDb'));
  await captureStep(`${f.screenshotPrefix}-05-same-endpoint`);
  await dismissOkDialog();

  await moveDataSyncBackTo('endpoints');
  await selectFixtureEndpoints(f);
  await captureStep(`${f.screenshotPrefix}-07-endpoints-selected`);

  try {
    await invokeBackend('delete_connection', { id: sameEndpointId });
  } catch {
    /* ok */
  }
}

export async function runEndpointSwapBranch(f: SyncJourneyFixture) {
  await expect(await $('[data-testid="data-sync-swap"]')).not.toBeExisting();
  await captureStep(`${f.screenshotPrefix}-08-endpoints-step`);
  await browser.pause(300);
  await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
}

export async function runCompareCancelBranch(f: SyncJourneyFixture) {
  await waitForDataSyncStep('objects');
  await clickDataSyncNext();
  await waitForDataSyncStep('compare');
  const cancel = await $('[data-testid="data-sync-cancel"]');
  const sawCancel = await cancel
    .waitForDisplayed({ timeout: 8000 })
    .then(() => true)
    .catch(() => false);
  if (sawCancel) {
    await cancel.click();
    await browser.waitUntil(async () => !(await cancel.isDisplayed().catch(() => false)), {
      timeout: 15000,
      timeoutMsg: 'compare cancel did not finish',
    });
    await browser.pause(400);
    const err = await $('[data-testid="data-sync-error"]');
    if (await err.isDisplayed().catch(() => false)) {
      await dismissOkDialog();
    }
    await captureStep(`${f.screenshotPrefix}-09-compare-cancelled`);
  }
  await browser.pause(500);
}

export async function runDeleteEnableAcceptBranch(f: SyncJourneyFixture) {
  await advanceDataSyncToSetup();
  const deleteOpt = await $('[data-testid="data-sync-option-delete"]');
  await deleteOpt.click();
  await browser.pause(400);
  expect(await $('body').getText()).toContain(t('sync.deleteConfirmTitle'));
  const enableBtn = await $(`button*=${t('sync.enableDelete')}`);
  await enableBtn.waitForDisplayed({ timeout: 5000 });
  await enableBtn.click();
  await browser.pause(300);
  expect(await deleteOpt.isSelected()).toBe(true);
  await captureStep(`${f.screenshotPrefix}-10-delete-enabled`);
  await deleteOpt.click();
  await browser.pause(200);
  expect(await deleteOpt.isSelected()).toBe(false);
  await moveDataSyncBackTo('endpoints');
}

/** PG source selected: foreign MySQL target shows unsupportedPair in target list. */
export async function runUnsupportedPairHintBranch(
  f: SyncJourneyFixture,
  foreignTargetName: string,
) {
  await browser.execute(() => {
    const wrap = document.querySelector('[data-testid="data-sync-target"]');
    const btn = wrap?.querySelector('button[aria-haspopup="listbox"]') as HTMLElement | null;
    if (!btn) throw new Error('target select trigger not found');
    btn.click();
  });
  await browser.waitUntil(
    async () => {
      const list = await $('#dz-select-listbox');
      return list.isDisplayed().catch(() => false);
    },
    { timeout: 5000, timeoutMsg: 'target listbox did not open' },
  );
  const listText = await $('#dz-select-listbox').getText();
  expect(listText).toContain(foreignTargetName);
  expect(listText).toContain(t('common.unsupportedPair'));
  await captureStep(`${f.screenshotPrefix}-11-unsupported-pair-hint`);
  await browser.execute(() => {
    const wrap = document.querySelector('[data-testid="data-sync-target"]');
    const btn = wrap?.querySelector('button[aria-haspopup="listbox"]') as HTMLElement | null;
    btn?.click();
  });
  await browser.pause(400);
}
