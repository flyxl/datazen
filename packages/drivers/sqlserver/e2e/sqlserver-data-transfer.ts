/**
 * SQL Server Data Transfer live journey through the public IPC surface.
 *
 * Copies explicit source identity values into an existing SQL Server identity
 * table, then verifies the destination rows. Every object belongs to a unique
 * dz_e2e_transfer_* schema created by this spec.
 */
import { expect } from '@wdio/globals';

const HOST = (process.env.E2E_SQLSERVER_HOST || '').trim();
const PORT = Number(process.env.E2E_SQLSERVER_PORT || '1433');
const USER = (process.env.E2E_SQLSERVER_USER || '').trim();
const PASSWORD = process.env.E2E_SQLSERVER_PASSWORD || '';
const DATABASE = (process.env.E2E_SQLSERVER_DATABASE || '').trim();
const SSL_MODE = (process.env.E2E_SQLSERVER_SSL_MODE || 'require').trim();
const TRUST_CERT = process.env.E2E_SQLSERVER_TRUST_CERT !== '0';

const SOURCE_CONNECTION_ID = 'e2e-sqlserver-transfer-source';
const TARGET_CONNECTION_ID = 'e2e-sqlserver-transfer-target';
const TABLE = 'identity_rows';
const SOURCE_SCHEMA = `dz_e2e_transfer_src_${Date.now().toString(36)}`;
const TARGET_SCHEMA = `dz_e2e_transfer_tgt_${Date.now().toString(36)}`;

interface StatementPayload {
  rows: Array<Array<string | number | boolean | null>>;
}

interface MultiQueryPayload {
  results: StatementPayload[];
}

interface TransferPreview {
  planId: string;
  pairingPath: string;
  canExecute: boolean;
  blockReason?: string | null;
  writePlans: Array<{
    sourceTable: string;
    targetTable: string;
    mappedColumns: Array<{ sourceColumn: string; targetColumn: string; skip?: boolean }>;
  }>;
}

