import { $, browser, expect } from '@wdio/globals';
import {
  closeExtraWindows,
  connectConfig,
  executeQuery,
  invokeBackend,
  switchToNewWindow,
} from '../helpers.js';

type DatabaseType = 'postgresql' | 'mysql';
type Fault = 'lost_after_commit' | 'lost_before_commit';

interface QueryResult {
  results?: Array<{ rows?: unknown[][] }>;
}

interface ComparePreview {
  planId: string;
  selectionRevision: number;
  pageSize: number;
  tables: Array<{
    sourceTable: string;
    targetTable: string;
    firstCursor?: string | null;
    pageSize?: number;
  }>;
}

interface ComparePage {
  rows: Array<{ operation: string; key: unknown[] }>;
  nextCursor: string | null;
}

interface ExecutionResponse {
  outcome: 'not_started' | 'committed' | 'rolled_back' | 'unknown';
  applied: number;
  rolledBack: boolean;
  error?: string;
}

interface MigrationRun {
  id: string;
  outcome: string;
  profileId: string | null;
}

function config(databaseType: DatabaseType, id: string, database: string) {
  const prefix = databaseType === 'postgresql' ? 'E2E_PG' : 'E2E_MYSQL';
  return {
    id,
    name: id,
    databaseType,
    host: process.env[`${prefix}_HOST`] || '127.0.0.1',
    port: Number(process.env[`${prefix}_PORT`]) || (databaseType === 'mysql' ? 3306 : 5432),
    database,
    username: process.env[`${prefix}_USER`] || (databaseType === 'mysql' ? 'root' : 'postgres'),
    password: process.env[`${prefix}_PASSWORD`] || '',
    sslMode: 'disable',
  };
}

function databaseNames(databaseType: DatabaseType) {
  return databaseType === 'postgresql'
    ? { source: 'datazen_sync_src', target: 'datazen_sync_tgt' }
    : { source: 'datazen_sync_mysql_src', target: 'datazen_sync_mysql_tgt' };
}

async function valueAt(session: string, database: string, table: string): Promise<unknown> {
  const response = await executeQuery(session, `SELECT value FROM ${table} WHERE id = 1`, database);
  return response.results?.[0]?.rows?.[0]?.[0];
}

/** Observe a non-idempotent IPC exactly once; never retry an execution request. */
async function invokeBackendOnce<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const raw = await browser.executeAsync(
    (command: string, argsJson: string, done: (value: string) => void) => {
      const internals = (
        window as unknown as {
          __TAURI_INTERNALS__?: {
            invoke: (command: string, args: unknown) => Promise<unknown>;
          };
        }
      ).__TAURI_INTERNALS__;
      if (!internals) {
        done(JSON.stringify({ ok: false, error: 'Tauri IPC is unavailable in this window' }));
        return;
      }
      let payload: unknown;
      try {
        payload = JSON.parse(argsJson);
      } catch (error) {
        done(JSON.stringify({ ok: false, error: String(error) }));
        return;
      }
      void internals.invoke(command, payload).then(
        (value) => done(JSON.stringify({ ok: true, value })),
        (error: unknown) => done(JSON.stringify({ ok: false, error: String(error) })),
      );
    },
    cmd,
    JSON.stringify(args),
  );
  if (typeof raw !== 'string') {
    throw new Error(`one-shot IPC returned a non-string envelope for ${cmd}`);
  }
  const envelope = JSON.parse(raw) as { ok: true; value: T } | { ok: false; error: string };
  if (!envelope.ok) throw new Error(`one-shot IPC rejected ${cmd}: ${envelope.error}`);
  return envelope.value;
}

async function openDataSyncWindow(mainHandle: string) {
  await browser.url('tauri://localhost/window.html?window=data-sync');
  await browser.pause(800);
  const handles = await browser.getWindowHandles();
  if (handles.length > 1) await switchToNewWindow(mainHandle);
  await $('[data-testid="data-sync-window"]').waitForDisplayed({ timeout: 20000 });
}

