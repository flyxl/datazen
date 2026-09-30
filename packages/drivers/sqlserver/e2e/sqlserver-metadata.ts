/**
 * Live SQL Server catalog journey through the product's public IPC command.
 *
 * This exercises `get_table_schema` against scratch tables that combine a
 * composite primary key, secondary index, composite foreign key, and CHECK
 * constraint. It skips unless the SQL Server E2E connection variables are set.
 */
import { expect } from '@wdio/globals';

const HOST = (process.env.E2E_SQLSERVER_HOST || '').trim();
const PORT = Number(process.env.E2E_SQLSERVER_PORT || '1433');
const USER = (process.env.E2E_SQLSERVER_USER || '').trim();
const PASSWORD = process.env.E2E_SQLSERVER_PASSWORD || '';
const DATABASE = (process.env.E2E_SQLSERVER_DATABASE || '').trim();
const SCHEMA = (process.env.E2E_SQLSERVER_SCHEMA || 'dbo').trim();
const SSL_MODE = (process.env.E2E_SQLSERVER_SSL_MODE || 'require').trim();
const TRUST_CERT = process.env.E2E_SQLSERVER_TRUST_CERT !== '0';

const CONNECTION_ID = 'e2e-sqlserver-metadata';
const scratchSchema = `dz_e2e_meta_${Date.now().toString(36)}`;
const parentTable = 'parent_pair';
const childTable = 'child_pair';
const collationTable = 'collation_probe';
const identityTable = 'identity_probe';
const nonDefaultIdentityTable = 'identity_seed_probe';
const computedTable = 'computed_probe';
const rowversionTable = 'rowversion_probe';
const temporalTable = 'temporal_probe';
const temporalHistoryTable = 'temporal_history_probe';
const filteredIndexTable = 'filtered_index_probe';
const includedIndexTable = 'included_index_probe';
let nonDefaultCollation = '';

interface ColumnSchemaPayload {
  name: string;
  dataType: string;
  nullable: boolean;
  isPrimaryKey: boolean;
  isAutoIncrement: boolean;
}

interface IndexPayload {
  name: string;
  columns: string[];
  isUnique: boolean;
  isPrimary: boolean;
  indexType: string;
}

interface ForeignKeyPayload {
  name: string;
  columns: string[];
  referencedTable: string;
  referencedColumns: string[];
  onUpdate: string;
  onDelete: string;
}

interface CheckConstraintPayload {
  name: string;
  expression: string;
}

interface TableSchemaPayload {
  tableName: string;
  columns: ColumnSchemaPayload[];
  primaryKeys: string[];
  indexes: IndexPayload[];
  foreignKeys: ForeignKeyPayload[];
  checkConstraints: CheckConstraintPayload[];
}

interface StatementPayload {
  rows: Array<Array<string | number | boolean | null>>;
}

interface MultiQueryPayload {
  results: StatementPayload[];
}

