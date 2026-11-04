/** Independent PostgreSQL Data Transfer journey against isolated migration databases. */
import { expect, browser, $ } from '@wdio/globals';
import {
  closeExtraWindows,
  connectBackend,
  disconnectBackend,
  invokeBackend,
  openDataTransferWindow,
  parseQueryRows,
  queryScalar,
  selectDzOptionInWrap,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../../../e2e/helpers.js';

const SOURCE_DATABASE = 'dz_mig_0910_transfer_src';
const TARGET_DATABASE = 'dz_mig_0910_transfer_tgt';

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

describe('PostgreSQL Data Transfer wave1 isolated journey', function () {
  this.timeout(180_000);
  let mainWindow: string;
  const stamp = Date.now().toString(36);
  const sourceId = `e2e_transfer_tester_source_${stamp}`;
  const targetId = `e2e_transfer_tester_target_${stamp}`;
  const sourceName = `Transfer tester source ${stamp}`;
  const targetName = `Transfer tester target ${stamp}`;
  const table = `dt_tester_${stamp}`;

  async function next(): Promise<void> {
    const button = await $('[data-testid="data-transfer-next"]');
    await button.waitForClickable({ timeout: 15_000 });
    await button.click();
    await browser.pause(1_000);
  }

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: pgConfig(sourceId, sourceName, SOURCE_DATABASE),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(targetId, targetName, TARGET_DATABASE),
    });
    const sourceSession = await connectBackend(sourceId);
    const targetSession = await connectBackend(targetId);
    try {
      await withSafeModeOff(async () => {
        await invokeBackend('execute_query', {
          dbSessionId: sourceSession,
          sql: `CREATE TABLE ${table} (id integer PRIMARY KEY, skipped text, payload bytea, amount numeric(65,30))`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: sourceSession,
          sql: `INSERT INTO ${table} VALUES (1, 'do not copy', decode('00ff80','hex'), 12345678901234567890123456789012345.123456789012345678901234567890)`,
        });
        // Deliberately reorder the physical target columns and omit one source column.
        await invokeBackend('execute_query', {
          dbSessionId: targetSession,
          sql: `CREATE TABLE ${table} (amount numeric(65,30), payload bytea, id integer PRIMARY KEY)`,
        });
      });
    } finally {
      await disconnectBackend(sourceSession);
      await disconnectBackend(targetSession);
    }
  });

  after(async () => {
    try {
      const sourceSession = await connectBackend(sourceId);
      const targetSession = await connectBackend(targetId);
      try {
        await withSafeModeOff(async () => {
          await invokeBackend('execute_query', {
            dbSessionId: sourceSession,
            sql: `DROP TABLE IF EXISTS ${table}`,
          });
          await invokeBackend('execute_query', {
            dbSessionId: targetSession,
            sql: `DROP TABLE IF EXISTS ${table}`,
          });
        });
      } finally {
        await disconnectBackend(sourceSession);
        await disconnectBackend(targetSession);
      }
    } catch {
      // Teardown is best-effort; every object name is isolated to this test run.
    }
    await invokeBackend('delete_connection', { id: sourceId }).catch(() => undefined);
    await invokeBackend('delete_connection', { id: targetId }).catch(() => undefined);
    await closeExtraWindows(mainWindow);
  });

  it('test_tester transfers a subset with physical reordering, bytes, and 65-digit numeric intact', async () => {
    await openDataTransferWindow();
    await selectDzOptionInWrap('data-transfer-source', sourceName);
    await selectDzOptionInWrap('data-transfer-target', targetName);
    await browser.pause(1_000);
    await next();
    await (await $('[data-testid="data-transfer-mode-data"]')).click();
    await next();

    const row = await $(`[data-testid="data-transfer-table-row"]*=${table}`);
    await row.waitForDisplayed({ timeout: 20_000 });
    const checkbox = await row.$('input[type="checkbox"]');
    if (!(await checkbox.isSelected())) await checkbox.click();
    await next();

    await $('[data-testid="data-transfer-column-editor"]').waitForDisplayed({ timeout: 20_000 });
    expect(await $('[data-testid="data-transfer-skip-skipped"]').isSelected()).toBe(true);

    for (let attempt = 0; attempt < 6; attempt += 1) {
      const execute = await $('[data-testid="data-transfer-execute"]');
      if (await execute.isExisting().catch(() => false)) break;
      await next();
    }
    const execute = await $('[data-testid="data-transfer-execute"]');
    await execute.waitForClickable({ timeout: 20_000 });
    await execute.click();
    const result = await $('[data-testid="data-transfer-result"]');
    await result.waitForDisplayed({ timeout: 30_000 });
    expect(await result.getText()).toContain('1');

    const targetSession = await connectBackend(targetId);
    try {
      const count = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: targetSession,
        sql: `SELECT count(*)::int AS value FROM ${table}`,
      });
      expect(queryScalar(count, 'value')).toBe(1);
      const values = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: targetSession,
        sql: `SELECT id, encode(payload, 'hex') AS payload_hex, amount::text AS amount FROM ${table}`,
      });
      const copied = parseQueryRows(values)[0];
      expect(copied?.[0]).toBe(1);
      expect(copied?.[1]).toBe('00ff80');
      expect(copied?.[2]).toBe(
        '12345678901234567890123456789012345.123456789012345678901234567890',
      );
    } finally {
      await disconnectBackend(targetSession);
    }
  });
});
