/**
 * SQL Server → SQL Server Schema Diff live journey through the real UI.
 *
 * The source and target must be separate databases on an instance reachable
 * with the E2E_SQLSERVER_* credentials. Both databases receive the same unique
 * scratch schema so the SQL Server same-schema safety boundary is exercised.
 * The source gets one table that is absent from the target; the journey then
 * compares, generates a plan, deploys it, and checks the target catalog.
 *
 * Skips when credentials or distinct source/target database names are absent.
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  advanceSchemaDiffToReview,
  assertSchemaDiffDeploySuccess,
  assertSchemaDiffNoErrors,
  clickSchemaDiffCompare,
  clickSchemaDiffGeneratePlan,
  closeExtraWindows,
  deploySchemaDiffPlan,
  disconnectBackend,
  invokeBackend,
  openSchemaDiffWindow,
  parseQueryRows,
  queryScalar,
  selectSchemaDiffEndpoints,
  setSchemaDiffTables,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../../../e2e/helpers.js';

const HOST = (process.env.E2E_SQLSERVER_HOST || '').trim();
const PORT = Number(process.env.E2E_SQLSERVER_PORT || '1433');
const USER = (process.env.E2E_SQLSERVER_USER || '').trim();
const PASSWORD = process.env.E2E_SQLSERVER_PASSWORD || '';
const SOURCE_DATABASE = (
  process.env.E2E_SQLSERVER_SOURCE_DATABASE ||
  process.env.E2E_SQLSERVER_DATABASE ||
  ''
).trim();
const TARGET_DATABASE = (process.env.E2E_SQLSERVER_TARGET_DATABASE || '').trim();
const SSL_MODE = (process.env.E2E_SQLSERVER_SSL_MODE || 'require').trim();
const TRUST_CERT = process.env.E2E_SQLSERVER_TRUST_CERT !== '0';

const STAMP = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
const SOURCE_ID = `e2e-sqlserver-sd-src-${STAMP}`;
const TARGET_ID = `e2e-sqlserver-sd-tgt-${STAMP}`;
const SOURCE_NAME = `E2E SQL Server SD Source ${STAMP}`;
const TARGET_NAME = `E2E SQL Server SD Target ${STAMP}`;
const SCRATCH_SCHEMA = `dz_e2e_sd_${STAMP}`;
const TABLE = `schema_diff_${STAMP}`;

let mainWindow = '';
let sourceSaved = false;
let targetSaved = false;

function skipReason(): string | null {
  if (process.env.E2E_SKIP_SQLSERVER === '1') return 'E2E_SKIP_SQLSERVER=1';
  if (!HOST || !USER || !PASSWORD) {
    return 'set E2E_SQLSERVER_HOST / E2E_SQLSERVER_USER / E2E_SQLSERVER_PASSWORD';
  }
  if (!SOURCE_DATABASE || !TARGET_DATABASE) {
    return 'set distinct E2E_SQLSERVER_SOURCE_DATABASE and E2E_SQLSERVER_TARGET_DATABASE values';
  }
  if (SOURCE_DATABASE.toLowerCase() === TARGET_DATABASE.toLowerCase()) {
    return 'source and target databases must be different';
  }
  return null;
}

async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (name: string, serializedArgs: string, done: (value: unknown) => void) => {
      const internals = (
        window as unknown as {
          __TAURI_INTERNALS__?: {
            invoke: (commandName: string, input: unknown) => Promise<unknown>;
          };
        }
      ).__TAURI_INTERNALS__;
      if (!internals) {
        done({ __error: '__TAURI_INTERNALS__ is unavailable (not a webdriver build?)' });
        return;
      }
      internals
        .invoke(name, JSON.parse(serializedArgs))
        .then((value) => done(value))
        .catch((error: unknown) => done({ __error: String(error) }));
    },
    command,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as Record<string, unknown>)) {
    throw new Error(String((result as { __error: string }).__error));
  }
  return result as T;
}

function bracket(identifier: string): string {
  return `[${identifier.replaceAll(']', ']]')}]`;
}

function connectionConfig(id: string, name: string, database: string) {
  return {
    id,
    name,
    databaseType: 'sqlserver',
    host: HOST,
    port: PORT,
    database,
    schema: SCRATCH_SCHEMA,
    username: USER,
    password: PASSWORD,
    sslMode: SSL_MODE,
    connectionTimeout: 30,
    maxPoolSize: 5,
    options: { trustServerCertificate: TRUST_CERT },
  };
}

async function withConnection<T>(connectionId: string, work: (sessionId: string) => Promise<T>) {
  const sessionId = await invoke<string>('connect', { connectionId });
  try {
    return await work(sessionId);
  } finally {
    await disconnectBackend(sessionId);
  }
}

async function run(sessionId: string, sql: string): Promise<QueryResultPayload> {
  return invoke<QueryResultPayload>('execute_query', { dbSessionId: sessionId, sql });
}

async function createScratchObjects(): Promise<void> {
  const sourceSession = await invoke<string>('connect', { connectionId: SOURCE_ID });
  const targetSession = await invoke<string>('connect', { connectionId: TARGET_ID });
  try {
    await withSafeModeOff(async () => {
      for (const session of [sourceSession, targetSession]) {
        await run(session, `CREATE SCHEMA ${bracket(SCRATCH_SCHEMA)}`);
      }
      await run(
        sourceSession,
        `CREATE TABLE ${bracket(SCRATCH_SCHEMA)}.${bracket(TABLE)} (` +
          '[id] INT NOT NULL PRIMARY KEY, [label] NVARCHAR(80) NOT NULL)',
      );
    });
  } finally {
    await disconnectBackend(sourceSession);
    await disconnectBackend(targetSession);
  }
}

async function cleanupScratchObjects(): Promise<void> {
  const savedConnections = [
    { id: SOURCE_ID, saved: sourceSaved },
    { id: TARGET_ID, saved: targetSaved },
  ].filter((entry) => entry.saved);

  if (savedConnections.length > 0) {
    await withSafeModeOff(async () => {
      for (const { id } of savedConnections) {
        await withConnection(id, async (session) => {
          await run(
            session,
            `DROP TABLE IF EXISTS ${bracket(SCRATCH_SCHEMA)}.${bracket(TABLE)}`,
          ).catch((error: unknown) => {
            console.warn(
              `SQL Server Schema Diff cleanup could not drop scratch table: ${String(error)}`,
            );
          });
          await run(session, `DROP SCHEMA IF EXISTS ${bracket(SCRATCH_SCHEMA)}`).catch(
            (error: unknown) => {
              console.warn(
                `SQL Server Schema Diff cleanup could not drop scratch schema: ${String(error)}`,
              );
            },
          );
        }).catch((error: unknown) => {
          console.warn(`SQL Server Schema Diff cleanup could not connect: ${String(error)}`);
        });
      }
    }).catch((error: unknown) => {
      console.warn(`SQL Server Schema Diff cleanup could not disable Safe Mode: ${String(error)}`);
    });
  }

  for (const { id, saved } of [
    { id: SOURCE_ID, saved: sourceSaved },
    { id: TARGET_ID, saved: targetSaved },
  ]) {
    if (saved) await invoke('delete_connection', { id }).catch(() => undefined);
  }
}

describe('SQL Server Schema Diff live journey', () => {
  let reason: string | null = null;

  before(async function () {
    this.timeout(120_000);
    reason = skipReason();
    if (reason) {
      console.warn(`⏩ Skipping SQL Server Schema Diff E2E: ${reason}`);
      this.skip();
    }

    mainWindow = await browser.getWindowHandle();
    const handles = await browser.getWindowHandles();
    if (handles[0]) await browser.switchToWindow(handles[0]);
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });

    await invoke('save_connection', {
      config: connectionConfig(SOURCE_ID, SOURCE_NAME, SOURCE_DATABASE),
    });
    sourceSaved = true;
    await invoke('save_connection', {
      config: connectionConfig(TARGET_ID, TARGET_NAME, TARGET_DATABASE),
    });
    targetSaved = true;

    await createScratchObjects();
  });

  after(async function () {
    this.timeout(60_000);
    if (mainWindow) {
      await closeExtraWindows(mainWindow);
      await browser.switchToWindow(mainWindow);
    }
    await cleanupScratchObjects();
  });

  it('compares SQL Server schemas, deploys the generated plan, and verifies the target table', async function () {
    this.timeout(120_000);
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints(SOURCE_NAME, TARGET_NAME);
    await setSchemaDiffTables(TABLE);
    await clickSchemaDiffCompare();

    const compareText = await $('[data-testid="schema-diff-detail-panel"]').getText();
    expect(compareText).toContain(TABLE);
    await assertSchemaDiffNoErrors();

    await clickSchemaDiffGeneratePlan();
    const planText = await $('[data-testid="schema-diff-plan-panel"]').getText();
    expect(planText).toContain(TABLE);
    expect(planText).toMatch(/CREATE\s+TABLE/i);
    await assertSchemaDiffNoErrors();

    await advanceSchemaDiffToReview();
    await deploySchemaDiffPlan();
    await assertSchemaDiffDeploySuccess();

    await withConnection(TARGET_ID, async (session) => {
      const result = await run(
        session,
        'SELECT [COLUMN_NAME] FROM [INFORMATION_SCHEMA].[COLUMNS] ' +
          `WHERE [TABLE_SCHEMA] = N'${SCRATCH_SCHEMA}' AND [TABLE_NAME] = N'${TABLE}' ` +
          'ORDER BY [ORDINAL_POSITION]',
      );
      const columns = parseQueryRows(result).map((row) => String(row[0]));
      expect(columns).toEqual(['id', 'label']);

      const primaryKeyCount = await run(
        session,
        'SELECT COUNT(*) AS [c] FROM [INFORMATION_SCHEMA].[TABLE_CONSTRAINTS] ' +
          `WHERE [TABLE_SCHEMA] = N'${SCRATCH_SCHEMA}' AND [TABLE_NAME] = N'${TABLE}' ` +
          "AND [CONSTRAINT_TYPE] = N'PRIMARY KEY'",
      );
      expect(queryScalar(primaryKeyCount, 'c')).toBe(1);
    });
  });
});