async function setUpAndExecuteUnknown(databaseType: DatabaseType, fault: Fault) {
  const suffix = `${databaseType}_${fault}_${Date.now().toString(36)}`;
  const names = databaseNames(databaseType);
  const table = `dz_sync_uncertain_${suffix}`;
  const sourceId = `dz_sync_unknown_src_${suffix}`;
  const targetId = `dz_sync_unknown_tgt_${suffix}`;
  const profileId = `dz_sync_unknown_profile_${suffix}`;
  let sourceSession = '';
  let targetSession = '';
  let profileSaved = false;
  let originalSettings: Record<string, unknown> | undefined;

  try {
    await invokeBackend('save_connection', {
      config: config(databaseType, sourceId, names.source),
    });
    await invokeBackend('save_connection', {
      config: config(databaseType, targetId, names.target),
    });
    sourceSession = await connectConfig(sourceId);
    targetSession = await connectConfig(targetId);
    originalSettings = await invokeBackend<Record<string, unknown>>('get_settings');
    await invokeBackend('save_settings', { settings: { ...originalSettings, safeMode: false } });

    await executeQuery(sourceSession, `DROP TABLE IF EXISTS ${table}`, names.source);
    await executeQuery(targetSession, `DROP TABLE IF EXISTS ${table}`, names.target);
    await executeQuery(
      sourceSession,
      `CREATE TABLE ${table} (id INT PRIMARY KEY, value INT NOT NULL)`,
      names.source,
    );
    await executeQuery(
      targetSession,
      `CREATE TABLE ${table} (id INT PRIMARY KEY, value INT NOT NULL)`,
      names.target,
    );
    await executeQuery(sourceSession, `INSERT INTO ${table} VALUES (1, 20)`, names.source);
    await executeQuery(targetSession, `INSERT INTO ${table} VALUES (1, 10)`, names.target);

    const timestamp = new Date().toISOString();
    await invokeBackend('save_sync_profile', {
      profile: {
        version: 1,
        id: profileId,
        name: profileId,
        sourceConnectionId: sourceId,
        targetConnectionId: targetId,
        sourceDatabase: names.source,
        targetDatabase: names.target,
        sourceSchema: null,
        targetSchema: null,
        tables: [{ sourceTable: table, targetTable: table, enabled: true }],
        options: {
          insert: true,
          update: true,
          delete: false,
          matchingStrategy: 'primaryKey',
          batchSize: 100,
          largeValueMode: 'full',
          conflictPolicy: 'abort',
        },
        createdAt: timestamp,
        updatedAt: timestamp,
      },
    });
    profileSaved = true;
    const profiles =
      await invokeBackend<Array<{ id: string; updatedAt: string }>>('get_sync_profiles');
    const profile = profiles.find((candidate) => candidate.id === profileId);
    if (!profile) throw new Error('test profile did not persist');

    const options = {
      insert: true,
      update: true,
      delete: false,
      matchingStrategy: 'primaryKey',
      batchSize: 100,
      largeValueMode: 'full',
      conflictPolicy: 'abort',
    };
    const preview = await invokeBackend<ComparePreview>('compare_data_sync', {
      sourceDbSessionId: sourceSession,
      targetDbSessionId: targetSession,
      sourceDatabase: names.source,
      targetDatabase: names.target,
      tables: [table],
      options,
    });
    const comparedTable = preview.tables.find((candidate) => candidate.sourceTable === table);
    if (!comparedTable) throw new Error('fixture did not produce a comparison table');
    const firstPage = await invokeBackend<ComparePage>('get_data_sync_comparison_page', {
      request: {
        planId: preview.planId,
        sourceTable: comparedTable.sourceTable,
        targetTable: comparedTable.targetTable,
        cursor: comparedTable.firstCursor ?? null,
        limit: comparedTable.pageSize ?? preview.pageSize,
      },
    });
    const row = firstPage.rows.find((candidate) => candidate.operation === 'UPDATE');
    if (!row) throw new Error('fixture did not produce an update row');

    await invokeBackend('set_data_sync_test_commit_fault', { fault });
    const selection = {
      revision: preview.selectionRevision,
      rows: [
        {
          sourceTable: comparedTable.sourceTable,
          targetTable: comparedTable.targetTable,
          operation: row.operation,
          key: row.key,
        },
      ],
      scopes: [],
    };
    const profileRef = { id: profileId, revision: profile.updatedAt };
    const response = await invokeBackendOnce<ExecutionResponse>('execute_data_sync', {
      request: {
        planId: preview.planId,
        selection,
        options,
        jobId: null,
      },
      profile: profileRef,
    });
    if (response.outcome !== 'unknown') {
      throw new Error(`fault injection expected unknown; got ${response.outcome}`);
    }

    const history = await invokeBackend<{ items: MigrationRun[] }>('list_migration_runs', {
      filter: { operation: 'dataSync', profileId },
      offset: 0,
      limit: 10,
    });
    const run = history.items.find(
      (item) => item.outcome === 'unknown' && item.profileId === profileId,
    );
    if (!run) throw new Error('unknown Data Sync run was not persisted in history');

    return {
      databaseType,
      fault,
      names,
      table,
      sourceId,
      targetId,
      profileId,
      runId: run.id,
      planId: preview.planId,
      selection,
      options,
      profileRef,
      originalSettings,
      sourceSession,
      targetSession,
      profileSaved,
    };
  } catch (error) {
    await invokeBackend('set_data_sync_test_commit_fault', { fault: 'none' }).catch(
      () => undefined,
    );
    if (originalSettings) {
      await invokeBackend('save_settings', { settings: originalSettings }).catch(() => undefined);
    }
    if (profileSaved)
      await invokeBackend('delete_sync_profile', { profileId }).catch(() => undefined);
    if (sourceSession) {
      await executeQuery(sourceSession, `DROP TABLE IF EXISTS ${table}`, names.source).catch(
        () => undefined,
      );
      await invokeBackend('disconnect', { dbSessionId: sourceSession }).catch(() => undefined);
    }
    if (targetSession) {
      await executeQuery(targetSession, `DROP TABLE IF EXISTS ${table}`, names.target).catch(
        () => undefined,
      );
      await invokeBackend('disconnect', { dbSessionId: targetSession }).catch(() => undefined);
    }
    await invokeBackend('delete_connection', { id: sourceId }).catch(() => undefined);
    await invokeBackend('delete_connection', { id: targetId }).catch(() => undefined);
    throw error;
  }
}

