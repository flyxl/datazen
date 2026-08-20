/**
 * Host IPC contract E2E for Data Sync (`inspect_data_sync`, `compare_data_sync`, etc.).
 *
 * Uses live PostgreSQL / MySQL instances as fixtures but asserts Host-level IPC
 * behavior only — not driver-specific sync adapters. Driver dialect / adapter tests
 * belong in `packages/drivers/<id>/` (see AGENTS.md「驱动测试落点」).
 *
 * Prerequisites: run `e2e/setup-sync-dbs.sh` first to create test databases
 * and the restricted `datazen_readonly` user in both PG and MySQL.
 *
 * Tests call Tauri IPC commands directly (no UI interaction) for speed.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import { sqlBlockedBySafeMode, withSafeModeOff } from '../helpers.js';

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

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: any) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: any) => done(r))
        .catch((e: any) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as any)) {
    throw new Error((result as any).__error);
  }
  return result as T;
}

async function expectCommandNotFound(invoke: () => Promise<unknown>): Promise<void> {
  let message = '';
  try {
    await invoke();
  } catch (e) {
    message = e instanceof Error ? e.message : String(e);
  }
  expect(message.toLowerCase()).toMatch(/command .+ not found|unknown command/i);
}

async function saveAndConnect(cfg: typeof PG_SRC): Promise<string> {
  await invokeBackend('save_connection', { config: cfg });
  return invokeBackend<string>('connect', { configId: cfg.id });
}

async function runSQL(connectionId: string, sql: string): Promise<void> {
  const run = () => invokeBackend('execute_query', { connectionId, sql });
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
  rows?: Array<{ operation: string }>;
}

// ── Live connection IDs (filled by before hook) ─────────────────────

let srcConnId: string;
let tgtConnId: string;
let roConnId: string;
let myTgtConnId: string;
let myRoConnId: string;

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
    srcConnId = await saveAndConnect(PG_SRC);
    tgtConnId = await saveAndConnect(PG_TGT);

    // Clean slate: drop any leftover test tables
    const cleanSQL = `
      DROP TABLE IF EXISTS sync_users;
      DROP TABLE IF EXISTS sync_products;
      DROP TABLE IF EXISTS sync_tgt_only;
      DROP TABLE IF EXISTS sync_simple;
      DROP TABLE IF EXISTS sync_pg_types;
    `;
    await runSQL(srcConnId, cleanSQL);
    await runSQL(tgtConnId, cleanSQL);
  });

  after(async () => {
    // Clean up test tables
    const cleanSQL = `
      DROP TABLE IF EXISTS sync_users;
      DROP TABLE IF EXISTS sync_products;
      DROP TABLE IF EXISTS sync_tgt_only;
      DROP TABLE IF EXISTS sync_simple;
      DROP TABLE IF EXISTS sync_pg_types;
    `;
    try {
      await runSQL(srcConnId, cleanSQL);
    } catch {
      /* ok */
    }
    try {
      await runSQL(tgtConnId, cleanSQL);
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
  });

  it('SYNC-REAL-001: inspect — source has table, target is empty → UNMAPPED_SOURCE', async () => {
    await runSQL(
      srcConnId,
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
      sourceConnectionId: srcConnId,
      targetConnectionId: tgtConnId,
    });

    const users = results.find((r) => r.sourceTable === 'sync_users');
    expect(users).toBeDefined();
    expect(users!.status).toBe('UNMAPPED_SOURCE');
  });

  it('SYNC-REAL-002: legacy sync_table IPC is removed', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceConnectionId: srcConnId,
        targetConnectionId: tgtConnId,
        tableName: 'sync_users',
      }),
    );
  });

  it('SYNC-REAL-003: inspect without sync — table remains UNMAPPED_SOURCE', async () => {
    const results = await invokeBackend<InspectResult[]>('inspect_data_sync', {
      sourceConnectionId: srcConnId,
      targetConnectionId: tgtConnId,
    });

    const users = results.find((r) => r.sourceTable === 'sync_users');
    expect(users).toBeDefined();
    expect(users!.status).toBe('UNMAPPED_SOURCE');
  });

  it('SYNC-REAL-004: compare_data_sync — same schema different rows reports INSERT rows', async () => {
    await runSQL(
      tgtConnId,
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
      srcConnId,
      `
      INSERT INTO sync_users (id, name, email) VALUES
        (4, 'Dave', 'dave@example.com'),
        (5, 'Eve', 'eve@example.com');
    `,
    );

    const results = await invokeBackend<CompareDataSyncResult[]>('compare_data_sync', {
      sourceConnectionId: srcConnId,
      targetConnectionId: tgtConnId,
      tables: ['sync_users'],
    });

    const users = results.find((r) => r.sourceTable === 'sync_users');
    expect(users).toBeDefined();
    expect(users!.status).toBe('MATCHED');
    expect(users!.rows?.some((r) => r.operation === 'INSERT')).toBe(true);
  });

  it('SYNC-REAL-005: inspect — different schemas → INCOMPATIBLE', async () => {
    // Source: 3 columns
    await runSQL(
      srcConnId,
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
      tgtConnId,
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
      sourceConnectionId: srcConnId,
      targetConnectionId: tgtConnId,
    });

    const products = results.find((r) => r.sourceTable === 'sync_products');
    expect(products).toBeDefined();
    expect(products!.status).toBe('INCOMPATIBLE');
  });

  it('SYNC-REAL-006: inspect — UNMAPPED_TARGET table', async () => {
    await runSQL(tgtConnId, 'CREATE TABLE sync_tgt_only (id int);');

    const results = await invokeBackend<InspectResult[]>('inspect_data_sync', {
      sourceConnectionId: srcConnId,
      targetConnectionId: tgtConnId,
    });

    const tgtOnly = results.find((r) => r.targetTable === 'sync_tgt_only');
    expect(tgtOnly).toBeDefined();
    expect(tgtOnly!.status).toBe('UNMAPPED_TARGET');
  });

  it('SYNC-REAL-007: legacy sync_table IPC is removed for mismatched schema', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceConnectionId: srcConnId,
        targetConnectionId: tgtConnId,
        tableName: 'sync_products',
      }),
    );
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
    if (!srcConnId) srcConnId = await saveAndConnect(PG_SRC);
    if (!tgtConnId) tgtConnId = await saveAndConnect(PG_TGT);

    // Ensure sync_users exists in source
    try {
      await runSQL(
        srcConnId,
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
    roConnId = await saveAndConnect(PG_RO);
    myRoConnId = await saveAndConnect(MY_RO);
  });

  it('SYNC-REAL-010: legacy sync_table IPC is removed for PG read-only target', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceConnectionId: srcConnId,
        targetConnectionId: roConnId,
        tableName: 'sync_users',
      }),
    );
  });

  it('SYNC-REAL-011: legacy sync_table IPC is removed for MySQL read-only target', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceConnectionId: srcConnId,
        targetConnectionId: myRoConnId,
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

    if (!srcConnId) srcConnId = await saveAndConnect(PG_SRC);
    myTgtConnId = await saveAndConnect(MY_TGT);

    // Clean MySQL target
    try {
      await runSQL(
        myTgtConnId,
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
        myTgtConnId,
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
        srcConnId,
        `
        DROP TABLE IF EXISTS sync_simple;
        DROP TABLE IF EXISTS sync_diverse;
        DROP TABLE IF EXISTS sync_pg_arrays;
      `,
      );
    } catch {
      /* ok */
    }
  });

  it('SYNC-REAL-020: classify_sync_pair rejects PG→MySQL heterogeneous pair', async () => {
    const view = await invokeBackend<{ path: string; supported: boolean }>('classify_sync_pair', {
      sourceDatabaseType: 'postgresql',
      targetDatabaseType: 'mysql',
    });
    expect(view.path).toBe('ir');
    expect(view.supported).toBe(false);
  });

  it('SYNC-REAL-021: legacy sync_table IPC is removed for PG→MySQL', async () => {
    try {
      await runSQL(srcConnId, 'DROP TABLE IF EXISTS sync_simple;');
    } catch {
      /* ok */
    }
    await runSQL(
      srcConnId,
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
        sourceConnectionId: srcConnId,
        targetConnectionId: myTgtConnId,
        tableName: 'sync_simple',
      }),
    );
  });

  it('SYNC-REAL-022: legacy sync_table IPC is removed for diverse types PG→MySQL', async () => {
    try {
      await runSQL(srcConnId, 'DROP TABLE IF EXISTS sync_diverse;');
    } catch {
      /* ok */
    }
    await runSQL(
      srcConnId,
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
        sourceConnectionId: srcConnId,
        targetConnectionId: myTgtConnId,
        tableName: 'sync_diverse',
      }),
    );
  });

  it('SYNC-REAL-023: legacy sync_table IPC is removed for PG array type PG→MySQL', async () => {
    try {
      await runSQL(srcConnId, 'DROP TABLE IF EXISTS sync_pg_arrays;');
    } catch {
      /* ok */
    }
    await runSQL(
      srcConnId,
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
        sourceConnectionId: srcConnId,
        targetConnectionId: myTgtConnId,
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
  });

  it('SYNC-INSPECT-001: inspect_data_sync maps same-family tables', async () => {
    const results = await invokeBackend<
      Array<{ sourceTable: string; targetTable: string; status: string }>
    >('inspect_data_sync', {
      sourceConnectionId: batchSrcId,
      targetConnectionId: batchTgtId,
    });
    const names = results.map((r) => r.sourceTable);
    expect(names).toContain('sync_batch_a');
    expect(results.find((r) => r.sourceTable === 'sync_batch_a')?.status).toBe('MATCHED');
  });

  it('SYNC-BATCH-001: legacy sync_tables IPC is removed', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_tables', {
        taskId: 'test-batch-001',
        sourceConnectionId: batchSrcId,
        targetConnectionId: batchTgtId,
        sourceConfigId: PG_SRC.id,
        targetConfigId: PG_TGT.id,
        tables: ['sync_batch_a', 'sync_batch_b', 'sync_batch_c'],
        skipTables: [],
        strategy: 'full',
      }),
    );
  });

  it('SYNC-BATCH-002: classify_sync_pair rejects heterogeneous IR for Data Sync', async () => {
    const view = await invokeBackend<{ path: string; supported: boolean }>('classify_sync_pair', {
      sourceDatabaseType: 'postgresql',
      targetDatabaseType: 'mysql',
    });
    expect(view.path).toBe('ir');
    expect(view.supported).toBe(false);
  });

  it('SYNC-BATCH-003: legacy sync_table IPC is removed', async () => {
    await expectCommandNotFound(() =>
      invokeBackend('sync_table', {
        sourceConnectionId: batchSrcId,
        targetConnectionId: batchTgtId,
        tableName: 'sync_batch_a',
      }),
    );
  });

  it('SYNC-BATCH-004: sync task CRUD still works without overwrite copy', async () => {
    const pausedTask: SyncTask = {
      id: 'test-batch-004',
      sourceConnectionId: batchSrcId,
      targetConnectionId: batchTgtId,
      sourceConfigId: PG_SRC.id,
      targetConfigId: PG_TGT.id,
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
  });

  it('SYNC-RESUME-001: paused task can be saved without overwrite copy', async () => {
    const pausedTask: SyncTask = {
      id: 'test-resume-conflict',
      sourceConnectionId: resumeSrcId,
      targetConnectionId: resumeTgtId,
      sourceConfigId: PG_SRC.id,
      targetConfigId: PG_TGT.id,
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
        sourceConnectionId: resumeSrcId,
        targetConnectionId: resumeTgtId,
        sourceConfigId: PG_SRC.id,
        targetConfigId: PG_TGT.id,
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
  sourceConfigId: string;
  targetConfigId: string;
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
