/**
 * SQL Server Data Transfer structure-mode live journey through the public IPC
 * surface.
 *
 * Transfers a SQL Server source schema into a fresh SQL Server target schema in
 * `structure` mode and proves that the secondary index and foreign key objects
 * survive the crossing. The source fixture deliberately carries one index for
 * every index shape the catalog parser reports — a plain NONCLUSTERED index, a
 * UNIQUE index, and a UNIQUE constraint (reported as
 * `UNIQUE_CONSTRAINT:NONCLUSTERED`) — because the driver is responsible for
 * erasing that SQL Server-only vocabulary before the host renders portable DDL.
 *
 * Every assertion that matters is made against the *target* catalog
 * (`sys.indexes` / `sys.foreign_keys`) rather than against the generated DDL
 * text, so a change in wording cannot make a passing transfer look broken.
 *
 * Every object belongs to a unique dz_e2e_xfer_* schema created by this spec.
 */
import { expect } from '@wdio/globals';

const HOST = (process.env.E2E_SQLSERVER_HOST || '').trim();
const PORT = Number(process.env.E2E_SQLSERVER_PORT || '1433');
const USER = (process.env.E2E_SQLSERVER_USER || '').trim();
const PASSWORD = process.env.E2E_SQLSERVER_PASSWORD || '';
const DATABASE = (process.env.E2E_SQLSERVER_DATABASE || '').trim();
const SSL_MODE = (process.env.E2E_SQLSERVER_SSL_MODE || 'require').trim();
const TRUST_CERT = process.env.E2E_SQLSERVER_TRUST_CERT !== '0';

const STAMP = `${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 7)}`;
const SOURCE_CONNECTION_ID = `e2e-sqlserver-xfer-source-${STAMP}`;
const TARGET_CONNECTION_ID = `e2e-sqlserver-xfer-target-${STAMP}`;
const SOURCE_SCHEMA = `dz_e2e_xfer_src_${STAMP}`;
const TARGET_SCHEMA = `dz_e2e_xfer_tgt_${STAMP}`;
const PARENT_TABLE = 'xfer_parent';
const CHILD_TABLE = 'xfer_child';

/** Secondary index names, in the shape the SQL Server catalog reports them. */
const PLAIN_INDEX = 'ix_xfer_child_code';
const UNIQUE_INDEX = 'ux_xfer_child_parent';
const UNIQUE_CONSTRAINT_INDEX = 'uq_xfer_child_label';
const FOREIGN_KEY = 'fk_xfer_child_parent';
const SECONDARY_INDEXES = [PLAIN_INDEX, UNIQUE_INDEX, UNIQUE_CONSTRAINT_INDEX];

interface StatementPayload {
  rows: Array<Array<string | number | boolean | null>>;
}

interface MultiQueryPayload {
  results: StatementPayload[];
}

interface DdlPreviewItem {
  sourceTable: string;
  targetTable: string;
  ddl: string;
  kind: string;
  dependsOn: string[];
}

interface TransferPreview {
  planId: string;
  pairingPath: string;
  canExecute: boolean;
  blockReason?: string | null;
  ddl: DdlPreviewItem[];
  writePlans: unknown[];
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
    const detail = String((result as { __error: string }).__error);
    throw new Error(PASSWORD ? detail.replaceAll(PASSWORD, '[redacted]') : detail);
  }
  return result as T;
}

function bracket(name: string): string {
  return `[${name.replaceAll(']', ']]')}]`;
}

function literal(value: string): string {
  return `N'${value.replaceAll("'", "''")}'`;
}

function skipReason(): string | null {
  if (process.env.E2E_SKIP_SQLSERVER === '1') return 'E2E_SKIP_SQLSERVER=1';
  if (!HOST || !USER || !PASSWORD) {
    return 'set E2E_SQLSERVER_HOST / E2E_SQLSERVER_USER / E2E_SQLSERVER_PASSWORD (see README.md)';
  }
  return null;
}

