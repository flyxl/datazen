/**
 * PostgreSQL Data Transfer FK-order journey.
 *
 * Requires the local E2E sync databases from e2e/setup-sync-dbs.sh.
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  closeExtraWindows,
  connectBackend,
  disconnectBackend,
  invokeBackend,
  queryScalar,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../helpers.js';
import type {
  TransferExecutionResult,
  TransferJob,
  TransferPreview,
} from '../../../packages/frontend/src/commands/transfer';

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
    schema: 'public',
    sslMode: 'disable',
  };
}

describe('PostgreSQL Data Transfer foreign-key ordering', () => {
  let mainWindow: string;
  let sourceSession: string | undefined;
  let targetSession: string | undefined;
  const stamp = Date.now().toString(36);
  const sourceConnectionId = `e2e_fk_order_src_${stamp}`;
  const targetConnectionId = `e2e_fk_order_tgt_${stamp}`;
  const sourceName = `FK-order-source-${stamp}`;
  const targetName = `FK-order-target-${stamp}`;
  const parent = `dt_fk_parent_${stamp}`;
  const child = `dt_fk_child_${stamp}`;
  const cycleA = `dt_fk_cycle_a_${stamp}`;
  const cycleB = `dt_fk_cycle_b_${stamp}`;
  const fixtureTables = [parent, child, cycleA, cycleB];

  async function runSql(sessionId: string, sql: string) {
    await invokeBackend('execute_query', { dbSessionId: sessionId, sql });
  }

  async function preview(tables: string[]): Promise<TransferPreview> {
    const job: TransferJob = {
      source: { dbSessionId: sourceSession!, database: 'datazen_sync_src', schema: 'public' },
      target: { dbSessionId: targetSession!, database: 'datazen_sync_tgt', schema: 'public' },
      mode: 'data',
      writeMode: 'insert',
      tables: tables.map((sourceTable) => ({
        sourceTable,
        targetTable: sourceTable,
        enabled: true,
      })),
      options: { batchSize: 50, stopOnError: true },
    };
    return invokeBackend<TransferPreview>('preview_data_transfer', { job });
  }

  async function execute(
    planId: string,
    sourceTables?: string[],
  ): Promise<TransferExecutionResult> {
    return invokeBackend<TransferExecutionResult>('execute_data_transfer', {
      request: {
        planId,
        ...(sourceTables ? { selection: { sourceTables } } : {}),
      },
    });
  }

  async function rowCount(sessionId: string, table: string): Promise<number> {
    const result = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: sessionId,
      sql: `SELECT count(*)::int AS count FROM ${table}`,
    });
    return queryScalar(result, 'count');
  }

  async function remainingFixtureCount(sessionId: string): Promise<number> {
    const names = fixtureTables.map((table) => `'${table}'`).join(', ');
    const result = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: sessionId,
      sql: `SELECT count(*)::int AS count FROM information_schema.tables WHERE table_schema = 'public' AND table_name IN (${names})`,
    });
    return queryScalar(result, 'count');
  }

  async function expectCommandError(invoke: () => Promise<unknown>): Promise<string> {
    let message = '';
    try {
      await invoke();
    } catch (error) {
      message = error instanceof Error ? error.message : String(error);
    }
    expect(message).not.toBe('');
    return message;
  }

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
    await invokeBackend('save_connection', {
      config: pgConfig(sourceConnectionId, sourceName, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(targetConnectionId, targetName, 'datazen_sync_tgt'),
    });
    sourceSession = await connectBackend(sourceConnectionId);
    targetSession = await connectBackend(targetConnectionId);

    await withSafeModeOff(async () => {
      const drop = `DROP TABLE IF EXISTS ${child}, ${parent}, ${cycleA}, ${cycleB} CASCADE`;
      await runSql(sourceSession!, drop);
      await runSql(targetSession!, drop);
      await runSql(
        sourceSession!,
        `CREATE TABLE ${parent} (id integer PRIMARY KEY, label text NOT NULL);
         CREATE TABLE ${child} (id integer PRIMARY KEY, parent_id integer NOT NULL REFERENCES ${parent}(id), label text NOT NULL);
         INSERT INTO ${parent} VALUES (1, 'source parent');
         INSERT INTO ${child} VALUES (11, 1, 'source child');`,
      );
      await runSql(
        targetSession!,
        `CREATE TABLE ${parent} (id integer PRIMARY KEY, label text NOT NULL);
         CREATE TABLE ${child} (id integer PRIMARY KEY, parent_id integer NOT NULL REFERENCES ${parent}(id), label text NOT NULL);
         INSERT INTO ${parent} VALUES (1, 'existing parent');`,
      );
      await runSql(
        sourceSession!,
        `CREATE TABLE ${cycleA} (id integer PRIMARY KEY, b_id integer NOT NULL);
         CREATE TABLE ${cycleB} (id integer PRIMARY KEY, a_id integer NOT NULL);
         INSERT INTO ${cycleA} VALUES (1, 1);
         INSERT INTO ${cycleB} VALUES (1, 1);`,
      );
      await runSql(
        targetSession!,
        `CREATE TABLE ${cycleA} (id integer PRIMARY KEY, b_id integer NOT NULL);
         CREATE TABLE ${cycleB} (id integer PRIMARY KEY, a_id integer NOT NULL);
         ALTER TABLE ${cycleA} ADD CONSTRAINT fk_${stamp}_a_b FOREIGN KEY (b_id) REFERENCES ${cycleB}(id) DEFERRABLE INITIALLY DEFERRED;
         ALTER TABLE ${cycleB} ADD CONSTRAINT fk_${stamp}_b_a FOREIGN KEY (a_id) REFERENCES ${cycleA}(id) DEFERRABLE INITIALLY DEFERRED;`,
      );
    });
  });

  after(async () => {
    const cleanupErrors: string[] = [];
    const cleanupSessions: string[] = [];
    for (const endpoint of [
      { label: 'source', connectionId: sourceConnectionId },
      { label: 'target', connectionId: targetConnectionId },
    ]) {
      try {
        const session = await connectBackend(endpoint.connectionId);
        cleanupSessions.push(session);
        await withSafeModeOff(async () => {
          await runSql(
            session,
            `DROP TABLE IF EXISTS ${child}, ${parent}, ${cycleA}, ${cycleB} CASCADE`,
          );
        });
        const remaining = await remainingFixtureCount(session);
        if (remaining !== 0) {
          throw new Error(`${remaining} uniquely named fixture table(s) remain`);
        }
      } catch (error) {
        cleanupErrors.push(
          `${endpoint.label}: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    }

    for (const [label, session] of [
      ...cleanupSessions.map(
        (session, index) => [`cleanup session ${index + 1}`, session] as const,
      ),
      ...(sourceSession ? [['original source session', sourceSession] as const] : []),
      ...(targetSession ? [['original target session', targetSession] as const] : []),
    ]) {
      try {
        await disconnectBackend(session);
      } catch (error) {
        cleanupErrors.push(
          `disconnect ${label}: ${error instanceof Error ? error.message : String(error)}`,
        );
      }
    }
    for (const connectionId of [sourceConnectionId, targetConnectionId]) {
      try {
        await invokeBackend('delete_connection', { id: connectionId });
      } catch {
        /* connection may already have been removed */
      }
    }
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    if (cleanupErrors.length > 0) {
      throw new Error(`fixture cleanup failed:\n${cleanupErrors.join('\n')}`);
    }
  });

  it('orders parent rows first and preserves child-only selection', async () => {
    const childOnlyPlan = await preview([child, parent]);
    expect(childOnlyPlan.writePlans.map((item) => item.sourceTable)).toEqual([parent, child]);

    const childOnly = await execute(childOnlyPlan.planId, [child]);
    expect(childOnly.partial).toBe(false);
    expect(childOnly.rowsInserted).toBe(1);
    expect(await rowCount(targetSession!, parent)).toBe(1);
    expect(await rowCount(targetSession!, child)).toBe(1);

    await withSafeModeOff(async () => {
      await runSql(targetSession!, `DELETE FROM ${child}; DELETE FROM ${parent}`);
    });
    const fullPlan = await preview([child, parent]);
    const fullResult = await execute(fullPlan.planId);
    expect(fullResult.partial).toBe(false);
    expect(fullResult.rowsInserted).toBe(2);
    expect(await rowCount(targetSession!, parent)).toBe(1);
    expect(await rowCount(targetSession!, child)).toBe(1);

    const readback = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: targetSession!,
      sql: `SELECT p.id AS parent_id, c.id AS child_id, c.parent_id AS child_parent_id FROM ${parent} p JOIN ${child} c ON c.parent_id = p.id`,
    });
    expect(readback.results[0]?.rows).toEqual([[1, 11, 1]]);
  });

  it('rejects a selected FK cycle before any target row is written', async () => {
    const plan = await preview([cycleA, cycleB]);
    const error = await expectCommandError(() => execute(plan.planId));
    expect(error).toContain('foreign-key dependency cycle');
    expect(await rowCount(targetSession!, cycleA)).toBe(0);
    expect(await rowCount(targetSession!, cycleB)).toBe(0);
  });
});
