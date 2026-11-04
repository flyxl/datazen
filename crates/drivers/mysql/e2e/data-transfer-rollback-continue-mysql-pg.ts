/** MySQL→PG Data Transfer journey: confirmed rollback continues when stop-on-error is disabled. */
import { expect, browser, $ } from '@wdio/globals';
import {
  advanceTransferWizardToPreview,
  clickTransferNext,
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
import { t } from '../../../../e2e/i18n.js';

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

describe('Data Transfer MySQL→PG confirmed rollback continuation', function () {
  this.timeout(180_000);
  const stamp = Date.now().toString(36);
  let mainWindow: string | undefined;
  const sourceId = `e2e_dt_rb_mpg_src_${stamp}`;
  const targetId = `e2e_dt_rb_mpg_tgt_${stamp}`;
  const sourceAdminId = `e2e_dt_rb_mpg_src_admin_${stamp}`;
  const targetAdminId = `e2e_dt_rb_mpg_tgt_admin_${stamp}`;
  const sourceDatabase = `dz_dt_rb_mpg_src_${stamp}`;
  const targetDatabase = `dz_dt_rb_mpg_tgt_${stamp}`;
  const sourceName = `DT rollback MySQL source ${stamp}`;
  const targetName = `DT rollback PG target ${stamp}`;
  const failedTable = `dt_rollback_a_fail_${stamp}`;
  const continuedTable = `dt_rollback_b_continue_${stamp}`;
  let sourceAdminConfigSaved = false;
  let targetAdminConfigSaved = false;
  let sourceConfigSaved = false;
  let targetConfigSaved = false;
  let sourceDatabaseCreated = false;
  let targetDatabaseCreated = false;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: mysqlConfig(sourceAdminId, `DT rollback admin MySQL ${stamp}`, 'mysql'),
    });
    sourceAdminConfigSaved = true;
    await invokeBackend('save_connection', {
      config: pgConfig(targetAdminId, `DT rollback admin PG ${stamp}`, 'postgres'),
    });
    targetAdminConfigSaved = true;

    let sourceAdminSession: string | undefined;
    let targetAdminSession: string | undefined;
    try {
      const sourceAdmin = await connectBackend(sourceAdminId);
      sourceAdminSession = sourceAdmin;
      const targetAdmin = await connectBackend(targetAdminId);
      targetAdminSession = targetAdmin;
      const sourceCatalog = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: sourceAdmin,
        sql: `SELECT SCHEMA_NAME FROM information_schema.SCHEMATA WHERE SCHEMA_NAME = '${sourceDatabase}'`,
      });
      const targetCatalog = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: targetAdmin,
        sql: `SELECT datname FROM pg_database WHERE datname = '${targetDatabase}'`,
      });
      if (parseQueryRows(sourceCatalog).length > 0 || parseQueryRows(targetCatalog).length > 0) {
        throw new Error(
          `Refusing to use pre-existing Transfer fixture database ${sourceDatabase} or ${targetDatabase}`,
        );
      }
      await withSafeModeOff(async () => {
        await invokeBackend('execute_query', {
          dbSessionId: sourceAdmin,
          sql: `CREATE DATABASE \`${sourceDatabase}\``,
        });
        sourceDatabaseCreated = true;
        await invokeBackend('execute_query', {
          dbSessionId: targetAdmin,
          sql: `CREATE DATABASE ${targetDatabase}`,
        });
        targetDatabaseCreated = true;
      });
    } finally {
      if (sourceAdminSession) await disconnectBackend(sourceAdminSession);
      if (targetAdminSession) await disconnectBackend(targetAdminSession);
    }

    await invokeBackend('save_connection', {
      config: mysqlConfig(sourceId, sourceName, sourceDatabase),
    });
    sourceConfigSaved = true;
    await invokeBackend('save_connection', {
      config: pgConfig(targetId, targetName, targetDatabase),
    });
    targetConfigSaved = true;

    let sourceSession: string | undefined;
    let targetSession: string | undefined;
    try {
      const source = await connectBackend(sourceId);
      sourceSession = source;
      const target = await connectBackend(targetId);
      targetSession = target;
      await withSafeModeOff(async () => {
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE TABLE ${failedTable} (id INT PRIMARY KEY, payload TEXT NOT NULL) ENGINE=InnoDB`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `INSERT INTO ${failedTable} VALUES (1, 'source row collides')`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: target,
          sql: `CREATE TABLE ${failedTable} (id INT PRIMARY KEY, payload TEXT NOT NULL)`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: target,
          sql: `INSERT INTO ${failedTable} VALUES (1, 'preserve this row')`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `CREATE TABLE ${continuedTable} (id INT PRIMARY KEY, payload TEXT NOT NULL) ENGINE=InnoDB`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: source,
          sql: `INSERT INTO ${continuedTable} VALUES (7, 'copied after rollback')`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: target,
          sql: `CREATE TABLE ${continuedTable} (id INT PRIMARY KEY, payload TEXT NOT NULL)`,
        });
      });
    } finally {
      if (sourceSession) await disconnectBackend(sourceSession);
      if (targetSession) await disconnectBackend(targetSession);
    }
  });

  after(async () => {
    if (mainWindow) {
      await browser.url('tauri://localhost').catch(() => undefined);
      await browser.pause(400);
      await closeExtraWindows(mainWindow).catch(() => undefined);
    }
    let sourceAdminSession: string | undefined;
    let targetAdminSession: string | undefined;
    const cleanupErrors: string[] = [];
    try {
      if (sourceDatabaseCreated && sourceAdminConfigSaved) {
        try {
          sourceAdminSession = await connectBackend(sourceAdminId);
        } catch (error) {
          cleanupErrors.push(
            `cannot reconnect MySQL admin for ${sourceDatabase}: ${String(error)}`,
          );
        }
      }
      if (targetDatabaseCreated && targetAdminConfigSaved) {
        try {
          targetAdminSession = await connectBackend(targetAdminId);
        } catch (error) {
          cleanupErrors.push(`cannot reconnect PG admin for ${targetDatabase}: ${String(error)}`);
        }
      }
      if (sourceDatabaseCreated && sourceAdminSession) {
        try {
          const sourceAdmin = sourceAdminSession;
          await withSafeModeOff(() =>
            invokeBackend('execute_query', {
              dbSessionId: sourceAdmin,
              sql: `DROP DATABASE IF EXISTS \`${sourceDatabase}\``,
            }),
          );
          sourceDatabaseCreated = false;
        } catch (error) {
          cleanupErrors.push(
            `failed to drop owned MySQL database ${sourceDatabase}: ${String(error)}`,
          );
        }
      }
      if (targetDatabaseCreated && targetAdminSession) {
        try {
          const targetAdmin = targetAdminSession;
          await withSafeModeOff(() =>
            invokeBackend('execute_query', {
              dbSessionId: targetAdmin,
              sql: `DROP DATABASE IF EXISTS ${targetDatabase} WITH (FORCE)`,
            }),
          );
          targetDatabaseCreated = false;
        } catch (error) {
          cleanupErrors.push(
            `failed to drop owned PG database ${targetDatabase}: ${String(error)}`,
          );
        }
      }
    } finally {
      if (sourceAdminSession) await disconnectBackend(sourceAdminSession);
      if (targetAdminSession) await disconnectBackend(targetAdminSession);
      if (sourceConfigSaved)
        await invokeBackend('delete_connection', { id: sourceId }).catch(() => undefined);
      if (targetConfigSaved)
        await invokeBackend('delete_connection', { id: targetId }).catch(() => undefined);
      if (sourceAdminConfigSaved)
        await invokeBackend('delete_connection', { id: sourceAdminId }).catch(() => undefined);
      if (targetAdminConfigSaved)
        await invokeBackend('delete_connection', { id: targetAdminId }).catch(() => undefined);
      if (mainWindow) await closeExtraWindows(mainWindow).catch(() => undefined);
    }
    const remainingConnections = await invokeBackend<Array<{ id: string }>>('get_connections');
    const remainingFixtureIds = remainingConnections
      .filter(({ id }) => [sourceId, targetId, sourceAdminId, targetAdminId].includes(id))
      .map(({ id }) => id);
    expect(remainingFixtureIds).toEqual([]);
    if (cleanupErrors.length > 0) {
      throw new Error(`Transfer fixture cleanup incomplete: ${cleanupErrors.join('; ')}`);
    }
  });

  it('rolls back the conflicting table and copies the later table', async () => {
    await openDataTransferWindow();
    await selectDzOptionInWrap('data-transfer-source', sourceName);
    await selectDzOptionInWrap('data-transfer-target', targetName);
    await browser.pause(1_000);
    await clickTransferNext();

    await (await $('[data-testid="data-transfer-mode-data"]')).click();
    const stopOnError = await $(`label*=${t('transfer.stopOnError')}`).$('input[type="checkbox"]');
    if (await stopOnError.isSelected()) await stopOnError.click();
    expect(await stopOnError.isSelected()).toBe(false);
    await clickTransferNext();

    for (const table of [failedTable, continuedTable]) {
      const row = await $(`[data-testid="data-transfer-table-row"]*=${table}`);
      await row.waitForDisplayed({ timeout: 20_000 });
      const checkbox = await row.$('input[type="checkbox"]');
      if (!(await checkbox.isSelected())) await checkbox.click();
    }
    await clickTransferNext();
    await advanceTransferWizardToPreview();

    const execute = await $('[data-testid="data-transfer-execute"]');
    await execute.waitForClickable({ timeout: 20_000 });
    await execute.click();
    const result = await $('[data-testid="data-transfer-result"]');
    await result.waitForDisplayed({ timeout: 30_000 });
    expect(await result.getText()).toContain(failedTable);
    expect(await result.getText()).toContain(continuedTable);
    expect(
      await (
        await $(`[data-testid="data-transfer-table-result-${failedTable}"]`)
      ).getAttribute('data-outcome'),
    ).toBe('rolledBack');
    expect(
      await (
        await $(`[data-testid="data-transfer-table-result-${continuedTable}"]`)
      ).getAttribute('data-outcome'),
    ).toBe('committed');

    const targetSession = await connectBackend(targetId);
    try {
      const preserved = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: targetSession,
        sql: `SELECT COUNT(*) AS c FROM ${failedTable}`,
      });
      const copied = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: targetSession,
        sql: `SELECT COUNT(*) AS c FROM ${continuedTable}`,
      });
      expect(queryScalar(preserved, 'c')).toBe(1);
      expect(queryScalar(copied, 'c')).toBe(1);
    } finally {
      await disconnectBackend(targetSession);
    }
  });
});