describe('Data Sync unknown outcome history recovery', () => {
  for (const databaseType of ['postgresql', 'mysql'] as const) {
    for (const fault of ['lost_after_commit', 'lost_before_commit'] as const) {
      it(`[tester] ${databaseType} ${fault} requires a fresh review and never replays the old run`, async function () {
        this.timeout(180_000);
        const fixture = await setUpAndExecuteUnknown(databaseType, fault);
        const expectedValue = fault === 'lost_after_commit' ? 20 : 10;
        const mainHandle = await browser.getWindowHandle();
        try {
          expect(await valueAt(fixture.targetSession, fixture.names.target, fixture.table)).toBe(
            expectedValue,
          );

          const replay = await invokeBackendOnce<ExecutionResponse>('execute_data_sync', {
            request: {
              planId: fixture.planId,
              selection: fixture.selection,
              options: fixture.options,
              jobId: null,
            },
            profile: fixture.profileRef,
          });
          // An idempotent receipt repeats the original unknown outcome without
          // attempting another write. It must never turn uncertainty into a refusal.
          expect(replay.outcome).toBe('unknown');
          expect(await valueAt(fixture.targetSession, fixture.names.target, fixture.table)).toBe(
            expectedValue,
          );
          const historyAfterReplay = await invokeBackend<{ items: MigrationRun[] }>(
            'list_migration_runs',
            {
              filter: { operation: 'dataSync', profileId: fixture.profileId },
              offset: 0,
              limit: 10,
            },
          );
          expect(
            historyAfterReplay.items.filter(
              (item) => item.outcome === 'unknown' && item.profileId === fixture.profileId,
            ),
          ).toHaveLength(1);

          await openDataSyncWindow(mainHandle);
          await $('[data-testid="migration-history-open"]').click();
          const row = await $(`[data-testid="migration-run-${fixture.runId}"]`);
          await row.waitForDisplayed({ timeout: 10000 });
          await row.click();
          expect(
            (await $('[data-testid="migration-run-detail"]').getText()).toLowerCase(),
          ).toContain('unknown');
          await $('[data-testid="migration-run-reconcile"]').click();

          await browser.waitUntil(
            async () =>
              (await $('[data-testid="data-sync-window"]').getAttribute(
                'data-write-outcome-uncertain',
              )) === 'true',
            { timeout: 10000, timeoutMsg: 'history recovery did not keep the unknown write fence' },
          );
          expect(await $('[data-testid="data-sync-start"]').isExisting()).toBe(false);
          expect(await $('[data-testid="data-sync-execute"]').isExisting()).toBe(false);

          await browser.waitUntil(
            async () => await $('[data-testid="data-sync-step-endpoints"]').isDisplayed(),
            { timeout: 10000, timeoutMsg: 'history action did not return to Data Sync endpoints' },
          );
          await $('[data-testid="data-sync-next"]').click();
          await browser.waitUntil(
            async () =>
              (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-step')) ===
              'setup',
            { timeout: 10000, timeoutMsg: 'reconciliation did not reach setup step' },
          );
          await $('[data-testid="data-sync-next"]').click();
          await browser.waitUntil(
            async () =>
              (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-step')) ===
              'objects',
            { timeout: 30000, timeoutMsg: 'fresh profile inspection did not finish' },
          );
          await browser.waitUntil(
            async () =>
              (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-state')) !==
                'inspecting' && (await $('[data-testid="data-sync-next"]').isEnabled()),
            {
              timeout: 60000,
              interval: 250,
              timeoutMsg: 'object inspection did not finish or Next stayed disabled',
            },
          );
          await $('[data-testid="data-sync-next"]').click();
          await $('[data-testid="data-sync-summary"]').waitForDisplayed({ timeout: 120000 });
          await browser.waitUntil(
            async () =>
              (await $('[data-testid="data-sync-window"]').getAttribute('data-sync-state')) ===
              'compared',
            { timeout: 120000, timeoutMsg: 'fresh comparison did not finish' },
          );
          expect(
            await $('[data-testid="data-sync-window"]').getAttribute(
              'data-write-outcome-uncertain',
            ),
          ).toBe('false');
          const summary = await $('[data-testid="data-sync-summary"]').getText();
          expect(summary).toContain(fault === 'lost_after_commit' ? '~0' : '~1');
          expect(await valueAt(fixture.targetSession, fixture.names.target, fixture.table)).toBe(
            expectedValue,
          );
        } finally {
          await closeExtraWindows(mainHandle).catch(() => undefined);
          await browser.switchToWindow(mainHandle).catch(() => undefined);
          await invokeBackend('set_data_sync_test_commit_fault', { fault: 'none' }).catch(
            () => undefined,
          );
          if (fixture.originalSettings) {
            await invokeBackend('save_settings', { settings: fixture.originalSettings }).catch(
              () => undefined,
            );
          }
          if (fixture.profileSaved) {
            await invokeBackend('delete_sync_profile', { profileId: fixture.profileId }).catch(
              () => undefined,
            );
          }
          await executeQuery(
            fixture.sourceSession,
            `DROP TABLE IF EXISTS ${fixture.table}`,
            fixture.names.source,
          ).catch(() => undefined);
          await executeQuery(
            fixture.targetSession,
            `DROP TABLE IF EXISTS ${fixture.table}`,
            fixture.names.target,
          ).catch(() => undefined);
          await invokeBackend('disconnect', { dbSessionId: fixture.sourceSession }).catch(
            () => undefined,
          );
          await invokeBackend('disconnect', { dbSessionId: fixture.targetSession }).catch(
            () => undefined,
          );
          await invokeBackend('delete_connection', { id: fixture.sourceId }).catch(() => undefined);
          await invokeBackend('delete_connection', { id: fixture.targetId }).catch(() => undefined);
        }
      });
    }
  }
});
