/**
 * Data Transfer: limitations UI + diverse column types + bulk row counts.
 *
 * Covers PG→MySQL and MySQL→PG (IR path), 100 rows per direction.
 * Requires `bash e2e/setup-sync-dbs.sh`.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  invokeBackend,
  queryScalar,
  parseQueryRows,
  selectDzOption,
  withSafeModeOff,
  type QueryResultPayload,
} from '../helpers.js';

const ROW_COUNT = 100;

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

function pgCreateTableSql(table: string): string {
  return `CREATE TABLE ${table} (
    id BIGINT PRIMARY KEY,
    qty INT NOT NULL,
    big_qty BIGINT,
    amount DECIMAL(12,4) NOT NULL DEFAULT 0,
    label VARCHAR(128) NOT NULL DEFAULT 'unnamed',
    body TEXT,
    meta JSONB,
    active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now()
  )`;
}

function mysqlCreateTableSql(table: string): string {
  return `CREATE TABLE ${table} (
    id BIGINT PRIMARY KEY,
    qty INT NOT NULL,
    big_qty BIGINT,
    amount DECIMAL(12,4) NOT NULL DEFAULT 0,
    label VARCHAR(128) NOT NULL DEFAULT 'unnamed',
    body TEXT,
    meta JSON,
    active TINYINT(1) NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL
  )`;
}

function pgBulkInsertSql(table: string, count: number): string {
  return `INSERT INTO ${table} (id, qty, big_qty, amount, label, body, meta, active, created_at)
    SELECT
      g,
      (g % 50)::int,
      g * 1000000,
      (g * 1.2345)::decimal(12,4),
      'label_' || g,
      'body text ' || g,
      jsonb_build_object('n', g),
      (g % 2 = 0),
      '2024-01-01'::timestamptz + (g || ' hours')::interval
    FROM generate_series(1, ${count}) g`;
}

function mysqlBulkInsertSql(table: string, count: number): string {
  const values = Array.from({ length: count }, (_, i) => {
    const g = i + 1;
    const amount = (g * 1.2345).toFixed(4);
    return `(${g}, ${g % 50}, ${g * 1000000}, ${amount}, 'label_${g}', 'body text ${g}', JSON_OBJECT('n', ${g}), ${g % 2}, DATE_ADD('2024-01-01', INTERVAL ${g} HOUR))`;
  }).join(',\n');
  return `INSERT INTO ${table} (id, qty, big_qty, amount, label, body, meta, active, created_at) VALUES ${values}`;
}

async function openTransferWindow() {
  await browser.url('tauri://localhost/window.html?window=data-transfer');
  await browser.pause(1500);
  await $('[data-testid="data-transfer-window"]').waitForDisplayed({ timeout: 10000 });
  await $('[data-testid="data-transfer-step-endpoints"]').waitForDisplayed({ timeout: 10000 });
}

async function clickNext(label: string) {
  const next = await $('[data-testid="data-transfer-next"]');
  await next.waitForClickable({ timeout: 8000 });
  await next.click();
  await browser.pause(1200);
  await captureJourneyStep(label, 0, true);
}

async function runDataTransferWizard(sourceName: string, targetName: string, tableName: string) {
  await openTransferWindow();
  await selectDzOption(t('transfer.pickConnection'), sourceName);
  await selectDzOption(t('transfer.pickConnection'), targetName);
  await browser.pause(1500);
  await clickNext('dt-div-endpoints');

  const dataMode = await $('[data-testid="data-transfer-mode-data"]');
  await dataMode.click();
  await browser.pause(300);
  await clickNext('dt-div-mode');

  await browser.pause(2000);
  const tableRow = await $(`[data-testid="data-transfer-table-row"]*=${tableName}`);
  await tableRow.waitForDisplayed({ timeout: 15000 });
  const checkbox = await tableRow.$('input[type="checkbox"]');
  if (!(await checkbox.isSelected())) {
    await checkbox.click();
  }
  await clickNext('dt-div-objects');

  for (let i = 0; i < 10; i++) {
    const execute = await $('[data-testid="data-transfer-execute"]');
    if (await execute.isExisting().catch(() => false)) {
      await captureJourneyStep('dt-div-execute-ready', 0, true);
      break;
    }
    const next = await $('[data-testid="data-transfer-next"]');
    if (!(await next.isEnabled())) await browser.pause(1000);
    await clickNext(`dt-div-advance-${i}`);
  }

  const execute = await $('[data-testid="data-transfer-execute"]');
  await execute.waitForClickable({ timeout: 15000 });
  await execute.click();
  await browser.pause(2500);

  const result = await $('[data-testid="data-transfer-result"]');
  await result.waitForDisplayed({ timeout: 20000 });
  const resultText = await result.getText();
  expect(resultText).not.toContain(t('transfer.error'));
}

async function assertTargetRows(
  connectionId: string,
  table: string,
  dialect: 'postgresql' | 'mysql',
) {
  const session = await invokeBackend<string>('connect', { connectionId });
  const countSql =
    dialect === 'postgresql'
      ? `SELECT count(*)::int AS c FROM ${table}`
      : `SELECT COUNT(*) AS c FROM ${table}`;
  const rows = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: countSql,
  });
  expect(queryScalar(rows, 'c')).toBe(ROW_COUNT);

  const sumSql =
    dialect === 'postgresql'
      ? `SELECT sum(qty)::int AS s FROM ${table}`
      : `SELECT SUM(qty) AS s FROM ${table}`;
  const sumRows = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: sumSql,
  });
  const expectedSum = Array.from({ length: ROW_COUNT }, (_, i) => (i + 1) % 50).reduce(
    (a, b) => a + b,
    0,
  );
  expect(Number(queryScalar(sumRows, 's'))).toBe(expectedSum);

  const amountSql =
    dialect === 'postgresql'
      ? `SELECT amount::float AS a FROM ${table} WHERE id = 50`
      : `SELECT CAST(amount AS DECIMAL(12,4)) AS a FROM ${table} WHERE id = 50`;
  const amountRows = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: amountSql,
  });
  expect(Number(queryScalar(amountRows, 'a'))).toBeCloseTo(50 * 1.2345, 3);

  const activeSql = `SELECT active FROM ${table} WHERE id = 50`;
  const activeRows = await invokeBackend<QueryResultPayload>('execute_query', {
    dbSessionId: session,
    sql: activeSql,
  });
  const legacy = activeRows.data?.[0] as Record<string, unknown> | undefined;
  const activeVal = legacy?.active ?? parseQueryRows(activeRows)[0]?.[0];
  expect(String(activeVal)).toMatch(/^(t|f|true|false|1|0)$/i);
}

describe('数据传输限制说明与多类型大批量 (DT-DIV)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DT-LIM-001: 应显示当前版本限制说明', async () => {
    await openTransferWindow();
    const panel = await $('[data-testid="data-transfer-limitations"]');
    await panel.waitForDisplayed({ timeout: 8000 });
    const text = await panel.getText();
    expect(text).toContain(t('transfer.limitations.title'));
    expect(text).toContain(t('transfer.limitations.noFkIndexes'));
    expect(text).toContain(t('transfer.limitations.noViews'));
    await captureJourneyStep('dt-div-limitations', 0, true);
  });
});

describe('PG→MySQL 多类型 100 行 (DT-DIV-PG-MYSQL)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_dt_div_pg_${STAMP}`;
  const TGT_ID = `e2e_dt_div_mysql_${STAMP}`;
  const SRC_NAME = `DT-Div-PG-${STAMP}`;
  const TGT_NAME = `DT-Div-MySQL-${STAMP}`;
  const TABLE = `dt_div_pg_mysql_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
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
        sql: pgCreateTableSql(TABLE),
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: pgBulkInsertSql(TABLE, ROW_COUNT),
      });
      await invokeBackend('execute_query', {
        dbSessionId: tgtSession,
        sql: mysqlCreateTableSql(TABLE),
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

  it(`DT-DIV-001: PG→MySQL 应迁移 ${ROW_COUNT} 行且类型值正确`, async () => {
    await runDataTransferWizard(SRC_NAME, TGT_NAME, TABLE);
    await assertTargetRows(TGT_ID, TABLE, 'mysql');
    await captureJourneyStep('dt-div-pg-mysql-done', 0, true);
  });
});

describe('MySQL→PG 多类型 100 行 (DT-DIV-MYSQL-PG)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_dt_div_mysql_src_${STAMP}`;
  const TGT_ID = `e2e_dt_div_pg_tgt_${STAMP}`;
  const SRC_NAME = `DT-Div-MySQL-Src-${STAMP}`;
  const TGT_NAME = `DT-Div-PG-Tgt-${STAMP}`;
  const TABLE = `dt_div_mysql_pg_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: mysqlConfig(SRC_ID, SRC_NAME, 'datazen_sync_mysql_tgt'),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(TGT_ID, TGT_NAME, 'datazen_sync_tgt'),
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
        sql: mysqlCreateTableSql(TABLE),
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: mysqlBulkInsertSql(TABLE, ROW_COUNT),
      });
      await invokeBackend('execute_query', {
        dbSessionId: tgtSession,
        sql: `CREATE TABLE ${TABLE} (
          id BIGINT PRIMARY KEY,
          qty INT NOT NULL,
          big_qty BIGINT,
          amount DECIMAL(12,4) NOT NULL DEFAULT 0,
          label VARCHAR(128) NOT NULL DEFAULT 'unnamed',
          body TEXT,
          meta JSONB,
          active BOOLEAN NOT NULL DEFAULT true,
          created_at TIMESTAMPTZ NOT NULL
        )`,
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

  it(`DT-DIV-002: MySQL→PG 应迁移 ${ROW_COUNT} 行且类型值正确`, async () => {
    await runDataTransferWizard(SRC_NAME, TGT_NAME, TABLE);
    await assertTargetRows(TGT_ID, TABLE, 'postgresql');
    await captureJourneyStep('dt-div-mysql-pg-done', 0, true);
  });
});
