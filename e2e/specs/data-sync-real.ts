/**
 * Host IPC contract E2E for Data Sync.
 *
 * Covers: `inspect_data_sync`, `compare_data_sync`, `generate_data_sync_sql`,
 * `execute_data_sync`, `apply_data_sync`, `revalidate_data_sync`. Uses live PostgreSQL / MySQL fixtures;
 * asserts Host-level IPC only — driver dialect tests belong in `packages/drivers/<id>/`.
 *
 * Prerequisites: run `e2e/setup-sync-dbs.sh` for `datazen_sync_src` / `datazen_sync_tgt`
 * and the restricted `datazen_readonly` user. Apply/recompare tests require writable PG target.
 *
 * UI Diff Workspace smoke: `e2e/specs/data-sync-window.ts`.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  invokeBackend,
  sqlBlockedBySafeMode,
  disconnectBackend,
  withSafeModeOff,
} from '../helpers.js';

// ── Connection configs (credentials from environment variables) ─────

const PG_SRC = {
  id: 'sync_pg_src',
  name: 'SyncTest-PG-Src',
  databaseType: 'postgresql',
  host: process.env.E2E_PG_HOST || 'localhost',
  port: Number(process.env.E2E_PG_PORT) || 5432,
  database: 'datazen_sync_src',
  username: process.env.E2E_PG_USER || 'postgres',
  password: process.env.E2E_PG_PASSWORD || '',
  sslMode: 'disable',
};

const PG_TGT = {
  id: 'sync_pg_tgt',
  name: 'SyncTest-PG-Tgt',
  databaseType: 'postgresql',
  host: process.env.E2E_PG_HOST || 'localhost',
  port: Number(process.env.E2E_PG_PORT) || 5432,
  database: 'datazen_sync_tgt',
  username: process.env.E2E_PG_USER || 'postgres',
  password: process.env.E2E_PG_PASSWORD || '',
  sslMode: 'disable',
};

const PG_RO = {
  id: 'sync_pg_ro',
  name: 'SyncTest-PG-RO',
  databaseType: 'postgresql',
  host: process.env.E2E_PG_HOST || 'localhost',
  port: Number(process.env.E2E_PG_PORT) || 5432,
  database: 'datazen_sync_tgt',
  username: process.env.E2E_PG_RO_USER || 'datazen_readonly',
  password: process.env.E2E_PG_RO_PASSWORD || '',
  sslMode: 'disable',
};

const MY_TGT = {
  id: 'sync_my_tgt',
  name: 'E2E-MySQL-Types',
  databaseType: 'mysql',
  host: process.env.E2E_MYSQL_HOST || '127.0.0.1',
  port: Number(process.env.E2E_MYSQL_PORT) || 3306,
  database: process.env.E2E_MYSQL_DB || 'datazen_test',
  username: process.env.E2E_MYSQL_USER || 'root',
  password: process.env.E2E_MYSQL_PASSWORD || '',
  sslMode: 'disable',
};

const MY_RO = {
  id: 'sync_my_ro',
  name: 'SyncTest-MY-RO',
  databaseType: 'mysql',
  host: process.env.E2E_MYSQL_HOST || '127.0.0.1',
  port: Number(process.env.E2E_MYSQL_PORT) || 3306,
  database: process.env.E2E_MYSQL_DB || 'datazen_test',
  username: process.env.E2E_MYSQL_RO_USER || 'datazen_readonly',
  password: process.env.E2E_MYSQL_RO_PASSWORD || '',
  sslMode: 'disable',
};

const ALL_CONFIGS = [PG_SRC, PG_TGT, PG_RO, MY_TGT, MY_RO];

// ── Helpers ─────────────────────────────────────────────────────────

async function expectCommandNotFound(invoke: () => Promise<unknown>): Promise<void> {
  let message = '';
  try {
    const result = await invoke();
    if (result && typeof result === 'object' && 'error' in result) {
      message = String(result.error ?? '');
    }
  } catch (e) {
    message = e instanceof Error ? e.message : String(e);
  }
  expect(message.toLowerCase()).toMatch(/command .+ not found|unknown command/i);
}

async function saveAndConnect(cfg: typeof PG_SRC): Promise<string> {
  await invokeBackend('save_connection', { config: cfg });
  return invokeBackend<string>('connect', { connectionId: cfg.id });
}

async function runSQL(dbSessionId: string, sql: string): Promise<void> {
  const run = () => invokeBackend('execute_query', { dbSessionId, sql });
  if (sqlBlockedBySafeMode(sql)) {
    await withSafeModeOff(run);
    return;
  }
  await run();
}

interface InspectResult {
  sourceTable: string;
  targetTable: string;
  status: string;
}

interface CompareDataSyncResult {
  sourceTable: string;
  targetTable: string;
  status: string;
  rowCount?: number;
  pageSize?: number;
  firstCursor?: string | null;
}

interface CompareDataSyncPreview {
  planId: string;
  selectionRevision: number;
  pageSize: number;
  tables: CompareDataSyncResult[];
}

interface CompareDataSyncPage {
  rows: Array<{ operation: string; key: unknown[]; selected?: boolean }>;
  nextCursor: string | null;
}

interface DataSyncSelection {
  revision: number;
  rows: Array<{
    sourceTable: string;
    targetTable: string;
    operation: string;
    key: unknown[];
  }>;
}

const SYNC_EXEC_OPTIONS = {
  insert: true,
  update: true,
  delete: false,
  matchingStrategy: 'primaryKey',
  batchSize: 1000,
  largeValueMode: 'full',
};

interface ExecutionResult {
  applied: number;
  rolledBack: boolean;
  outcome?: 'not_started' | 'committed' | 'rolled_back' | 'partially_applied' | 'unknown';
  error?: string;
  rollbackReason?: string;
  conflicts?: Array<{ table: string; operation: string; rowKey: unknown[]; message: string }>;
}

interface SqlStatement {
  table: string;
  operation: string;
  sql: string;
}

async function comparisonRows(
  preview: CompareDataSyncPreview,
  tableName: string,
): Promise<Array<{ operation: string; key: unknown[]; selected?: boolean }>> {
  const table = preview.tables.find((candidate) => candidate.sourceTable === tableName);
  if (!table) throw new Error(`comparison table ${tableName} is missing`);
  const rows: Array<{ operation: string; key: unknown[]; selected?: boolean }> = [];
  let cursor = table.firstCursor ?? null;
  while (cursor) {
    const page = await invokeBackend<CompareDataSyncPage>('get_data_sync_comparison_page', {
      request: {
        planId: preview.planId,
        sourceTable: table.sourceTable,
        targetTable: table.targetTable,
        cursor,
        limit: table.pageSize ?? preview.pageSize,
      },
    });
    rows.push(...page.rows);
    cursor = page.nextCursor;
  }
  return rows;
}

async function selectionFor(
  preview: CompareDataSyncPreview,
  tableName: string,
  operation?: string,
): Promise<DataSyncSelection> {
  const table = preview.tables.find((candidate) => candidate.sourceTable === tableName);
  if (!table) throw new Error(`comparison table ${tableName} is missing`);
  const rows: DataSyncSelection['rows'] = [];
  let cursor = table.firstCursor ?? null;
  while (cursor) {
    const page = await invokeBackend<CompareDataSyncPage>('get_data_sync_comparison_page', {
      request: {
        planId: preview.planId,
        sourceTable: table.sourceTable,
        targetTable: table.targetTable,
        cursor,
        limit: table.pageSize ?? preview.pageSize,
      },
    });
    for (const row of page.rows) {
      if (row.operation === 'UNCHANGED' || (operation && row.operation !== operation)) continue;
      rows.push({
        sourceTable: table.sourceTable,
        targetTable: table.targetTable,
        operation: row.operation,
        key: row.key,
      });
    }
    cursor = page.nextCursor;
  }
  if (rows.length === 0)
    throw new Error(`comparison rows ${tableName}/${operation ?? 'changes'} are missing`);
  return {
    revision: preview.selectionRevision,
    rows,
  };
}

async function expectCommandError(invoke: () => Promise<unknown>): Promise<string> {
  let message = '';
  try {
    const result = await invoke();
    if (result && typeof result === 'object' && 'error' in result) {
      message = String(result.error ?? '');
    }
  } catch (e) {
    message = e instanceof Error ? e.message : String(e);
  }
  expect(message).not.toBe('');
  return message;
}

// ── Live connection IDs (filled by before hook) ─────────────────────

let srcSessionId: string;
let tgtSessionId: string;
let roSessionId: string;
let myTgtSessionId: string;
let myRoSessionId: string;

// ═════════════════════════════════════════════════════════════════════
// Group 1: PostgreSQL → PostgreSQL (Happy Path)
// ═════════════════════════════════════════════════════════════════════

describe('数据同步: PG→PG 基础功能 (SYNC-REAL)', () => {
  before(async () => {
    await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({
      timeout: 10000,
    });
    await browser.pause(500);

    // Save connection configs and connect
    srcSessionId = await saveAndConnect(PG_SRC);
    tgtSessionId = await saveAndConnect(PG_TGT);

    // Clean slate: drop any leftover test tables
    const cleanSQL = `
      DROP TABLE IF EXISTS sync_users;
      DROP TABLE IF EXISTS sync_products;
      DROP TABLE IF EXISTS sync_tgt_only;
      DROP TABLE IF EXISTS sync_simple;
      DROP TABLE IF EXISTS sync_pg_types;
      DROP TABLE IF EXISTS sync_apply_exec;
      DROP TABLE IF EXISTS sync_stream_large;
      DROP TABLE IF EXISTS sync_stream_rollback;
    `;
    await runSQL(srcSessionId, cleanSQL);
    await runSQL(tgtSessionId, cleanSQL);
  });

  after(async () => {
    // Clean up test tables
    const cleanSQL = `
      DROP TABLE IF EXISTS sync_users;
      DROP TABLE IF EXISTS sync_products;
      DROP TABLE IF EXISTS sync_tgt_only;
      DROP TABLE IF EXISTS sync_simple;
      DROP TABLE IF EXISTS sync_pg_types;
      DROP TABLE IF EXISTS sync_apply_exec;
      DROP TABLE IF EXISTS sync_stream_large;
      DROP TABLE IF EXISTS sync_stream_rollback;
    `;
    try {
      await runSQL(srcSessionId, cleanSQL);
    } catch {
      /* ok */
    }
    try {
      await runSQL(tgtSessionId, cleanSQL);
    } catch {
      /* ok */
    }

    // Delete test connections
    for (const cfg of ALL_CONFIGS) {
      try {
        await invokeBackend('delete_connection', { id: cfg.id });
      } catch {
        /* ok */
      }
    }
    try {
      if (srcSessionId) {
        await disconnectBackend(srcSessionId);
        srcSessionId = '';
      }
    } catch {
      /* ok */
    }
    try {
      if (tgtSessionId) {
        await disconnectBackend(tgtSessionId);
        tgtSessionId = '';
      }
    } catch {
      /* ok */
    }
  });

  it('SYNC-REAL-001: inspect — source has table, target is empty → UNMAPPED_SOURCE', async () => {
    await runSQL(
      srcSessionId,
      `
      CREATE TABLE sync_users (
        id integer NOT NULL,
        name varchar(100) NOT NULL,
        email text,
        PRIMARY KEY (id)
      );
      INSERT INTO sync_users (id, name, email) VALUES
        (1, 'Alice', 'alice@example.com'),
        (2, 'Bob', 'bob@example.com'),
        (3, 'Charlie', 'charlie@example.com');
    `,
    );

    const results = await invokeBackend<InspectResult[]>('inspect_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
    });

    const users = results.find((r) => r.sourceTable === 'sync_users');
    expect(users).toBeDefined();
    expect(users!.status).toBe('UNMAPPED_SOURCE');
  });

  it('SYNC-REAL-002: legacy sync_table IPC is removed', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceDbSessionId: srcSessionId,
        targetDbSessionId: tgtSessionId,
        tableName: 'sync_users',
      }),
    );
  });

  it('SYNC-REAL-003: inspect without sync — table remains UNMAPPED_SOURCE', async () => {
    const results = await invokeBackend<InspectResult[]>('inspect_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
    });

    const users = results.find((r) => r.sourceTable === 'sync_users');
    expect(users).toBeDefined();
    expect(users!.status).toBe('UNMAPPED_SOURCE');
  });

  it('SYNC-REAL-004: compare_data_sync — same schema different rows reports INSERT rows', async () => {
    await runSQL(
      tgtSessionId,
      `
      CREATE TABLE sync_users (
        id integer NOT NULL,
        name varchar(100) NOT NULL,
        email text,
        PRIMARY KEY (id)
      );
      INSERT INTO sync_users (id, name, email) VALUES
        (1, 'Alice', 'alice@example.com'),
        (2, 'Bob', 'bob@example.com'),
        (3, 'Charlie', 'charlie@example.com');
    `,
    );
    await runSQL(
      srcSessionId,
      `
      INSERT INTO sync_users (id, name, email) VALUES
        (4, 'Dave', 'dave@example.com'),
        (5, 'Eve', 'eve@example.com');
    `,
    );

    const preview = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_users'],
    });

    const users = preview.tables.find((r) => r.sourceTable === 'sync_users');
    expect(users).toBeDefined();
    expect(users!.status).toBe('MATCHED');
    expect(
      (await comparisonRows(preview, 'sync_users')).some((r) => r.operation === 'INSERT'),
    ).toBe(true);
  });

  it('SYNC-REAL-005: inspect — different schemas → INCOMPATIBLE', async () => {
    // Source: 3 columns
    await runSQL(
      srcSessionId,
      `
      CREATE TABLE sync_products (
        id integer NOT NULL,
        name text NOT NULL,
        price numeric(10,2),
        PRIMARY KEY (id)
      );
      INSERT INTO sync_products (id, name, price) VALUES (1, 'Widget', 9.99);
    `,
    );

    // Target: 2 columns (missing price)
    await runSQL(
      tgtSessionId,
      `
      CREATE TABLE sync_products (
        id integer NOT NULL,
        name text NOT NULL,
        PRIMARY KEY (id)
      );
      INSERT INTO sync_products (id, name) VALUES (1, 'Widget');
    `,
    );

    const results = await invokeBackend<InspectResult[]>('inspect_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
    });

    const products = results.find((r) => r.sourceTable === 'sync_products');
    expect(products).toBeDefined();
    expect(products!.status).toBe('INCOMPATIBLE');
  });

  it('SYNC-REAL-006: inspect — UNMAPPED_TARGET table', async () => {
    await runSQL(tgtSessionId, 'CREATE TABLE sync_tgt_only (id int);');

    const results = await invokeBackend<InspectResult[]>('inspect_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
    });

    const tgtOnly = results.find((r) => r.targetTable === 'sync_tgt_only');
    expect(tgtOnly).toBeDefined();
    expect(tgtOnly!.status).toBe('UNMAPPED_TARGET');
  });

  it('SYNC-REAL-007: legacy sync_table IPC is removed for mismatched schema', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceDbSessionId: srcSessionId,
        targetDbSessionId: tgtSessionId,
        tableName: 'sync_products',
      }),
    );
  });

  it('SYNC-REAL-008: generate_data_sync_sql — INSERT diff yields parameterized statements', async () => {
    await runSQL(
      srcSessionId,
      `
      DROP TABLE IF EXISTS sync_apply_exec;
      CREATE TABLE sync_apply_exec (
        id integer NOT NULL,
        val text NOT NULL,
        PRIMARY KEY (id)
      );
      INSERT INTO sync_apply_exec (id, val) VALUES (1, 'a'), (2, 'b'), (3, 'c');
    `,
    );
    await runSQL(
      tgtSessionId,
      `
      DROP TABLE IF EXISTS sync_apply_exec;
      CREATE TABLE sync_apply_exec (
        id integer NOT NULL,
        val text NOT NULL,
        PRIMARY KEY (id)
      );
      INSERT INTO sync_apply_exec (id, val) VALUES (1, 'a'), (2, 'b');
    `,
    );

    const preview = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_apply_exec'],
      options: SYNC_EXEC_OPTIONS,
    });
    const table = preview.tables.find((r) => r.sourceTable === 'sync_apply_exec');
    expect(table).toBeDefined();
    expect(
      (await comparisonRows(preview, 'sync_apply_exec')).some((r) => r.operation === 'INSERT'),
    ).toBe(true);
    const selection = await selectionFor(preview, 'sync_apply_exec', 'INSERT');

    const stmts = await invokeBackend<SqlStatement[]>('generate_data_sync_sql', {
      planId: preview.planId,
      selection,
      options: SYNC_EXEC_OPTIONS,
    });
    expect(stmts.length).toBeGreaterThan(0);
    expect(stmts.some((s) => s.operation === 'INSERT')).toBe(true);
    expect(stmts[0]!.sql.toUpperCase()).toContain('INSERT');
  });

  it('SYNC-REAL-009: apply_data_sync → recompare — pending INSERTs cleared', async () => {
    const revalidate = await invokeBackend<{ ok: boolean; staleTables: unknown[] }>(
      'revalidate_data_sync',
      {
        sourceDbSessionId: srcSessionId,
        targetDbSessionId: tgtSessionId,
        tables: ['sync_apply_exec'],
      },
    );
    expect(revalidate.ok).toBe(true);
    expect(revalidate.staleTables.length).toBe(0);

    const legacyError = await expectCommandError(() =>
      invokeBackend('apply_data_sync', {
        sourceDbSessionId: srcSessionId,
        targetDbSessionId: tgtSessionId,
        tables: ['sync_apply_exec'],
        options: SYNC_EXEC_OPTIONS,
      }),
    );
    expect(legacyError).toMatch(/reviewed (?:row selection|plan)|comparison plan/i);

    const preview = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_apply_exec'],
      options: SYNC_EXEC_OPTIONS,
    });
    const selection = await selectionFor(preview, 'sync_apply_exec', 'INSERT');
    const applyResult = await invokeBackend<ExecutionResult>('execute_data_sync', {
      request: {
        planId: preview.planId,
        selection,
        options: SYNC_EXEC_OPTIONS,
        jobId: null,
      },
    });
    expect(applyResult.applied).toBeGreaterThan(0);
    expect(applyResult.rolledBack).toBe(false);

    const replay = await invokeBackend<ExecutionResult>('execute_data_sync', {
      request: { planId: preview.planId, selection, options: SYNC_EXEC_OPTIONS, jobId: null },
    });
    expect(replay.applied).toBe(applyResult.applied);
    expect(replay.outcome).toBe('committed');

    const after = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_apply_exec'],
      options: SYNC_EXEC_OPTIONS,
    });
    const table = after.tables.find((r) => r.sourceTable === 'sync_apply_exec');
    expect(table).toBeDefined();
    const pending = (await comparisonRows(after, 'sync_apply_exec')).filter(
      (r) => r.operation === 'INSERT' || r.operation === 'UPDATE' || r.operation === 'DELETE',
    );
    expect(pending.length).toBe(0);
  });

  it('SYNC-REAL-025: first-batch conflict rolls back and permits a fresh comparison', async () => {
    await runSQL(srcSessionId, "UPDATE sync_apply_exec SET val = 'source-update' WHERE id = 1");
    const preview = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_apply_exec'],
      options: SYNC_EXEC_OPTIONS,
    });
    const selection = await selectionFor(preview, 'sync_apply_exec', 'UPDATE');

    await runSQL(tgtSessionId, "UPDATE sync_apply_exec SET val = 'target-concurrent' WHERE id = 1");
    const result = await invokeBackend<ExecutionResult>('execute_data_sync', {
      request: { planId: preview.planId, selection, options: SYNC_EXEC_OPTIONS, jobId: null },
    });
    expect(result.outcome).toBe('rolled_back');
    expect(result.rolledBack).toBe(true);
    expect(result.applied).toBe(0);

    const target = await invokeBackend<{ results: { rows: unknown[][] }[] }>('execute_query', {
      dbSessionId: tgtSessionId,
      sql: 'SELECT id, val FROM sync_apply_exec WHERE id = 1',
    });
    expect(target.results[0]?.rows).toEqual([[1, 'target-concurrent']]);

    const refreshed = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_apply_exec'],
      options: SYNC_EXEC_OPTIONS,
    });
    expect(refreshed.planId).not.toBe(preview.planId);
    expect(
      (await comparisonRows(refreshed, 'sync_apply_exec')).some(
        (row) => row.operation === 'UPDATE' && row.key[0] === 1,
      ),
    ).toBe(true);
  });

  it('SYNC-REAL-026: streams and applies a plan larger than the former 64 MiB load limit', async function () {
    this.timeout(300000);
    const valueBytes = 14 * 1024;
    const expectedRows = 5000;
    const options = { ...SYNC_EXEC_OPTIONS, batchSize: 500 };
    await runSQL(
      srcSessionId,
      `CREATE TABLE sync_stream_large (id integer PRIMARY KEY, val text NOT NULL);
       INSERT INTO sync_stream_large (id, val)
       SELECT id, repeat('x', ${valueBytes}) FROM generate_series(1, ${expectedRows}) AS gs(id);`,
    );
    await runSQL(
      tgtSessionId,
      'CREATE TABLE sync_stream_large (id integer PRIMARY KEY, val text NOT NULL);',
    );

    const preview = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_stream_large'],
      options,
    });
    const selection = await selectionFor(preview, 'sync_stream_large', 'INSERT');
    const previewError = await expectCommandError(() =>
      invokeBackend('generate_data_sync_sql', {
        planId: preview.planId,
        selection,
        options,
      }),
    );
    expect(previewError).toMatch(/16 MiB IPC limit/);

    const result = await invokeBackend<ExecutionResult>('execute_data_sync', {
      request: {
        planId: preview.planId,
        selection,
        options,
        jobId: null,
      },
    });
    expect(result.applied).toBe(expectedRows);
    expect(result.rolledBack).toBe(false);

    const count = await invokeBackend<{ results: { rows: unknown[][] }[] }>('execute_query', {
      dbSessionId: tgtSessionId,
      sql: 'SELECT count(*) FROM sync_stream_large',
    });
    expect(count.results[0]?.rows).toEqual([[expectedRows]]);

    const readback = await invokeBackend<{ results: { rows: unknown[][] }[] }>('execute_query', {
      dbSessionId: tgtSessionId,
      sql: 'SELECT id, length(val) FROM sync_stream_large WHERE id IN (1, 2500, 5000) ORDER BY id',
    });
    expect(readback.results[0]?.rows).toEqual([
      [1, valueBytes],
      [2500, valueBytes],
      [5000, valueBytes],
    ]);
  });

  it('SYNC-REAL-027: later-batch conflict preserves and reports exactly the committed first batch', async function () {
    this.timeout(180000);
    await runSQL(
      srcSessionId,
      `CREATE TABLE sync_stream_rollback (id integer PRIMARY KEY, val text NOT NULL);
       INSERT INTO sync_stream_rollback (id, val)
       SELECT id, CASE WHEN id = 500 THEN 'source-before' ELSE 'source-' || id END
       FROM generate_series(0, 500) AS gs(id);`,
    );
    await runSQL(
      tgtSessionId,
      `CREATE TABLE sync_stream_rollback (id integer PRIMARY KEY, val text NOT NULL);
       INSERT INTO sync_stream_rollback VALUES (500, 'target-before');`,
    );
    const preview = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_stream_rollback'],
      options: SYNC_EXEC_OPTIONS,
    });
    const selection = await selectionFor(preview, 'sync_stream_rollback');
    await runSQL(
      tgtSessionId,
      "UPDATE sync_stream_rollback SET val = 'target-concurrent' WHERE id = 500;",
    );

    const result = await invokeBackend<ExecutionResult>('execute_data_sync', {
      request: { planId: preview.planId, selection, options: SYNC_EXEC_OPTIONS, jobId: null },
    });
    expect(result.outcome).toBe('partially_applied');
    expect(result.applied).toBe(500);
    expect(result.rolledBack).toBe(false);
    expect(result.error).toMatch(/batches committed/i);

    const readback = await invokeBackend<{ results: { rows: unknown[][] }[] }>('execute_query', {
      dbSessionId: tgtSessionId,
      sql: 'SELECT id, val FROM sync_stream_rollback ORDER BY id',
    });
    expect(readback.results[0]?.rows).toEqual([
      ...Array.from({ length: 500 }, (_, id) => [id, `source-${id}`]),
      [500, 'target-concurrent'],
    ]);
  });

  it('SYNC-REAL-010: stale target schema rejects before write', async () => {
    await runSQL(
      srcSessionId,
      "UPDATE sync_apply_exec SET val = 'changed-after-review' WHERE id = 1",
    );
    const preview = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_apply_exec'],
      options: SYNC_EXEC_OPTIONS,
    });
    const selection = await selectionFor(preview, 'sync_apply_exec', 'UPDATE');
    const before = await invokeBackend<{ results: { rows: unknown[][] }[] }>('execute_query', {
      dbSessionId: tgtSessionId,
      sql: 'SELECT id, val FROM sync_apply_exec ORDER BY id',
    });
    await runSQL(tgtSessionId, 'ALTER TABLE sync_apply_exec ADD COLUMN stale_guard INT NULL');

    const error = await expectCommandError(() =>
      invokeBackend('execute_data_sync', {
        request: {
          planId: preview.planId,
          selection,
          options: SYNC_EXEC_OPTIONS,
          jobId: null,
        },
      }),
    );
    expect(error).toMatch(/did not start/i);

    const after = await invokeBackend<{ results: { rows: unknown[][] }[] }>('execute_query', {
      dbSessionId: tgtSessionId,
      sql: 'SELECT id, val FROM sync_apply_exec ORDER BY id',
    });
    expect(after.results[0].rows).toEqual(before.results[0].rows);
  });

  it('SYNC-REAL-024: compare + apply on PG wide-type table (numeric, bool, double, uuid, timestamptz)', async () => {
    await runSQL(
      srcSessionId,
      `
      DROP TABLE IF EXISTS sync_pg_types;
      CREATE TABLE sync_pg_types (
        id integer NOT NULL PRIMARY KEY,
        name text NOT NULL,
        price numeric(10,2),
        ratio double precision,
        is_active boolean NOT NULL DEFAULT true,
        note varchar(200),
        uid uuid NOT NULL,
        created_at timestamptz NOT NULL
      );
      INSERT INTO sync_pg_types (id, name, price, ratio, is_active, note, uid, created_at) VALUES
        (1, 'Alpha', 19.99, 3.14, true, 'first', '550e8400-e29b-41d4-a716-446655440000', '2024-01-15 10:30:00+00'),
        (2, 'Beta', 29.50, 2.71, false, 'second', '6ba7b810-9dad-11d1-80b4-00c04fd430c8', '2024-02-20 14:00:00+00');
    `,
    );
    await runSQL(
      tgtSessionId,
      `
      DROP TABLE IF EXISTS sync_pg_types;
      CREATE TABLE sync_pg_types (
        id integer NOT NULL PRIMARY KEY,
        name text NOT NULL,
        price numeric(10,2),
        ratio double precision,
        is_active boolean NOT NULL DEFAULT true,
        note varchar(200),
        uid uuid NOT NULL,
        created_at timestamptz NOT NULL
      );
      INSERT INTO sync_pg_types (id, name, price, ratio, is_active, note, uid, created_at) VALUES
        (1, 'Alpha', 19.99, 3.14, true, 'first', '550e8400-e29b-41d4-a716-446655440000', '2024-01-15 10:30:00+00');
    `,
    );

    const preview = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_pg_types'],
      options: SYNC_EXEC_OPTIONS,
    });
    const table = preview.tables.find((r) => r.sourceTable === 'sync_pg_types');
    expect(table).toBeDefined();
    expect(
      (await comparisonRows(preview, 'sync_pg_types')).some((r) => r.operation === 'INSERT'),
    ).toBe(true);
    const selection = await selectionFor(preview, 'sync_pg_types', 'INSERT');

    const applyResult = await invokeBackend<ExecutionResult>('execute_data_sync', {
      request: {
        planId: preview.planId,
        selection,
        options: SYNC_EXEC_OPTIONS,
        jobId: null,
      },
    });
    expect(applyResult.applied).toBeGreaterThan(0);
    expect(applyResult.rolledBack).toBe(false);

    const after = await invokeBackend<CompareDataSyncPreview>('compare_data_sync', {
      sourceDbSessionId: srcSessionId,
      targetDbSessionId: tgtSessionId,
      tables: ['sync_pg_types'],
      options: SYNC_EXEC_OPTIONS,
    });
    const synced = after.tables.find((r) => r.sourceTable === 'sync_pg_types');
    expect(synced).toBeDefined();
    const pending = (await comparisonRows(after, 'sync_pg_types')).filter(
      (r) => r.operation === 'INSERT' || r.operation === 'UPDATE' || r.operation === 'DELETE',
    );
    expect(pending.length).toBe(0);
  });
});

