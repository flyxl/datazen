/**
 * Data Transfer mode coverage matrix: structure / data / both × dialect pairs.
 *
 * Verifies each selectable transfer mode reaches preview (and execute for data paths).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  invokeBackend,
  queryScalar,
  selectDzOption,
  withSafeModeOff,
  type QueryResultPayload,
} from '../helpers.js';

type TransferModeUi = 'data' | 'structure' | 'both';
type Pair = 'pg-pg' | 'pg-mysql' | 'mysql-pg';

function pgConfig(id: string, name: string, database: string) {
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

function mysqlConfig(id: string, name: string, database: string) {
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

async function openTransferWindow() {
  await browser.url('tauri://localhost/window.html?window=data-transfer');
  await browser.pause(1500);
  await $('[data-testid="data-transfer-window"]').waitForDisplayed({ timeout: 10000 });
}

async function clickNext() {
  const next = await $('[data-testid="data-transfer-next"]');
  await next.waitForClickable({ timeout: 10000 });
  await next.click();
  await browser.pause(1200);
}

async function advanceToPreview() {
  for (let i = 0; i < 10; i++) {
    const preview = await $('[data-testid="data-transfer-preview"]');
    if (await preview.isDisplayed().catch(() => false)) return;
    const execute = await $('[data-testid="data-transfer-execute"]');
    if (await execute.isExisting().catch(() => false)) {
      await clickNext();
      return;
    }
    await clickNext();
  }
}

async function runWizard(
  srcName: string,
  tgtName: string,
  table: string,
  mode: TransferModeUi,
  createNew: boolean,
) {
  await openTransferWindow();
  await selectDzOption(t('transfer.pickConnection'), srcName);
  await selectDzOption(t('transfer.pickConnection'), tgtName);
  await browser.pause(1500);
  await clickNext();

  const modeTestId =
    mode === 'data'
      ? 'data-transfer-mode-data'
      : mode === 'structure'
        ? 'data-transfer-mode-structure'
        : 'data-transfer-mode-both';
  await (await $(`[data-testid="${modeTestId}"]`)).click();
  await browser.pause(300);
  await clickNext();

  await browser.pause(2000);
  const tableRow = await $(`[data-testid="data-transfer-table-row"]*=${table}`);
  await tableRow.waitForDisplayed({ timeout: 15000 });
  const checkbox = await tableRow.$('input[type="checkbox"]');
  if (!(await checkbox.isSelected())) await checkbox.click();
  await clickNext();

  if (createNew) {
    await $('[data-testid="data-transfer-mapping-step"]').waitForDisplayed({ timeout: 15000 });
    const toggle = await $('[data-testid="data-transfer-create-new-toggle"]');
    if (await toggle.isExisting()) {
      if (!(await toggle.isSelected())) await toggle.click();
      const targetTableInput = await $('[data-testid="data-transfer-target-table-input"]');
      await targetTableInput.click();
      await browser.keys(['Tab']);
      await browser.pause(1500);
    }
  }

  for (let i = 0; i < 6; i++) {
    const preview = await $('[data-testid="data-transfer-preview"]');
    if (await preview.isDisplayed().catch(() => false)) break;
    await clickNext();
  }
  await $('[data-testid="data-transfer-preview"]').waitForDisplayed({ timeout: 15000 });
}

describe('数据传输模式路径矩阵 (DT-MODE-MATRIX)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);

  const pairs: Array<{
    pair: Pair;
    srcId: string;
    tgtId: string;
    srcName: string;
    tgtName: string;
    table: string;
    srcDb: string;
    tgtDb: string;
  }> = [
    {
      pair: 'pg-pg',
      srcId: `e2e_dt_mm_pg_src_${STAMP}`,
      tgtId: `e2e_dt_mm_pg_tgt_${STAMP}`,
      srcName: `DT-MM-PG-Src-${STAMP}`,
      tgtName: `DT-MM-PG-Tgt-${STAMP}`,
      table: `dt_mm_pg_${STAMP}`,
      srcDb: 'datazen_sync_src',
      tgtDb: 'datazen_sync_tgt',
    },
    {
      pair: 'pg-mysql',
      srcId: `e2e_dt_mm_pgm_src_${STAMP}`,
      tgtId: `e2e_dt_mm_pgm_tgt_${STAMP}`,
      srcName: `DT-MM-PGM-Src-${STAMP}`,
      tgtName: `DT-MM-PGM-Tgt-${STAMP}`,
      table: `dt_mm_pgm_${STAMP}`,
      srcDb: 'datazen_sync_src',
      tgtDb: 'datazen_sync_mysql_tgt',
    },
    {
      pair: 'mysql-pg',
      srcId: `e2e_dt_mm_mpg_src_${STAMP}`,
      tgtId: `e2e_dt_mm_mpg_tgt_${STAMP}`,
      srcName: `DT-MM-MPG-Src-${STAMP}`,
      tgtName: `DT-MM-MPG-Tgt-${STAMP}`,
      table: `dt_mm_mpg_${STAMP}`,
      srcDb: 'datazen_sync_mysql_tgt',
      tgtDb: 'datazen_sync_tgt',
    },
  ];

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });

    for (const p of pairs) {
      if (p.pair === 'mysql-pg') {
        await invokeBackend('save_connection', {
          config: mysqlConfig(p.srcId, p.srcName, p.srcDb),
        });
        await invokeBackend('save_connection', {
          config: pgConfig(p.tgtId, p.tgtName, p.tgtDb),
        });
      } else if (p.pair === 'pg-mysql') {
        await invokeBackend('save_connection', {
          config: pgConfig(p.srcId, p.srcName, p.srcDb),
        });
        await invokeBackend('save_connection', {
          config: mysqlConfig(p.tgtId, p.tgtName, p.tgtDb),
        });
      } else {
        await invokeBackend('save_connection', {
          config: pgConfig(p.srcId, p.srcName, p.srcDb),
        });
        await invokeBackend('save_connection', {
          config: pgConfig(p.tgtId, p.tgtName, p.tgtDb),
        });
      }

      const srcSession = await invokeBackend<string>('connect', { connectionId: p.srcId });
      const tgtSession = await invokeBackend<string>('connect', { connectionId: p.tgtId });

      await withSafeModeOff(async () => {
        await invokeBackend('execute_query', {
          dbSessionId: srcSession,
          sql: `DROP TABLE IF EXISTS ${p.table}`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: tgtSession,
          sql: `DROP TABLE IF EXISTS ${p.table}`,
        });
        const pgCreate = `CREATE TABLE ${p.table} (id INT PRIMARY KEY, name TEXT NOT NULL)`;
        const mysqlCreate = `CREATE TABLE ${p.table} (id INT PRIMARY KEY, name VARCHAR(255) NOT NULL)`;
        const pgInsert = `INSERT INTO ${p.table} (id, name) VALUES (1,'one'),(2,'two')`;
        const mysqlInsert = pgInsert;

        if (p.pair === 'pg-pg' || p.pair === 'pg-mysql') {
          await invokeBackend('execute_query', { dbSessionId: srcSession, sql: pgCreate });
          await invokeBackend('execute_query', { dbSessionId: srcSession, sql: pgInsert });
        } else {
          await invokeBackend('execute_query', { dbSessionId: srcSession, sql: mysqlCreate });
          await invokeBackend('execute_query', { dbSessionId: srcSession, sql: mysqlInsert });
        }

        if (p.pair === 'pg-mysql') {
          await invokeBackend('execute_query', { dbSessionId: tgtSession, sql: mysqlCreate });
        } else if (p.pair === 'mysql-pg' || p.pair === 'pg-pg') {
          await invokeBackend('execute_query', { dbSessionId: tgtSession, sql: pgCreate });
        }
      });
    }
  });

  after(async () => {
    for (const p of pairs) {
      try {
        const srcSession = await invokeBackend<string>('connect', { connectionId: p.srcId });
        const tgtSession = await invokeBackend<string>('connect', { connectionId: p.tgtId });
        await withSafeModeOff(async () => {
          await invokeBackend('execute_query', {
            dbSessionId: srcSession,
            sql: `DROP TABLE IF EXISTS ${p.table}`,
          });
          await invokeBackend('execute_query', {
            dbSessionId: tgtSession,
            sql: `DROP TABLE IF EXISTS ${p.table}`,
          });
        });
      } catch {
        /* ok */
      }
      try {
        await invokeBackend('delete_connection', { id: p.srcId });
      } catch {
        /* ok */
      }
      try {
        await invokeBackend('delete_connection', { id: p.tgtId });
      } catch {
        /* ok */
      }
    }
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  for (const p of pairs) {
    for (const mode of ['data', 'structure', 'both'] as TransferModeUi[]) {
      const createNew = mode !== 'data';
      it(`DT-MODE-${p.pair}-${mode}: ${p.pair} ${mode} 模式应到达 Preview`, async () => {
        await runWizard(p.srcName, p.tgtName, p.table, mode, createNew);
        const preview = await $('[data-testid="data-transfer-preview"]');
        await expect(preview).toBeDisplayed();
      });
    }
  }

  it('DT-MODE-pg-pg-data-exec: PG→PG data 模式执行后行数=2', async () => {
    const p = pairs[0];
    await runWizard(p.srcName, p.tgtName, p.table, 'data', false);
    await advanceToPreview();
    await clickNext();
    const execute = await $('[data-testid="data-transfer-execute"]');
    await execute.waitForClickable({ timeout: 10000 });
    await execute.click();
    await browser.pause(2000);
    await $('[data-testid="data-transfer-result"]').waitForDisplayed({ timeout: 15000 });

    const tgtSession = await invokeBackend<string>('connect', { connectionId: p.tgtId });
    const rows = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: tgtSession,
      sql: `SELECT count(*)::int AS c FROM ${p.table}`,
    });
    expect(queryScalar(rows, 'c')).toBe(2);
  });
});