async function invoke<T>(command: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (name: string, serializedArgs: string, done: (value: unknown) => void) => {
      const internals = (
        window as unknown as {
          __TAURI_INTERNALS__?: { invoke: (command: string, input: unknown) => Promise<unknown> };
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

function bracket(name: string): string {
  return `[${name.replaceAll(']', ']]')}]`;
}

function skipReason(): string | null {
  if (process.env.E2E_SKIP_SQLSERVER === '1') return 'E2E_SKIP_SQLSERVER=1';
  if (!HOST || !USER || !PASSWORD) {
    return 'set E2E_SQLSERVER_HOST / E2E_SQLSERVER_USER / E2E_SQLSERVER_PASSWORD (see README.md)';
  }
  return null;
}

describe('SQL Server Data Transfer (live)', () => {
  let sourceSessionId = '';
  let targetSessionId = '';
  let database = DATABASE;
  let originalSafeMode: boolean | undefined;

  const run = (dbSessionId: string, sql: string) =>
    invoke<MultiQueryPayload>('execute_query', {
      dbSessionId,
      sql,
      ...(database ? { database } : {}),
    });

  const qualified = (schema: string) => `${bracket(schema)}.${bracket(TABLE)}`;

  const setSafeMode = async (enabled: boolean) => {
    const settings = await invoke<Record<string, unknown>>('get_settings');
    if (originalSafeMode === undefined) originalSafeMode = settings.safeMode !== false;
    await invoke('save_settings', { settings: { ...settings, safeMode: enabled } });
  };

  const connectionConfig = (id: string, name: string) => ({
    id,
    name,
    databaseType: 'sqlserver',
    host: HOST,
    port: PORT,
    ...(DATABASE ? { database: DATABASE } : {}),
    username: USER,
    password: PASSWORD,
    sslMode: SSL_MODE,
    connectionTimeout: 30,
    maxPoolSize: 5,
    options: { trustServerCertificate: TRUST_CERT },
  });

  const cleanup = async () => {
    const cleanupSession = sourceSessionId || targetSessionId;
    if (cleanupSession) {
      try {
        await setSafeMode(false);
        await run(cleanupSession, `DROP TABLE IF EXISTS ${qualified(SOURCE_SCHEMA)}`).catch(
          () => undefined,
        );
        await run(cleanupSession, `DROP TABLE IF EXISTS ${qualified(TARGET_SCHEMA)}`).catch(
          () => undefined,
        );
        await run(cleanupSession, `DROP SCHEMA IF EXISTS ${bracket(SOURCE_SCHEMA)}`).catch(
          () => undefined,
        );
        await run(cleanupSession, `DROP SCHEMA IF EXISTS ${bracket(TARGET_SCHEMA)}`).catch(
          () => undefined,
        );
      } finally {
        if (sourceSessionId) {
          await invoke('disconnect', { dbSessionId: sourceSessionId }).catch(() => undefined);
          sourceSessionId = '';
        }
        if (targetSessionId) {
          await invoke('disconnect', { dbSessionId: targetSessionId }).catch(() => undefined);
          targetSessionId = '';
        }
      }
    }

    await invoke('delete_connection', { id: SOURCE_CONNECTION_ID }).catch(() => undefined);
    await invoke('delete_connection', { id: TARGET_CONNECTION_ID }).catch(() => undefined);
    if (originalSafeMode !== undefined) {
      const settings = await invoke<Record<string, unknown>>('get_settings').catch(() => null);
      if (settings) {
        await invoke('save_settings', {
          settings: { ...settings, safeMode: originalSafeMode },
        }).catch(() => undefined);
      }
    }
  };

  before(async function () {
    this.timeout(120_000);
    const reason = skipReason();
    if (reason) {
      console.warn(`⏩ Skipping SQL Server Data Transfer E2E: ${reason}`);
      this.skip();
    }

    const handles = await browser.getWindowHandles();
    if (handles[0]) await browser.switchToWindow(handles[0]);

    await invoke('save_connection', {
      config: connectionConfig(SOURCE_CONNECTION_ID, 'E2E SQL Server transfer source'),
    });
    await invoke('save_connection', {
      config: connectionConfig(TARGET_CONNECTION_ID, 'E2E SQL Server transfer target'),
    });
    sourceSessionId = await invoke<string>('connect', { connectionId: SOURCE_CONNECTION_ID });
    targetSessionId = await invoke<string>('connect', { connectionId: TARGET_CONNECTION_ID });

    if (!database) {
      const active = await run(sourceSessionId, 'SELECT DB_NAME() AS database_name');
      database = String(active.results[0]?.rows[0]?.[0] ?? '');
      if (!database) throw new Error('SQL Server did not return its active database name');
    }

    await setSafeMode(false);
    await run(sourceSessionId, `CREATE SCHEMA ${bracket(SOURCE_SCHEMA)}`);
    await run(sourceSessionId, `CREATE SCHEMA ${bracket(TARGET_SCHEMA)}`);
    await run(
      sourceSessionId,
      `CREATE TABLE ${qualified(SOURCE_SCHEMA)} (` +
        '[id] INT IDENTITY(1,1) NOT NULL PRIMARY KEY, [payload] NVARCHAR(80) NOT NULL)',
    );
    await run(
      targetSessionId,
      `CREATE TABLE ${qualified(TARGET_SCHEMA)} (` +
        '[id] INT IDENTITY(1,1) NOT NULL PRIMARY KEY, [payload] NVARCHAR(80) NOT NULL)',
    );
    // Make generated target IDs differ from the source so the assertion below
    // proves that Transfer inserted the explicit source identity values.
    await run(targetSessionId, `DBCC CHECKIDENT (N'${qualified(TARGET_SCHEMA)}', RESEED, 100)`);
    await run(
      sourceSessionId,
      `INSERT INTO ${qualified(SOURCE_SCHEMA)} ([payload]) VALUES ` +
        `(N'identity alpha'), (N'identity beta'), (N'identity gamma')`,
    );
  });

  after(async function () {
    this.timeout(60_000);
    await cleanup();
  });

  it('copies source identity values into a matching target identity column', async () => {
    const preview = await invoke<TransferPreview>('preview_data_transfer', {
      job: {
        source: { dbSessionId: sourceSessionId, database, schema: SOURCE_SCHEMA },
        target: { dbSessionId: targetSessionId, database, schema: TARGET_SCHEMA },
        mode: 'data',
        writeMode: 'insert',
        tables: [{ sourceTable: TABLE, targetTable: TABLE, enabled: true }],
        options: { batchSize: 2, stopOnError: true },
      },
    });

    expect(preview.pairingPath).toBe('direct');
    if (!preview.canExecute) {
      throw new Error(preview.blockReason || 'transfer preview should be executable');
    }
    expect(preview.planId).not.toBe('');
    expect(preview.writePlans).toHaveLength(1);
    expect(preview.writePlans[0]?.mappedColumns).toHaveLength(2);
    expect(
      preview.writePlans[0]?.mappedColumns.some(
        (mapping) =>
          mapping.sourceColumn === 'id' && mapping.targetColumn === 'id' && mapping.skip !== true,
      ),
    ).toBe(true);

    await invoke('execute_data_transfer', {
      request: { planId: preview.planId, selection: { sourceTables: [TABLE] } },
    });

    const copied = await run(
      targetSessionId,
      `SELECT [id], [payload] FROM ${qualified(TARGET_SCHEMA)} ORDER BY [id]`,
    );
    expect(copied.results[0]?.rows).toEqual([
      [1, 'identity alpha'],
      [2, 'identity beta'],
      [3, 'identity gamma'],
    ]);

    // A normal generated-identity insert on the same target session proves the
    // transfer left IDENTITY_INSERT OFF after copying explicit values.
    await run(
      targetSessionId,
      `INSERT INTO ${qualified(TARGET_SCHEMA)} ([payload]) VALUES (N'after transfer')`,
    );
    const afterTransfer = await run(
      targetSessionId,
      `SELECT [id], [payload] FROM ${qualified(TARGET_SCHEMA)} WHERE [payload] = N'after transfer'`,
    );
    expect(afterTransfer.results[0]?.rows).toHaveLength(1);
    expect(Number(afterTransfer.results[0]?.rows[0]?.[0])).toBeGreaterThan(3);
  });
});