// ═════════════════════════════════════════════════════════════════════
// Group 2: Permission Errors
// ═════════════════════════════════════════════════════════════════════

describe('数据同步: 权限错误 (SYNC-PERM)', () => {
  before(async () => {
    await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({
      timeout: 10000,
    });
    await browser.pause(500);

    // Ensure source connection is ready with a table to sync
    if (!srcSessionId) srcSessionId = await saveAndConnect(PG_SRC);
    if (!tgtSessionId) tgtSessionId = await saveAndConnect(PG_TGT);

    // Ensure sync_users exists in source
    try {
      await runSQL(
        srcSessionId,
        `
        CREATE TABLE IF NOT EXISTS sync_users (
          id integer NOT NULL,
          name varchar(100) NOT NULL,
          email text,
          PRIMARY KEY (id)
        );
        INSERT INTO sync_users (id, name, email)
          SELECT 1, 'Test', 'test@test.com'
          WHERE NOT EXISTS (SELECT 1 FROM sync_users LIMIT 1);
      `,
      );
    } catch {
      /* may already exist */
    }

    // Connect readonly users
    roSessionId = await saveAndConnect(PG_RO);
    myRoSessionId = await saveAndConnect(MY_RO);
  });

  after(async () => {
    try {
      if (roSessionId) {
        await disconnectBackend(roSessionId);
        roSessionId = '';
      }
    } catch {
      /* ok */
    }
    try {
      if (myRoSessionId) {
        await disconnectBackend(myRoSessionId);
        myRoSessionId = '';
      }
    } catch {
      /* ok */
    }
  });

  it('SYNC-REAL-010: legacy sync_table IPC is removed for PG read-only target', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceDbSessionId: srcSessionId,
        targetDbSessionId: roSessionId,
        tableName: 'sync_users',
      }),
    );
  });

  it('SYNC-REAL-011: legacy sync_table IPC is removed for MySQL read-only target', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceDbSessionId: srcSessionId,
        targetDbSessionId: myRoSessionId,
        tableName: 'sync_users',
      }),
    );
  });
});

