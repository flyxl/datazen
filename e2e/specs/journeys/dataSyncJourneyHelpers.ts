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
    table: `ds_j_${tag}_${stamp}`,
    srcDatabase: driver === 'postgresql' ? 'datazen_sync_src' : 'datazen_sync_mysql_src',
    tgtDatabase: driver === 'postgresql' ? 'datazen_sync_tgt' : 'datazen_sync_mysql_tgt',
    screenshotPrefix: `ds-journey-${tag}`,
  };
}

export async function openDataSyncWindow() {
  await browser.url('tauri://localhost/window.html?window=data-sync');
  await browser.pause(1500);
  await $('[data-testid="data-sync-compare"]').waitForDisplayed({ timeout: 10000 });
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

  await $('[data-testid="data-sync-compare"]').click();
  await browser.waitUntil(
    async () => {
      const cancel = await $('[data-testid="data-sync-cancel"]');
      return !(await cancel.isDisplayed().catch(() => false));
    },
    { timeout: 120000, timeoutMsg: 'compare did not finish' },
  );

  const err = await $('[data-testid="data-sync-error"]');
  if (await err.isDisplayed().catch(() => false)) {
    throw new Error(`compare error: ${await err.getText()}`);
  }

  await $('[data-testid="data-sync-summary"]').waitForDisplayed({ timeout: 15000 });
  await browser.pause(500);

  const rows = await browser.execute((tableName: string) => {
    const els = document.querySelectorAll('[data-testid="data-sync-mapping-row"]');
    return Array.from(els).some((el) => (el.textContent || '').includes(tableName));
  }, f.table);
  expect(rows).toBe(true);

  const path = await $('[data-testid="data-sync-path"]');
  await expect(path).toBeDisplayed();
  expect(await path.getText()).toContain(t('sync.pathDirect'));
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

  const previewTab = await $(`button*=${t('sync.sqlPreviewTab')}`);
  await previewTab.click();
  await browser.pause(600);
  await expect(await $('[data-testid="data-sync-preview"]')).toBeDisplayed();

  const rowDiffTab = await $(`button*=${t('sync.rowDiffTab')}`);
  await rowDiffTab.click();
  await browser.pause(400);
  await expect(await $('[data-testid="data-sync-row-diff"]')).toBeDisplayed();

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
  await expect(await $('[data-testid="data-sync-start-disabled"]')).toBeDisplayed();

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

  const insertOpt = await $('[data-testid="data-sync-option-insert"]');
  if (await insertOpt.isSelected()) {
    await insertOpt.click();
    await browser.pause(300);
  }
  await expect(await $('[data-testid="data-sync-start-disabled"]')).toBeDisplayed();
  await insertOpt.click();
  await browser.pause(300);

  // Disabling a mapping table clears row diffs; re-compare restores executable state.
  await $('[data-testid="data-sync-compare"]').click();
  await browser.waitUntil(
    async () => {
      const cancel = await $('[data-testid="data-sync-cancel"]');
      return !(await cancel.isDisplayed().catch(() => false));
    },
    { timeout: 120000, timeoutMsg: 're-compare after review branches did not finish' },
  );
  const err = await $('[data-testid="data-sync-error"]');
  if (await err.isDisplayed().catch(() => false)) {
    throw new Error(`re-compare error: ${await err.getText()}`);
  }
  await $('[data-testid="data-sync-summary"]').waitForDisplayed({ timeout: 15000 });
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

/** Validation branches reachable before a successful compare (driver-agnostic UI). */
export async function runPreCompareValidationBranches(
  f: SyncJourneyFixture,
  sameEndpointId: string,
  sameEndpointName: string,
) {
  await openDataSyncWindow();
  await captureStep(`${f.screenshotPrefix}-01-window-open`);

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

  await $('[data-testid="data-sync-compare"]').click();
  await browser.pause(500);
  const errSelectBoth = await $('[data-testid="data-sync-error"]');
  await expect(errSelectBoth).toBeDisplayed();
  expect(await errSelectBoth.getText()).toContain(t('sync.selectBoth'));
  await captureStep(`${f.screenshotPrefix}-04-select-both`);
  await dismissOkDialog();

  await selectDzOptionInWrap('data-sync-source', sameEndpointName);
  await selectDzOptionInWrap('data-sync-target', sameEndpointName);
  await browser.pause(800);
  await $('[data-testid="data-sync-compare"]').click();
  await browser.pause(500);
  const errSame = await $('[data-testid="data-sync-error"]');
  await expect(errSame).toBeDisplayed();
  expect(await errSame.getText()).toContain(t('sync.cannotSameDb'));
  await captureStep(`${f.screenshotPrefix}-05-same-endpoint`);
  await dismissOkDialog();

  await selectFixtureEndpoints(f);
  await captureStep(`${f.screenshotPrefix}-07-endpoints-selected`);

  try {
    await invokeBackend('delete_connection', { id: sameEndpointId });
  } catch {
    /* ok */
  }
}

export async function runEndpointSwapBranch(f: SyncJourneyFixture) {
  await $('[data-testid="data-sync-swap"]').click();
  await browser.pause(800);
  const err = await $('[data-testid="data-sync-error"]');
  expect(await err.isDisplayed().catch(() => false)).toBe(false);
  await captureStep(`${f.screenshotPrefix}-08-swap`);
  await $('[data-testid="data-sync-swap"]').click();
  await browser.pause(1500);
  await expect(await $('[data-testid="data-sync-source-database"]')).toBeDisplayed();
}

export async function runCompareCancelBranch(f: SyncJourneyFixture) {
  await $('[data-testid="data-sync-compare"]').click();
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
