/**
 * SQL Server Data Sync journey through the host's public IPC surface.
 *
 * The test compares a source IDENTITY table with an empty target IDENTITY
 * table, selects the INSERT operation from the immutable compare plan,
 * inspects the client-safe SQL preview, applies the plan and verifies that
 * explicit identity values were preserved. It skips without SQL Server E2E
 * credentials. All scratch objects use a per-run `dz_e2e_*` schema.
 */
import { expect, browser } from '@wdio/globals';

const HOST = (process.env.E2E_SQLSERVER_HOST || '').trim();
const PORT = Number(process.env.E2E_SQLSERVER_PORT || '1433');
const USER = (process.env.E2E_SQLSERVER_USER || '').trim();
const PASSWORD = process.env.E2E_SQLSERVER_PASSWORD || '';
const DATABASE = (process.env.E2E_SQLSERVER_DATABASE || '').trim();
const SSL_MODE = (process.env.E2E_SQLSERVER_SSL_MODE || 'require').trim();
const TRUST_CERT = process.env.E2E_SQLSERVER_TRUST_CERT !== '0';

const STAMP = Date.now().toString(36);
const SOURCE_SCHEMA = `dz_e2e_sync_src_${STAMP}`;
const TARGET_SCHEMA = `dz_e2e_sync_tgt_${STAMP}`;
const TABLE = `dz_e2e_identity_${STAMP}`;
const SOURCE_CONNECTION_ID = `e2e-sqlserver-ds-src-${STAMP}`;
const TARGET_CONNECTION_ID = `e2e-sqlserver-ds-tgt-${STAMP}`;
const IDS = [101, 202, 303];

type Cell = string | number | boolean | null;

interface QueryResult {
  rows?: Cell[][];
}

interface MultiQueryPayload {
  results?: QueryResult[];
}

interface SyncComparisonTable {
  sourceTable: string;
  targetTable: string;
  status: string;
  insertCount: number;
  updateCount: number;
  deleteCount: number;
}

interface SyncComparison {
  planId: string;
  selectionRevision: number;
  tables: SyncComparisonTable[];
}

interface SyncSqlStatement {
  table: string;
  operation: string;
  sql: string;
  previewSql: string;
  parameters: Cell[];
  rowKey: Cell[];
  identityInsert?: unknown;
  identity_insert?: unknown;
}

interface SyncExecutionResult {
  applied: number;
  affectedRows: number;
  outcome: string;
  error?: string;
  rolledBack: boolean;
}

function skipReason(): string | null {
  if (process.env.E2E_SKIP_SQLSERVER === '1') return 'E2E_SKIP_SQLSERVER=1';
  if (!HOST || !USER || !PASSWORD) {
    return 'set E2E_SQLSERVER_HOST / E2E_SQLSERVER_USER / E2E_SQLSERVER_PASSWORD';
  }
  return null;
}

function bracket(identifier: string): string {
  return `[${identifier.replaceAll(']', ']]')}]`;
}

function qualified(schema: string): string {
  return `${bracket(schema)}.${bracket(TABLE)}`;
}

