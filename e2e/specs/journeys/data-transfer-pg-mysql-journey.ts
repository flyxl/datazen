/**
 * Data Transfer cross-dialect journey: PostgreSQL → MySQL (IR path).
 *
 * Requires `e2e/setup-sync-dbs.sh` (datazen_sync_src + datazen_sync_mysql_tgt).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  invokeBackend,
  openDataTransferWindow,
  queryScalar,
  selectDzOption,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../helpers.js';

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

describe('数据传输 PG→MySQL 跨方言旅程 (DT-PG-MYSQL-JOURNEY)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_dt_pg_src_${STAMP}`;
  const TGT_ID = `e2e_dt_mysql_tgt_${STAMP}`;
  const SRC_NAME = `DT-PG-Src-${STAMP}`;
  const TGT_NAME = `DT-MySQL-Tgt-${STAMP}`;
  const TABLE = `dt_pg_mysql_${STAMP}`;

  async function clickNext(stepLabel: string) {
    const next = await $('[data-testid="data-transfer-next"]');
    await next.waitForClickable({ timeout: 8000 });
    await next.click();
    await browser.pause(1200);
    await captureJourneyStep(stepLabel, 0, true);
  }

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });

    await invokeBackend('save_connection', {
      config: pgConfig(SRC_ID, SRC_NAME, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: mysqlConfig(TGT_ID, TGT_NAME, 'datazen_sync_mysql_tgt'),
    });

    const srcSession = await invokeBackend<string>('connect', { connectionId: SRC_ID });
    const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });

    await withSafeModeOff(async () => {
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `DROP TABLE IF EXISTS ${TABLE}`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: tgtSession,
        sql: `DROP TABLE IF EXISTS ${TABLE}`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL, qty int)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `INSERT INTO ${TABLE} (id, name, qty) VALUES (1,'a',10),(2,'b',20),(3,'c',30)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: tgtSession,
        sql: `CREATE TABLE ${TABLE} (id INT PRIMARY KEY, name VARCHAR(255) NOT NULL, qty INT)`,
      });
    });
  });

  after(async () => {
    try {
      const srcSession = await invokeBackend<string>('connect', { connectionId: SRC_ID });
      const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });
      await withSafeModeOff(async () => {
        await invokeBackend('execute_query', {
          dbSessionId: srcSession,
          sql: `DROP TABLE IF EXISTS ${TABLE}`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: tgtSession,
          sql: `DROP TABLE IF EXISTS ${TABLE}`,
        });
      });
    } catch {
      /* ok */
    }
    try {
      await invokeBackend('delete_connection', { id: SRC_ID });
    } catch {
      /* ok */
    }
    try {
      await invokeBackend('delete_connection', { id: TGT_ID });
    } catch {
      /* ok */
    }
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('Step 1: 通过 URL 打开数据传输窗口', async () => {
    await openDataTransferWindow();
    await expect(await $('[data-testid="data-transfer-window"]')).toBeDisplayed();
    await captureJourneyStep('dt-pg-mysql-01-window-open', 0, true);
  });

  it('Step 2: 选择 PG 源与 MySQL 目标并显示 IR 路径', async () => {
    await openDataTransferWindow();
    await selectDzOption(t('transfer.pickConnection'), SRC_NAME);
    await selectDzOption(t('transfer.pickConnection'), TGT_NAME);
    await browser.pause(1500);
    await expect(await $('[data-testid="data-transfer-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-target-database"]')).toBeDisplayed();
    const path = await $('[data-testid="data-transfer-path"]');
    await expect(path).toBeDisplayed();
    expect(await path.getText()).toContain(t('transfer.path.ir'));
    await captureJourneyStep('dt-pg-mysql-02-ir-path', 0, true);
  });

  it('Step 3: 进入 setup 并选择 data 模式', async () => {
    await clickNext('dt-pg-mysql-03-setup-step');
    await expect(await $('[data-testid="data-transfer-mode-data"]')).toBeDisplayed();
    const dataMode = await $('[data-testid="data-transfer-mode-data"]');
    await dataMode.click();
    await browser.pause(300);
    await captureJourneyStep('dt-pg-mysql-04-data-mode', 0, true);
  });

  it('Step 4: 进入对象选择并触发 inspect', async () => {
    await clickNext('dt-pg-mysql-05-objects-step');
    await browser.pause(2000);
    const body = await $('body').getText();
    expect(body).toContain(TABLE);
    await captureJourneyStep('dt-pg-mysql-06-objects-inspected', 0, true);
  });

  it('Step 5: 推进映射与预览直至执行步骤', async () => {
    for (let i = 0; i < 8; i++) {
      const execute = await $('[data-testid="data-transfer-execute"]');
      if (await execute.isExisting().catch(() => false)) {
        await captureJourneyStep('dt-pg-mysql-07-execute-ready', 0, true);
        break;
      }
      const preview = await $('[data-testid="data-transfer-preview"]');
      if (await preview.isDisplayed().catch(() => false)) {
        await captureJourneyStep('dt-pg-mysql-07-preview', 0, true);
      }
      const next = await $('[data-testid="data-transfer-next"]');
      if (!(await next.isEnabled())) await browser.pause(1000);
      await clickNext(`dt-pg-mysql-08-advance-${i}`);
    }
  });

  it('Step 6: 执行跨方言传输并验证 MySQL 目标行数', async () => {
    const execute = await $('[data-testid="data-transfer-execute"]');
    await execute.waitForClickable({ timeout: 15000 });
    await execute.click();
    await browser.pause(2000);
    await captureJourneyStep('dt-pg-mysql-09-executed', 0, true);

    const result = await $('[data-testid="data-transfer-result"]');
    await result.waitForDisplayed({ timeout: 20000 });
    const resultText = await result.getText();
    expect(resultText).not.toContain(t('transfer.error'));
    await captureJourneyStep('dt-pg-mysql-10-result', 0, true);

    const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });
    const rows = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: tgtSession,
      sql: `SELECT COUNT(*) AS c FROM ${TABLE}`,
    });
    expect(queryScalar(rows, 'c')).toBe(3);
    await captureJourneyStep('dt-pg-mysql-11-mysql-rows-verified', 0, true);
  });
});