// ═════════════════════════════════════════════════════════════════════
// Group 3: Cross-Database Type (PG → MySQL)
// ═════════════════════════════════════════════════════════════════════

describe('数据同步: PG→MySQL 跨库 (SYNC-CROSS)', () => {
  before(async () => {
    await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({
      timeout: 10000,
    });
    await browser.pause(500);

    if (!srcSessionId) srcSessionId = await saveAndConnect(PG_SRC);
    myTgtSessionId = await saveAndConnect(MY_TGT);

    // Clean MySQL target
    try {
      await runSQL(
        myTgtSessionId,
        `
        DROP TABLE IF EXISTS sync_users;
        DROP TABLE IF EXISTS sync_simple;
        DROP TABLE IF EXISTS sync_diverse;
        DROP TABLE IF EXISTS sync_pg_arrays;
      `,
      );
    } catch {
      /* ok */
    }
  });

  after(async () => {
    try {
      await runSQL(
        myTgtSessionId,
        `
        DROP TABLE IF EXISTS sync_users;
        DROP TABLE IF EXISTS sync_simple;
        DROP TABLE IF EXISTS sync_diverse;
        DROP TABLE IF EXISTS sync_pg_arrays;
      `,
      );
    } catch {
      /* ok */
    }
    // Clean PG source test tables
    try {
      await runSQL(
        srcSessionId,
        `
        DROP TABLE IF EXISTS sync_simple;
        DROP TABLE IF EXISTS sync_diverse;
        DROP TABLE IF EXISTS sync_pg_arrays;
      `,
      );
    } catch {
      /* ok */
    }
    try {
      if (srcSessionId) {
        await disconnectBackend(srcSessionId);
        srcSessionId = '';
      }
    } catch {
      /* ok */
    }
    try {
      if (myTgtSessionId) {
        await disconnectBackend(myTgtSessionId);
        myTgtSessionId = '';
      }
    } catch {
      /* ok */
    }
  });

  it('SYNC-REAL-020: classify_data_sync_pair IPC classifies heterogeneous SQL as Transfer', async () => {
    const view = await invokeBackend<{
      path: string;
      supported: boolean;
      reason?: string;
    }>('classify_data_sync_pair', {
      sourceDatabaseType: 'postgresql',
      targetDatabaseType: 'mysql',
    });
    expect(view.path).toBe('ir');
    expect(view.supported).toBe(false);
    expect(view.reason ?? '').toMatch(/Transfer/i);
  });

  it('SYNC-REAL-021: legacy sync_table IPC is removed for PG→MySQL', async () => {
    try {
      await runSQL(srcSessionId, 'DROP TABLE IF EXISTS sync_simple;');
    } catch {
      /* ok */
    }
    await runSQL(
      srcSessionId,
      `
      CREATE TABLE sync_simple (
        id integer NOT NULL,
        name varchar(100),
        active boolean,
        PRIMARY KEY (id)
      );
      INSERT INTO sync_simple (id, name, active) VALUES
        (1, 'Alpha', true),
        (2, 'Beta', false);
    `,
    );

    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceDbSessionId: srcSessionId,
        targetDbSessionId: myTgtSessionId,
        tableName: 'sync_simple',
      }),
    );
  });

  it('SYNC-REAL-022: legacy sync_table IPC is removed for diverse types PG→MySQL', async () => {
    try {
      await runSQL(srcSessionId, 'DROP TABLE IF EXISTS sync_diverse;');
    } catch {
      /* ok */
    }
    await runSQL(
      srcSessionId,
      `
      CREATE TABLE sync_diverse (
        id integer NOT NULL PRIMARY KEY,
        name text NOT NULL,
        price numeric(10,2),
        ratio double precision,
        is_active boolean,
        created_at timestamp with time zone DEFAULT now(),
        uid uuid,
        note varchar(200)
      );
      INSERT INTO sync_diverse (id, name, price, ratio, is_active, uid, note) VALUES
        (1, 'Widget', 19.99, 3.14, true, 'a0eebc99-9c0b-4ef8-bb6d-6bb9bd380a11', 'first item');
    `,
    );

    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceDbSessionId: srcSessionId,
        targetDbSessionId: myTgtSessionId,
        tableName: 'sync_diverse',
      }),
    );
  });

  it('SYNC-REAL-023: legacy sync_table IPC is removed for PG array type PG→MySQL', async () => {
    try {
      await runSQL(srcSessionId, 'DROP TABLE IF EXISTS sync_pg_arrays;');
    } catch {
      /* ok */
    }
    await runSQL(
      srcSessionId,
      `
      CREATE TABLE sync_pg_arrays (
        id integer NOT NULL PRIMARY KEY,
        tags text[]
      );
      INSERT INTO sync_pg_arrays (id, tags) VALUES (1, ARRAY['a','b']);
    `,
    );

    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceDbSessionId: srcSessionId,
        targetDbSessionId: myTgtSessionId,
        tableName: 'sync_pg_arrays',
      }),
    );
  });
});