/** Invoke one Tauri command without putting connection secrets in diagnostics. */
async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (name: string, serializedArgs: string, done: (value: unknown) => void) => {
      const internals = (
        window as unknown as {
          __TAURI_INTERNALS__?: { invoke: (cmd: string, input: unknown) => Promise<unknown> };
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
    const detail = String((result as { __error: string }).__error).replaceAll(
      PASSWORD,
      '[redacted]',
    );
    throw new Error(`${command} failed: ${detail}`);
  }
  return result as T;
}

describe('SQL Server Data Sync identity insert (live)', () => {
  let sourceSessionId = '';
  let targetSessionId = '';
  let originalSafeMode: boolean | null = null;

  const run = (sessionId: string, sql: string) =>
    invoke<MultiQueryPayload>('execute_query', {
      dbSessionId: sessionId,
      sql,
      ...(DATABASE ? { database: DATABASE } : {}),
    });

  const setSafeMode = async (enabled: boolean) => {
    const settings = await invoke<Record<string, unknown>>('get_settings');
    if (originalSafeMode === null) originalSafeMode = settings.safeMode !== false;
    if ((settings.safeMode !== false) === enabled) return;
    await invoke('save_settings', { settings: { ...settings, safeMode: enabled } });
  };

  const saveAndConnect = async (id: string, name: string, schema: string): Promise<string> => {
    await invoke('save_connection', {
      config: {
        id,
        name,
        databaseType: 'sqlserver',
        host: HOST,
        port: PORT,
        ...(DATABASE ? { database: DATABASE } : {}),
        schema,
        username: USER,
        password: PASSWORD,
        sslMode: SSL_MODE,
        connectionTimeout: 30,
        maxPoolSize: 5,
        options: { trustServerCertificate: TRUST_CERT },
      },
    });

    let lastError: unknown;
    for (let attempt = 1; attempt <= 4; attempt += 1) {
      try {
        return await invoke<string>('connect', { connectionId: id });
      } catch (error) {
        lastError = error;
        if (!String(error).toLowerCase().includes('not currently available') || attempt === 4) {
          throw error;
        }
        console.warn(`⏳ SQL Server database resuming (attempt ${attempt}/4), retrying in 8s`);
        await browser.pause(8000);
      }
    }
    throw lastError ?? new Error('connect returned no dbSessionId');
  };

  const dropSchema = async (sessionId: string, schema: string) => {
    try {
      await run(sessionId, `DROP TABLE IF EXISTS ${qualified(schema)}`);
    } catch (error) {
      console.warn(`SQL Server Data Sync cleanup could not drop scratch table: ${String(error)}`);
    }
    try {
      await run(sessionId, `DROP SCHEMA IF EXISTS ${bracket(schema)}`);
    } catch (error) {
      console.warn(`SQL Server Data Sync cleanup could not drop scratch schema: ${String(error)}`);
    }
  };

  const restoreSafeMode = async () => {
    if (originalSafeMode === null) return;
    const settings = await invoke<Record<string, unknown>>('get_settings').catch(() => null);
    if (!settings || (settings.safeMode !== false) === originalSafeMode) return;
    await invoke('save_settings', {
      settings: { ...settings, safeMode: originalSafeMode },
    }).catch(() => undefined);
  };

  before(async function () {
    this.timeout(120_000);
    const reason = skipReason();
    if (reason) {
      console.warn(`⏩ Skipping SQL Server Data Sync live E2E: ${reason}`);
      this.skip();
    }

    const handles = await browser.getWindowHandles();
    if (handles[0]) await browser.switchToWindow(handles[0]);
    await browser.url('tauri://localhost/');

    sourceSessionId = await saveAndConnect(
      SOURCE_CONNECTION_ID,
      'E2E SQL Server Data Sync source',
      SOURCE_SCHEMA,
    );
    targetSessionId = await saveAndConnect(
      TARGET_CONNECTION_ID,
      'E2E SQL Server Data Sync target',
      TARGET_SCHEMA,
    );

    // Safe Mode blocks scratch DDL. The exact pre-test preference is restored
    // in `after`, including when setup or an assertion fails.
    await setSafeMode(false);
    await run(sourceSessionId, `CREATE SCHEMA ${bracket(SOURCE_SCHEMA)}`);
    await run(targetSessionId, `CREATE SCHEMA ${bracket(TARGET_SCHEMA)}`);

    const sourceTable = qualified(SOURCE_SCHEMA);
    const targetTable = qualified(TARGET_SCHEMA);
    const ddl =
      `CREATE TABLE ${sourceTable} (` +
      '[id] INT IDENTITY(1,1) NOT NULL PRIMARY KEY, ' +
      '[label] NVARCHAR(64) NOT NULL)';
    await run(sourceSessionId, ddl);
    await run(
      targetSessionId,
      `CREATE TABLE ${targetTable} (` +
        '[id] INT IDENTITY(1,1) NOT NULL PRIMARY KEY, ' +
        '[label] NVARCHAR(64) NOT NULL)',
    );

    await run(sourceSessionId, `SET IDENTITY_INSERT ${sourceTable} ON`);
    try {
      await run(
        sourceSessionId,
        `INSERT INTO ${sourceTable} ([id], [label]) VALUES ` +
          IDS.map((id) => `(${id}, N'identity-${id}')`).join(', '),
      );
    } finally {
      await run(sourceSessionId, `SET IDENTITY_INSERT ${sourceTable} OFF`);
    }
  });

  after(async function () {
    this.timeout(60_000);
    try {
      if (sourceSessionId || targetSessionId) await setSafeMode(false);
      if (sourceSessionId) await dropSchema(sourceSessionId, SOURCE_SCHEMA);
      if (targetSessionId) await dropSchema(targetSessionId, TARGET_SCHEMA);
    } finally {
      if (sourceSessionId) {
        await invoke('disconnect', { dbSessionId: sourceSessionId }).catch(() => undefined);
        sourceSessionId = '';
      }
      if (targetSessionId) {
        await invoke('disconnect', { dbSessionId: targetSessionId }).catch(() => undefined);
        targetSessionId = '';
      }
      await invoke('delete_connection', { id: SOURCE_CONNECTION_ID }).catch(() => undefined);
      await invoke('delete_connection', { id: TARGET_CONNECTION_ID }).catch(() => undefined);
      await restoreSafeMode();
    }
  });

  it('compares, selects and applies explicit identity values without preview metadata', async function () {
    this.timeout(180_000);

    const options = {
      insert: true,
      update: false,
      delete: false,
      matchingStrategy: 'primaryKey',
      // One row per generated batch exercises IDENTITY_INSERT remaining on
      // across pages and being turned off before the transaction commits.
      batchSize: 1,
      largeValueMode: 'full',
      conflictPolicy: 'abort',
    };

    const comparison = await invoke<SyncComparison>('compare_data_sync', {
      sourceDbSessionId: sourceSessionId,
      targetDbSessionId: targetSessionId,
      tables: [TABLE],
      jobId: null,
      sourceDatabase: DATABASE || null,
      targetDatabase: DATABASE || null,
      sourceSchema: SOURCE_SCHEMA,
      targetSchema: TARGET_SCHEMA,
      options,
      filters: null,
    });
    expect(comparison.planId).not.toBe('');
    expect(comparison.selectionRevision).toBeGreaterThan(0);

    const table = comparison.tables.find(
      (entry) => entry.sourceTable === TABLE && entry.targetTable === TABLE,
    );
    expect(table).toBeDefined();
    expect(table?.status).toBe('MATCHED');
    expect(table?.insertCount).toBe(IDS.length);
    expect(table?.updateCount).toBe(0);
    expect(table?.deleteCount).toBe(0);

    const selection = {
      revision: comparison.selectionRevision,
      rows: [],
      scopes: [
        {
          sourceTable: TABLE,
          targetTable: TABLE,
          selectionMode: 'all',
          operations: ['INSERT'],
          excludedRows: [],
        },
      ],
    };
    const preview = await invoke<SyncSqlStatement[]>('generate_data_sync_sql', {
      planId: comparison.planId,
      selection,
      options,
    });
    expect(preview).toHaveLength(IDS.length);
    expect(preview.every((statement) => statement.operation === 'INSERT')).toBe(true);
    expect(preview.map((statement) => Number(statement.rowKey[0])).sort((a, b) => a - b)).toEqual(
      IDS,
    );
    for (const statement of preview) {
      expect(Object.keys(statement)).toEqual([
        'table',
        'operation',
        'sql',
        'previewSql',
        'parameters',
        'rowKey',
      ]);
      expect(statement.identityInsert).toBeUndefined();
      expect(statement.identity_insert).toBeUndefined();
      expect(statement.sql.toUpperCase()).not.toContain('IDENTITY_INSERT');
      expect(statement.previewSql.toUpperCase()).not.toContain('IDENTITY_INSERT');
    }

    const applied = await invoke<SyncExecutionResult>('execute_data_sync', {
      request: {
        planId: comparison.planId,
        selection,
        options,
        jobId: null,
      },
      profile: null,
    });
    expect(applied.outcome).toBe('committed');
    expect(applied.error).toBeUndefined();
    expect(applied.rolledBack).toBe(false);
    expect(applied.applied).toBe(IDS.length);
    expect(applied.affectedRows).toBe(IDS.length);

    const verify = await run(
      targetSessionId,
      `SELECT [id], [label] FROM ${qualified(TARGET_SCHEMA)} ORDER BY [id]`,
    );
    const rows = verify.results?.[0]?.rows ?? [];
    expect(rows.map((row) => Number(row[0]))).toEqual(IDS);
    expect(rows.map((row) => String(row[1]))).toEqual(IDS.map((id) => `identity-${id}`));
  });
});