describe('SQL Server Data Transfer structure mode (live)', () => {
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

  /** Run a catalog query and return its single result set as text rows. */
  const queryRows = async (dbSessionId: string, sql: string) => {
    const payload = await run(dbSessionId, sql);
    return payload.results[0]?.rows ?? [];
  };

  const tableRef = (schema: string, table: string) => `${bracket(schema)}.${bracket(table)}`;

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
        // SQL Server refuses to drop a schema that still holds objects, so the
        // tables have to go first or the schema drop silently fails.
        for (const schema of [SOURCE_SCHEMA, TARGET_SCHEMA]) {
          for (const table of [PARENT_TABLE, CHILD_TABLE]) {
            await run(cleanupSession, `DROP TABLE IF EXISTS ${tableRef(schema, table)}`).catch(
              () => undefined,
            );
          }
          await run(cleanupSession, `DROP SCHEMA IF EXISTS ${bracket(schema)}`).catch(
            () => undefined,
          );
        }
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
      console.warn(`⏩ Skipping SQL Server structure transfer E2E: ${reason}`);
      this.skip();
    }

    const handles = await browser.getWindowHandles();
    if (handles[0]) await browser.switchToWindow(handles[0]);
    await browser.setTimeout({ script: 180_000 });

    await invoke('save_connection', {
      config: connectionConfig(SOURCE_CONNECTION_ID, 'E2E SQL Server structure source'),
    });
    await invoke('save_connection', {
      config: connectionConfig(TARGET_CONNECTION_ID, 'E2E SQL Server structure target'),
    });
    sourceSessionId = await invoke<string>('connect', { connectionId: SOURCE_CONNECTION_ID });
    targetSessionId = await invoke<string>('connect', { connectionId: TARGET_CONNECTION_ID });

    if (!database) {
      const active = await queryRows(sourceSessionId, 'SELECT DB_NAME()');
      database = String(active[0]?.[0] ?? '');
      if (!database) throw new Error('SQL Server did not return its active database name');
    }

    await setSafeMode(false);

    // Both schemas are created; the target tables deliberately are not, so the
    // structure phase has to create them through `createNew`.
    await run(sourceSessionId, `CREATE SCHEMA ${bracket(SOURCE_SCHEMA)}`);
    await run(sourceSessionId, `CREATE SCHEMA ${bracket(TARGET_SCHEMA)}`);

    const parent = tableRef(SOURCE_SCHEMA, PARENT_TABLE);
    const child = tableRef(SOURCE_SCHEMA, CHILD_TABLE);

    await run(
      sourceSessionId,
      `CREATE TABLE ${parent} (` +
        '[id] INT IDENTITY(1,1) NOT NULL PRIMARY KEY, ' +
        '[code] NVARCHAR(64) NOT NULL)',
    );
    await run(
      sourceSessionId,
      `CREATE TABLE ${child} (` +
        '[id] INT IDENTITY(1,1) NOT NULL PRIMARY KEY, ' +
        '[parent_id] INT NOT NULL, ' +
        '[code] NVARCHAR(64) NOT NULL, ' +
        '[label] NVARCHAR(64) NOT NULL, ' +
        `CONSTRAINT ${bracket(FOREIGN_KEY)} FOREIGN KEY ([parent_id]) ` +
        `REFERENCES ${parent} ([id]))`,
    );

    // Plain secondary index -> catalog type NONCLUSTERED.
    await run(sourceSessionId, `CREATE INDEX ${bracket(PLAIN_INDEX)} ON ${child} ([code])`);
    // UNIQUE index -> catalog type NONCLUSTERED with is_unique set.
    await run(
      sourceSessionId,
      `CREATE UNIQUE INDEX ${bracket(UNIQUE_INDEX)} ON ${child} ([parent_id])`,
    );
    // UNIQUE constraint -> catalog type UNIQUE_CONSTRAINT:NONCLUSTERED.
    await run(
      sourceSessionId,
      `ALTER TABLE ${child} ADD CONSTRAINT ${bracket(UNIQUE_CONSTRAINT_INDEX)} UNIQUE ([label])`,
    );

    await run(
      sourceSessionId,
      `INSERT INTO ${parent} ([code]) VALUES (N'parent one'), (N'parent two')`,
    );
    await run(
      sourceSessionId,
      `INSERT INTO ${child} ([parent_id], [code], [label]) VALUES ` +
        `(1, N'child one', N'label one'), (2, N'child two', N'label two')`,
    );
  });

  after(async function () {
    this.timeout(60_000);
    await cleanup();
  });

  it('creates the target tables with their secondary indexes and foreign key', async () => {
    const preview = await invoke<TransferPreview>('preview_data_transfer', {
      job: {
        source: { dbSessionId: sourceSessionId, database, schema: SOURCE_SCHEMA },
        target: { dbSessionId: targetSessionId, database, schema: TARGET_SCHEMA },
        mode: 'structure',
        writeMode: 'insert',
        tables: [
          { sourceTable: PARENT_TABLE, targetTable: PARENT_TABLE, createNew: true, enabled: true },
          { sourceTable: CHILD_TABLE, targetTable: CHILD_TABLE, createNew: true, enabled: true },
        ],
        // SQL Server requires an explicit choice when text collation semantics cannot be proven.
        options: { batchSize: 100, stopOnError: true, useTargetDefaultCollation: true },
      },
    });

    expect(preview.pairingPath).toBe('direct');
    if (!preview.canExecute) {
      throw new Error(preview.blockReason || 'structure preview should be executable');
    }
    expect(preview.planId).not.toBe('');

    const tableItems = preview.ddl.filter((item) => item.kind === 'table');
    const indexItems = preview.ddl.filter((item) => item.kind === 'index');
    const foreignKeyItems = preview.ddl.filter((item) => item.kind === 'foreignKey');

    expect(tableItems.map((item) => item.sourceTable).sort()).toEqual(
      [CHILD_TABLE, PARENT_TABLE].sort(),
    );
    expect(indexItems.every((item) => item.targetTable === CHILD_TABLE)).toBe(true);
    expect(indexItems.length).toBe(SECONDARY_INDEXES.length);
    for (const name of SECONDARY_INDEXES) {
      const item = indexItems.find((candidate) => candidate.ddl.includes(name));
      expect(item?.ddl).toBeDefined();
      // The driver must erase the SQL Server-only vocabulary so the host can
      // render portable DDL; a leaked token means the bridge regressed.
      expect(item?.ddl).not.toMatch(/CLUSTERED/i);
      expect(item?.ddl).not.toMatch(/UNIQUE_CONSTRAINT/i);
    }

    expect(foreignKeyItems).toHaveLength(1);
    const foreignKeyItem = foreignKeyItems[0]!;
    expect(foreignKeyItem.sourceTable).toBe(CHILD_TABLE);
    expect(foreignKeyItem.ddl).toContain(FOREIGN_KEY);
    expect(foreignKeyItem.ddl).toContain('REFERENCES');
    // The reference must point at the target schema, never the source one.
    expect(foreignKeyItem.ddl).toContain(tableRef(TARGET_SCHEMA, PARENT_TABLE));
    expect(foreignKeyItem.ddl).not.toContain(SOURCE_SCHEMA);

    // Emission order is a contract in its own right: every table block, then
    // every index block, then every foreign-key block. Mirrors DT-OBJ-1 in
    // `e2e/specs/data-transfer-structure-objects.ts:265-266`, which asserts the
    // same chain over the rendered DOM; this asserts it over the IPC payload so
    // a dialect bridge that reorders its items still fails here.
    //
    // Deliberately index-based and NOT order-insensitive: comparing sorted name
    // lists (see the `tableItems` / `secondaryRows` assertions above) proves set
    // equality only and cannot observe ordering at all.
    const lastOf = (kind: string) =>
      preview.ddl.reduce((acc, item, i) => (item.kind === kind ? i : acc), -1);
    const firstOf = (kind: string) => preview.ddl.findIndex((item) => item.kind === kind);
    // Non-vacuous: every kind must actually be present, or `lastOf` returns -1
    // and `toBeLessThan` would compare against a sentinel rather than a position.
    expect(lastOf('table')).toBeGreaterThan(-1);
    expect(lastOf('index')).toBeGreaterThan(-1);
    expect(firstOf('foreignKey')).toBeGreaterThan(-1);
    expect(lastOf('table')).toBeLessThan(firstOf('index'));
    expect(lastOf('table')).toBeLessThan(firstOf('foreignKey'));
    expect(lastOf('index')).toBeLessThan(firstOf('foreignKey'));

    await invoke('execute_data_transfer', {
      request: {
        planId: preview.planId,
        selection: { sourceTables: [PARENT_TABLE, CHILD_TABLE] },
      },
    });

    // The target parent table now exists and carries a clustered primary key.
    const primaryKeyRows = await queryRows(
      targetSessionId,
      'SELECT i.name, i.type_desc FROM sys.indexes AS i ' +
        'JOIN sys.tables AS t ON t.object_id = i.object_id ' +
        'JOIN sys.schemas AS s ON s.schema_id = t.schema_id ' +
        `WHERE s.name = ${literal(TARGET_SCHEMA)} AND t.name = ${literal(PARENT_TABLE)} ` +
        'AND i.is_primary_key = 1',
    );
    expect(primaryKeyRows).toHaveLength(1);
    expect(primaryKeyRows[0]?.[1]).toBe('CLUSTERED');

    // All three secondary indexes landed with the uniqueness the plan promised.
    const secondaryRows = await queryRows(
      targetSessionId,
      'SELECT i.name, i.is_unique FROM sys.indexes AS i ' +
        'JOIN sys.tables AS t ON t.object_id = i.object_id ' +
        'JOIN sys.schemas AS s ON s.schema_id = t.schema_id ' +
        `WHERE s.name = ${literal(TARGET_SCHEMA)} AND t.name = ${literal(CHILD_TABLE)} ` +
        `AND i.name IN (${SECONDARY_INDEXES.map(literal).join(', ')})`,
    );
    expect(secondaryRows.map((row) => row[0]).sort()).toEqual([...SECONDARY_INDEXES].sort());
    const uniqueness = new Map(secondaryRows.map((row) => [String(row[0]), Boolean(row[1])]));
    expect(uniqueness.get(PLAIN_INDEX)).toBe(false);
    expect(uniqueness.get(UNIQUE_INDEX)).toBe(true);
    expect(uniqueness.get(UNIQUE_CONSTRAINT_INDEX)).toBe(true);

    // The foreign key landed once, pointing at the *target* parent table.
    const foreignKeyRows = await queryRows(
      targetSessionId,
      'SELECT OBJECT_SCHEMA_NAME(fk.referenced_object_id), OBJECT_NAME(fk.referenced_object_id), ' +
        'fk.delete_referential_action_desc, fk.update_referential_action_desc ' +
        `FROM sys.foreign_keys AS fk WHERE fk.name = ${literal(FOREIGN_KEY)}`,
    );
    expect(foreignKeyRows).toHaveLength(1);
    expect(foreignKeyRows[0]?.[0]).toBe(TARGET_SCHEMA);
    expect(foreignKeyRows[0]?.[1]).toBe(PARENT_TABLE);
    expect(foreignKeyRows[0]?.[2]).toBe('NO_ACTION');
    expect(foreignKeyRows[0]?.[3]).toBe('NO_ACTION');

    // The child rows really are enforced against the target parent table.
    const orphanCount = await queryRows(
      targetSessionId,
      'SELECT COUNT(*) FROM ' +
        `${tableRef(TARGET_SCHEMA, CHILD_TABLE)} AS c ` +
        `LEFT JOIN ${tableRef(TARGET_SCHEMA, PARENT_TABLE)} AS p ON p.[id] = c.[parent_id] ` +
        'WHERE p.[id] IS NULL',
    );
    expect(Number(orphanCount[0]?.[0])).toBe(0);

    // The transfer ran in `structure` mode, so the tables exist but the rows
    // deliberately do not travel with them.
    const copiedRows = await queryRows(
      targetSessionId,
      `SELECT COUNT(*) FROM ${tableRef(TARGET_SCHEMA, CHILD_TABLE)}`,
    );
    expect(Number(copiedRows[0]?.[0])).toBe(0);
  });
});