// ═════════════════════════════════════════════════════════════════════
// Group 4: Batch sync task persistence (legacy sync_tables removed)
// ═════════════════════════════════════════════════════════════════════

describe('数据同步: 批量同步与进度 (SYNC-BATCH)', () => {
  let batchSrcId: string;
  let batchTgtId: string;

  before(async () => {
    await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({
      timeout: 10000,
    });
    await browser.pause(500);

    batchSrcId = await saveAndConnect(PG_SRC);
    batchTgtId = await saveAndConnect(PG_TGT);

    // Clean and create test tables
    const cleanSQL = `
      DROP TABLE IF EXISTS sync_batch_a;
      DROP TABLE IF EXISTS sync_batch_b;
      DROP TABLE IF EXISTS sync_batch_c;
    `;
    await runSQL(batchSrcId, cleanSQL);
    await runSQL(batchTgtId, cleanSQL);

    await runSQL(
      batchSrcId,
      `
      CREATE TABLE sync_batch_a (id int PRIMARY KEY, val text);
      INSERT INTO sync_batch_a VALUES (1, 'a1'), (2, 'a2'), (3, 'a3');

      CREATE TABLE sync_batch_b (id int PRIMARY KEY, val text);
      INSERT INTO sync_batch_b VALUES (1, 'b1'), (2, 'b2');

      CREATE TABLE sync_batch_c (id int PRIMARY KEY, val text);
      INSERT INTO sync_batch_c VALUES (1, 'c1');
    `,
    );

    await runSQL(
      batchTgtId,
      `
      CREATE TABLE sync_batch_a (id int PRIMARY KEY, val text);
      CREATE TABLE sync_batch_b (id int PRIMARY KEY, val text);
      CREATE TABLE sync_batch_c (id int PRIMARY KEY, val text);
    `,
    );
  });

  after(async () => {
    const cleanSQL = `
      DROP TABLE IF EXISTS sync_batch_a;
      DROP TABLE IF EXISTS sync_batch_b;
      DROP TABLE IF EXISTS sync_batch_c;
    `;
    try {
      await runSQL(batchSrcId, cleanSQL);
    } catch {
      /* ok */
    }
    try {
      await runSQL(batchTgtId, cleanSQL);
    } catch {
      /* ok */
    }
    // Clean up sync tasks
    try {
      const tasks = await invokeBackend<SyncTask[]>('get_sync_tasks');
      for (const t of tasks) {
        if (t.id.startsWith('test-')) {
          await invokeBackend('delete_sync_task', { taskId: t.id });
        }
      }
    } catch {
      /* ok */
    }
    try {
      if (batchSrcId) {
        await disconnectBackend(batchSrcId);
        batchSrcId = '';
      }
    } catch {
      /* ok */
    }
    try {
      if (batchTgtId) {
        await disconnectBackend(batchTgtId);
        batchTgtId = '';
      }
    } catch {
      /* ok */
    }
  });

  it('SYNC-INSPECT-001: inspect_data_sync maps same-family tables', async () => {
    const results = await invokeBackend<
      Array<{ sourceTable: string; targetTable: string; status: string }>
    >('inspect_data_sync', {
      sourceDbSessionId: batchSrcId,
      targetDbSessionId: batchTgtId,
    });
    const names = results.map((r) => r.sourceTable);
    expect(names).toContain('sync_batch_a');
    expect(results.find((r) => r.sourceTable === 'sync_batch_a')?.status).toBe('MATCHED');
  });

  it('SYNC-BATCH-001: legacy sync_tables IPC is removed', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_tables', {
        taskId: 'test-batch-001',
        sourceDbSessionId: batchSrcId,
        targetDbSessionId: batchTgtId,
        tables: ['sync_batch_a', 'sync_batch_b', 'sync_batch_c'],
        skipTables: [],
        strategy: 'full',
      }),
    );
  });

  it('SYNC-BATCH-002: classify_data_sync_pair IPC marks same-family mysql as supported', async () => {
    const view = await invokeBackend<{
      path: string;
      supported: boolean;
      family?: string;
    }>('classify_data_sync_pair', {
      sourceDatabaseType: 'mysql',
      targetDatabaseType: 'mariadb',
    });
    expect(view.path).toBe('direct');
    expect(view.supported).toBe(true);
    expect(view.family).toBe('mysql');
  });

  it('SYNC-BATCH-003: legacy sync_table IPC is removed', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceDbSessionId: batchSrcId,
        targetDbSessionId: batchTgtId,
        tableName: 'sync_batch_a',
      }),
    );
  });

  it('SYNC-BATCH-004: sync task CRUD still works without overwrite copy', async () => {
    const pausedTask: SyncTask = {
      id: 'test-batch-004',
      sourceDbSessionId: batchSrcId,
      targetDbSessionId: batchTgtId,
      sourceConnectionId: PG_SRC.id,
      targetConnectionId: PG_TGT.id,
      tables: ['sync_batch_a'],
      completedTables: [],
      currentTable: null,
      currentTableOffset: 0,
      sourceRowCounts: {},
      strategy: 'full',
      status: 'paused',
      errorMessage: null,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };
    await invokeBackend('save_sync_task_direct', { task: pausedTask });
    const tasks = await invokeBackend<SyncTask[]>('get_sync_tasks');
    expect(tasks.find((t) => t.id === 'test-batch-004')).toBeDefined();
    await invokeBackend('delete_sync_task', { taskId: 'test-batch-004' });
    const after = await invokeBackend<SyncTask[]>('get_sync_tasks');
    expect(after.find((t) => t.id === 'test-batch-004')).toBeUndefined();
  });
});