async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (command: string, serializedArgs: string, done: (value: unknown) => void) => {
      const internals = (
        window as unknown as {
          __TAURI_INTERNALS__?: { invoke: (name: string, input: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__;
      if (!internals) {
        done({ __error: '__TAURI_INTERNALS__ is unavailable (not a webdriver build?)' });
        return;
      }
      internals
        .invoke(command, JSON.parse(serializedArgs))
        .then((value) => done(value))
        .catch((error: unknown) => done({ __error: String(error) }));
    },
    cmd,
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

describe('SQL Server schema metadata IPC (live)', () => {
  let dbSessionId = '';
  let database = DATABASE;
  let originalSafeMode: boolean | undefined;

  const run = async (sql: string) =>
    invoke<MultiQueryPayload>('execute_query', {
      dbSessionId,
      sql,
      ...(database ? { database } : {}),
    });

  const tableSchema = (table: string) =>
    invoke<TableSchemaPayload>('get_table_schema', {
      dbSessionId,
      table,
      database,
      schema: scratchSchema,
    });

  const qualified = (table: string) => `${bracket(scratchSchema)}.${bracket(table)}`;

  const setSafeMode = async (enabled: boolean) => {
    const settings = await invoke<Record<string, unknown>>('get_settings');
    if (originalSafeMode === undefined) originalSafeMode = settings.safeMode !== false;
    await invoke('save_settings', { settings: { ...settings, safeMode: enabled } });
  };

  const cleanup = async () => {
    if (dbSessionId) {
      try {
        await setSafeMode(false).catch(() => undefined);
        await run(
          `ALTER TABLE ${qualified(temporalTable)} SET (SYSTEM_VERSIONING = OFF)`,
        ).catch(() => undefined);
        for (const table of [
          childTable,
          parentTable,
          collationTable,
          identityTable,
          nonDefaultIdentityTable,
          computedTable,
          rowversionTable,
          temporalTable,
          temporalHistoryTable,
          filteredIndexTable,
          includedIndexTable,
        ]) {
          await run(`DROP TABLE IF EXISTS ${qualified(table)}`).catch((error: unknown) => {
            console.warn(`SQL Server metadata cleanup could not drop ${table}: ${String(error)}`);
          });
        }
        await run(`DROP SCHEMA IF EXISTS ${bracket(scratchSchema)}`).catch((error: unknown) => {
          console.warn(
            `SQL Server metadata cleanup could not drop scratch schema: ${String(error)}`,
          );
        });
      } finally {
        await invoke('disconnect', { dbSessionId }).catch(() => undefined);
        dbSessionId = '';
      }
    }
    await invoke('delete_connection', { id: CONNECTION_ID }).catch(() => undefined);
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
    if (process.env.E2E_SKIP_SQLSERVER === '1' || !HOST || !USER || !PASSWORD) {
      console.warn(
        '⏩ Skipping SQL Server metadata E2E: set E2E_SQLSERVER_HOST / E2E_SQLSERVER_USER / E2E_SQLSERVER_PASSWORD, or E2E_SKIP_SQLSERVER=1',
      );
      this.skip();
    }

    const handles = await browser.getWindowHandles();
    if (handles[0]) await browser.switchToWindow(handles[0]);

    await invoke('save_connection', {
      config: {
        id: CONNECTION_ID,
        name: 'E2E SQL Server metadata',
        databaseType: 'sqlserver',
        host: HOST,
        port: PORT,
        ...(DATABASE ? { database: DATABASE } : {}),
        ...(SCHEMA ? { schema: SCHEMA } : {}),
        username: USER,
        password: PASSWORD,
        sslMode: SSL_MODE,
        connectionTimeout: 30,
        maxPoolSize: 5,
        options: { trustServerCertificate: TRUST_CERT },
      },
    });
    dbSessionId = await invoke<string>('connect', { connectionId: CONNECTION_ID });

    await setSafeMode(false);
    if (!database) {
      const current = await run('SELECT DB_NAME() AS database_name');
      database = String(current.results[0]?.rows[0]?.[0] ?? '');
      if (!database) throw new Error('SQL Server did not return the active database name');
    }

    await run(`CREATE SCHEMA ${bracket(scratchSchema)}`);
    await run(
      `CREATE TABLE ${qualified(parentTable)} (` +
        '[part_b] INT NOT NULL, [part_a] INT NOT NULL, [label] NVARCHAR(40) NULL, ' +
        'CONSTRAINT [PK_parent_pair] PRIMARY KEY ([part_a], [part_b]))',
    );
    await run(
      `CREATE TABLE ${qualified(childTable)} (` +
        '[part_b] INT NOT NULL, [part_a] INT NOT NULL, [rank] INT NOT NULL, ' +
        'CONSTRAINT [PK_child_pair] PRIMARY KEY ([part_b], [part_a]), ' +
        'CONSTRAINT [FK_child_pair_parent] FOREIGN KEY ([part_a], [part_b]) ' +
        `REFERENCES ${qualified(parentTable)} ([part_a], [part_b]) ON DELETE CASCADE, ` +
        'CONSTRAINT [CK_child_pair_rank] CHECK ([rank] >= 0))',
    );
    await run(
      `CREATE NONCLUSTERED INDEX [IX_child_pair_rank] ON ${qualified(childTable)} ([rank], [part_a])`,
    );

    const alternateCollation = await run(
      "SELECT TOP (1) name FROM sys.fn_helpcollations() WHERE name <> CAST(DATABASEPROPERTYEX(DB_NAME(), 'Collation') AS nvarchar(128)) ORDER BY name",
    );
    nonDefaultCollation = String(alternateCollation.results[0]?.rows[0]?.[0] ?? '');
    if (!/^[A-Za-z0-9_]+$/.test(nonDefaultCollation)) {
      throw new Error('SQL Server did not return a usable alternate collation name');
    }
    await run(
      `CREATE TABLE ${qualified(collationTable)} ([text_value] NVARCHAR(20) COLLATE ${nonDefaultCollation} NULL)`,
    );
    await run(
      `CREATE TABLE ${qualified(identityTable)} (` +
        '[id] INT IDENTITY(1,1) NOT NULL PRIMARY KEY, [label] NVARCHAR(40) NULL)',
    );
    await run(
      `CREATE TABLE ${qualified(nonDefaultIdentityTable)} (` +
        '[id] INT IDENTITY(10,2) NOT NULL, [label] NVARCHAR(40) NULL)',
    );
    await run(
      `CREATE TABLE ${qualified(computedTable)} (` +
        '[base_value] INT NOT NULL, [computed_value] AS ([base_value] * 2) PERSISTED)',
    );
    await run(
      `CREATE TABLE ${qualified(rowversionTable)} (` +
        '[id] INT NOT NULL, [row_version] ROWVERSION)',
    );
    await run(
      `CREATE TABLE ${qualified(temporalTable)} (` +
        '[id] INT NOT NULL PRIMARY KEY CLUSTERED, ' +
        '[valid_from] DATETIME2 GENERATED ALWAYS AS ROW START NOT NULL, ' +
        '[valid_to] DATETIME2 GENERATED ALWAYS AS ROW END NOT NULL, ' +
        'PERIOD FOR SYSTEM_TIME ([valid_from], [valid_to])) ' +
        `WITH (SYSTEM_VERSIONING = ON (HISTORY_TABLE = ${qualified(temporalHistoryTable)}))`,
    );
    await run(`CREATE TABLE ${qualified(filteredIndexTable)} ([value] INT NULL)`);
    await run(
      `CREATE NONCLUSTERED INDEX [IX_filtered_index_probe] ON ${qualified(filteredIndexTable)} ([value]) WHERE [value] IS NOT NULL`,
    );
    await run(
      `CREATE TABLE ${qualified(includedIndexTable)} ([id] INT NOT NULL, [payload] NVARCHAR(40) NULL)`,
    );
    await run(
      `CREATE NONCLUSTERED INDEX [IX_included_index_probe] ON ${qualified(includedIndexTable)} ([id]) INCLUDE ([payload])`,
    );
  });

  after(async function () {
    this.timeout(60_000);
    await cleanup();
  });

  it('reads composite keys and secondary constraints through get_table_schema IPC', async () => {
    const parent = await tableSchema(parentTable);
    expect(parent.columns.map((column) => column.name)).toEqual(['part_b', 'part_a', 'label']);
    expect(parent.primaryKeys).toEqual(['part_a', 'part_b']);
    expect(
      parent.columns.filter((column) => column.isPrimaryKey).map((column) => column.name),
    ).toEqual(['part_b', 'part_a']);

    const child = await tableSchema(childTable);
    expect(child.primaryKeys).toEqual(['part_b', 'part_a']);

    const index = child.indexes.find((item) => item.name === 'IX_child_pair_rank');
    expect(index).toBeDefined();
    expect(index?.columns).toEqual(['rank', 'part_a']);
    expect(index?.isPrimary).toBe(false);
    expect(index?.indexType).toBe('NONCLUSTERED');

    expect(child.foreignKeys).toHaveLength(1);
    expect(child.foreignKeys[0]).toMatchObject({
      name: 'FK_child_pair_parent',
      columns: ['part_a', 'part_b'],
      referencedTable: `[${scratchSchema}].[${parentTable}]`,
      referencedColumns: ['part_a', 'part_b'],
      onDelete: 'CASCADE',
    });

    expect(child.checkConstraints).toHaveLength(1);
    expect(child.checkConstraints[0]?.name).toBe('CK_child_pair_rank');
    expect(child.checkConstraints[0]?.expression.toLowerCase()).toContain('rank');
    expect(child.checkConstraints[0]?.expression).toContain('0');
  });

  it('refuses non-default collations that TableSchema cannot represent', async () => {
    let error = '';
    try {
      await tableSchema(collationTable);
    } catch (cause) {
      error = String(cause);
    }
    expect(error.toLowerCase()).toMatch(/collation|unsupported|not represent/);
  });

  it('preserves SQL Server IDENTITY(1,1) as the auto-increment flag', async () => {
    const schema = await tableSchema(identityTable);
    expect(schema.columns).toEqual(
      expect.arrayContaining([
        expect.objectContaining({ name: 'id', isAutoIncrement: true }),
        expect.objectContaining({ name: 'label', isAutoIncrement: false }),
      ]),
    );
  });

  it('rejects non-default IDENTITY seed and increment values explicitly', async () => {
    let error = '';
    try {
      await tableSchema(nonDefaultIdentityTable);
    } catch (cause) {
      error = String(cause);
    }
    expect(error).toMatch(/unsupported/i);
    expect(error).toMatch(/non-default IDENTITY seed\/increment/i);
  });

  it('rejects computed columns with an explicit Unsupported error', async () => {
    let error = '';
    try {
      await tableSchema(computedTable);
    } catch (cause) {
      error = String(cause);
    }
    expect(error).toMatch(/unsupported/i);
    expect(error).toMatch(/computed columns/i);
  });

  it('rejects rowversion columns with an explicit Unsupported error', async () => {
    let error = '';
    try {
      await tableSchema(rowversionTable);
    } catch (cause) {
      error = String(cause);
    }
    expect(error).toMatch(/unsupported/i);
    expect(error).toMatch(/rowversion columns/i);
  });

  it('rejects temporal generated-always columns with an explicit Unsupported error', async () => {
    let error = '';
    try {
      await tableSchema(temporalTable);
    } catch (cause) {
      error = String(cause);
    }
    expect(error).toMatch(/unsupported/i);
    expect(error).toMatch(/generated-always columns/i);
  });

  it('rejects filtered indexes with the index name and unsupported feature in the error', async () => {
    let error = '';
    try {
      await tableSchema(filteredIndexTable);
    } catch (cause) {
      error = String(cause);
    }
    expect(error).toMatch(/unsupported/i);
    expect(error).toContain('IX_filtered_index_probe');
    expect(error).toMatch(/filtered/i);
  });

  it('rejects INCLUDE index columns with the index name and unsupported feature in the error', async () => {
    let error = '';
    try {
      await tableSchema(includedIndexTable);
    } catch (cause) {
      error = String(cause);
    }
    expect(error).toMatch(/unsupported/i);
    expect(error).toContain('IX_included_index_probe');
    expect(error).toMatch(/INCLUDE or descending key columns/i);
  });
});