// ═════════════════════════════════════════════════════════════════════
// Group 5: Checkpoint / Resume / Conflict Detection
// ═════════════════════════════════════════════════════════════════════

describe('数据同步: 断点续传与冲突检测 (SYNC-RESUME)', () => {
  let resumeSrcId: string;
  let resumeTgtId: string;

  before(async () => {
    await $(`input[placeholder="${t('main.searchPlaceholder')}"]`).waitForDisplayed({
      timeout: 10000,
    });
    await browser.pause(500);

    resumeSrcId = await saveAndConnect(PG_SRC);
    resumeTgtId = await saveAndConnect(PG_TGT);

    const cleanSQL = `
      DROP TABLE IF EXISTS sync_resume_a;
      DROP TABLE IF EXISTS sync_resume_b;
    `;
    await runSQL(resumeSrcId, cleanSQL);
    await runSQL(resumeTgtId, cleanSQL);

    await runSQL(
      resumeSrcId,
      `
      CREATE TABLE sync_resume_a (id int PRIMARY KEY, val text);
      INSERT INTO sync_resume_a VALUES (1, 'a1'), (2, 'a2');

      CREATE TABLE sync_resume_b (id int PRIMARY KEY, val text);
      INSERT INTO sync_resume_b VALUES (1, 'b1');
    `,
    );
  });

  after(async () => {
    const cleanSQL = `
      DROP TABLE IF EXISTS sync_resume_a;
      DROP TABLE IF EXISTS sync_resume_b;
    `;
    try {
      await runSQL(resumeSrcId, cleanSQL);
    } catch {
      /* ok */
    }
    try {
      await runSQL(resumeTgtId, cleanSQL);
    } catch {
      /* ok */
    }
    try {
      const tasks = await invokeBackend<SyncTask[]>('get_sync_tasks');
      for (const t of tasks) {
        if (t.id.startsWith('test-resume')) {
          await invokeBackend('delete_sync_task', { taskId: t.id });
        }
      }
    } catch {
      /* ok */
    }
    try {
      if (resumeSrcId) await disconnectBackend(resumeSrcId);
    } catch {
      /* ok */
    }
    try {
      if (resumeTgtId) await disconnectBackend(resumeTgtId);
    } catch {
      /* ok */
    }
  });

  it('SYNC-RESUME-001: paused task can be saved without overwrite copy', async () => {
    const pausedTask: SyncTask = {
      id: 'test-resume-conflict',
      sourceDbSessionId: resumeSrcId,
      targetDbSessionId: resumeTgtId,
      sourceConnectionId: PG_SRC.id,
      targetConnectionId: PG_TGT.id,
      tables: ['sync_resume_a', 'sync_resume_b'],
      completedTables: ['sync_resume_a'],
      currentTable: 'sync_resume_b',
      currentTableOffset: 0,
      sourceRowCounts: { sync_resume_a: 2, sync_resume_b: 1 },
      strategy: 'continue',
      status: 'paused',
      errorMessage: null,
      createdAt: new Date().toISOString(),
      updatedAt: new Date().toISOString(),
    };

    await invokeBackend('save_sync_task_direct', { task: pausedTask });

    // Check for conflicts: only non-completed tables are checked
    // sync_resume_a is completed → skipped
    // sync_resume_b has original=1, current=1 → no change → no conflict
    const conflicts = await invokeBackend<{
      hasConflicts: boolean;
      conflicts: Array<{ table: string; originalRows: number; currentRows: number }>;
    }>('check_sync_conflicts', { taskId: 'test-resume-conflict' });

    expect(conflicts.hasConflicts).toBe(false);
    expect(conflicts.conflicts.length).toBe(0);
  });

  it('SYNC-RESUME-002: check_sync_conflicts detects changes in remaining tables', async () => {
    // Now modify sync_resume_b in source (this is a non-completed table)
    await runSQL(resumeSrcId, `INSERT INTO sync_resume_b VALUES (2, 'b2'), (3, 'b3')`);

    const conflicts = await invokeBackend<{
      hasConflicts: boolean;
      conflicts: Array<{ table: string; originalRows: number; currentRows: number }>;
    }>('check_sync_conflicts', { taskId: 'test-resume-conflict' });

    expect(conflicts.hasConflicts).toBe(true);
    const conflictB = conflicts.conflicts.find((c) => c.table === 'sync_resume_b');
    expect(conflictB).toBeDefined();
    expect(conflictB!.originalRows).toBe(1);
    expect(conflictB!.currentRows).toBe(3);
  });

  it('SYNC-RESUME-003: legacy sync_tables IPC is removed', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_tables', {
        taskId: 'test-resume-skip',
        sourceDbSessionId: resumeSrcId,
        targetDbSessionId: resumeTgtId,
        tables: ['sync_resume_a', 'sync_resume_b'],
        skipTables: ['sync_resume_a'],
        strategy: 'continue',
      }),
    );
    await invokeBackend('delete_sync_task', { taskId: 'test-resume-conflict' });
  });
});

// Helper interface reused across test groups
interface SyncTask {
  id: string;
  sourceConnectionId: string;
  targetConnectionId: string;
  sourceDbSessionId: string;
  targetDbSessionId: string;
  tables: string[];
  completedTables: string[];
  currentTable: string | null;
  currentTableOffset: number;
  sourceRowCounts: Record<string, number>;
  strategy: string;
  status: string;
  errorMessage: string | null;
  createdAt: string;
  updatedAt: string;
}
