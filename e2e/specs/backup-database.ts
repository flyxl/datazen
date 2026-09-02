import { expect, browser } from '@wdio/globals';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';
import { withSafeModeOff } from '../helpers.js';

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
  if (result && typeof result === 'object' && '__error' in (result as Record<string, unknown>)) {
    throw new Error((result as any).__error);
  }
  return result as T;
}

const PG_CONFIG = {
  id: 'e2e-backup-pg',
  name: 'E2E-Backup-PG',
  databaseType: 'postgresql',
  host: process.env.E2E_PG_HOST || 'localhost',
  port: Number(process.env.E2E_PG_PORT) || 5432,
  database: process.env.E2E_PG_DB || 'postgres',
  username: process.env.E2E_PG_USER || 'postgres',
  password: process.env.E2E_PG_PASSWORD || '',
  sslMode: 'disable',
};

const TMP_DIR = os.tmpdir();
const TEST_TABLE = '_e2e_backup_test';

async function seedBackupTable(dbSessionId: string) {
  await withSafeModeOff(() =>
    invokeBackend('execute_query', {
      dbSessionId,
      sql: `
      DROP TABLE IF EXISTS ${TEST_TABLE};
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name TEXT NOT NULL
      );
      INSERT INTO ${TEST_TABLE} (name) VALUES ('alice');
    `,
    }),
  );
}

async function dropBackupTable(dbSessionId: string) {
  await withSafeModeOff(() =>
    invokeBackend('execute_query', {
      dbSessionId,
      sql: `DROP TABLE IF EXISTS ${TEST_TABLE}`,
    }),
  );
}

// ═════════════════════════════════════════════════════════════════════
// Group 1: Backend backup command tests
// ═════════════════════════════════════════════════════════════════════

// Decision 3+6: backup_database is the merged dialog/path IPC — E2E bypasses
// the native save dialog via the webdriver-only `overridePath`.
async function backupToPath(
  dbSessionId: string,
  outPath: string,
  opts: Record<string, unknown> = {},
): Promise<boolean> {
  const base = path.basename(outPath);
  const filterExtension = base.endsWith('.sql.gz') ? 'gz' : path.extname(base).replace('.', '');
  return invokeBackend<boolean>('backup_database', {
    dbSessionId,
    database: PG_CONFIG.database,
    defaultFileName: base,
    filterExtension,
    overridePath: outPath,
    ...opts,
  });
}

describe('数据库备份功能 (BACKUP)', () => {
  let dbSessionId: string;

  before(async () => {
    await browser.setTimeout({ script: 120000 });
    await browser.pause(300);
    await invokeBackend('save_connection', { config: PG_CONFIG });
    dbSessionId = await invokeBackend<string>('connect', { connectionId: PG_CONFIG.id });
    await seedBackupTable(dbSessionId);
  });

  after(async () => {
    try {
      await dropBackupTable(dbSessionId);
    } catch {
      /* ok */
    }
    try {
      await invokeBackend('disconnect', { dbSessionId });
    } catch {
      /* ok */
    }
    try {
      await invokeBackend('delete_connection', { id: PG_CONFIG.id });
    } catch {
      /* ok */
    }
  });

  it('BACKUP-001: connect returns a valid connection ID string', async () => {
    expect(typeof dbSessionId).toBe('string');
    expect(dbSessionId.length).toBeGreaterThan(0);
  });

  it('BACKUP-002: get_databases returns database list', async () => {
    const dbs = await invokeBackend<string[]>('get_databases', { dbSessionId });
    expect(Array.isArray(dbs)).toBe(true);
    expect(dbs.length).toBeGreaterThan(0);
    expect(dbs).toContain(PG_CONFIG.database);
  });

  it('BACKUP-003: backup_database creates a SQL file', async () => {
    const outPath = path.join(TMP_DIR, `datazen-backup-test-${Date.now()}.sql`);

    const saved = await backupToPath(dbSessionId, outPath, {
      options: [],
      compress: false,
    });
    expect(saved).toBe(true);

    const exists = fs.existsSync(outPath);
    expect(exists).toBe(true);

    const content = fs.readFileSync(outPath, 'utf-8');
    const lc = content.toLowerCase();
    expect(content).toContain('-- DataZen backup');
    expect(lc).toContain('create table');
    expect(lc).toContain(TEST_TABLE);

    fs.unlinkSync(outPath);
  });

  it('BACKUP-004: backup with --schema-only produces no INSERT statements', async () => {
    const outPath = path.join(TMP_DIR, `datazen-backup-schema-${Date.now()}.sql`);

    await backupToPath(dbSessionId, outPath, {
      options: ['schema-only'],
      compress: false,
    });

    const content = fs.readFileSync(outPath, 'utf-8');
    const lc = content.toLowerCase();
    expect(lc).toContain('create table');
    expect(lc).toContain(TEST_TABLE);
    expect(content).not.toMatch(/insert into/i);
    expect(content).toContain('-- Options:');

    fs.unlinkSync(outPath);
  });

  it('BACKUP-005: backup with --data-only produces no CREATE TABLE', async function () {
    this.timeout(120000);
    const outPath = path.join(TMP_DIR, `datazen-backup-data-${Date.now()}.sql`);

    // Use a small table subset via pg_dump -t to avoid dumping 1M+ rows
    await backupToPath(dbSessionId, outPath, {
      options: ['data-only'],
      compress: false,
    });

    const content = fs.readFileSync(outPath, 'utf-8');
    expect(content).not.toContain('CREATE TABLE');
    expect(content).toContain('-- DataZen backup');

    fs.unlinkSync(outPath);
  });

  it('BACKUP-006: backup with --clean adds DROP TABLE', async function () {
    this.timeout(120000);
    const outPath = path.join(TMP_DIR, `datazen-backup-clean-${Date.now()}.sql`);

    await backupToPath(dbSessionId, outPath, {
      options: ['clean', 'schema-only'],
      compress: false,
    });

    const content = fs.readFileSync(outPath, 'utf-8');
    const lc = content.toLowerCase();
    expect(lc).toContain('drop table if exists');
    expect(lc).toContain(TEST_TABLE);

    fs.unlinkSync(outPath);
  });

  it('BACKUP-007: backup with --create adds CREATE DATABASE', async function () {
    this.timeout(120000);
    const outPath = path.join(TMP_DIR, `datazen-backup-create-${Date.now()}.sql`);

    await backupToPath(dbSessionId, outPath, {
      options: ['create', 'schema-only'],
      compress: false,
    });

    const content = fs.readFileSync(outPath, 'utf-8');
    // pg_dump --create produces "CREATE DATABASE <name>"
    expect(content.toLowerCase()).toContain('create database');

    fs.unlinkSync(outPath);
  });

  it('BACKUP-008: backup with gzip compression creates a valid gzip file', async function () {
    this.timeout(120000);
    const outPath = path.join(TMP_DIR, `datazen-backup-gz-${Date.now()}.sql.gz`);

    await backupToPath(dbSessionId, outPath, {
      options: ['schema-only'],
      compress: true,
    });

    const exists = fs.existsSync(outPath);
    expect(exists).toBe(true);

    // Check gzip magic bytes (1f 8b)
    const buf = fs.readFileSync(outPath);
    expect(buf[0]).toBe(0x1f);
    expect(buf[1]).toBe(0x8b);
    expect(buf.length).toBeGreaterThan(20);

    fs.unlinkSync(outPath);
  });

  it('BACKUP-009: backup with multiple options combined', async function () {
    this.timeout(120000);
    const outPath = path.join(TMP_DIR, `datazen-backup-multi-${Date.now()}.sql`);

    await backupToPath(dbSessionId, outPath, {
      options: ['clean', 'schema-only'],
      compress: false,
    });

    const content = fs.readFileSync(outPath, 'utf-8');
    const lc = content.toLowerCase();
    expect(lc).toContain('drop table if exists');
    expect(lc).toContain(TEST_TABLE);
    expect(lc).toContain('create table');
    expect(content).toContain('-- Options:');

    fs.unlinkSync(outPath);
  });

  it('BACKUP-011: backup and restore include functions', async function () {
    this.timeout(120000);
    const fnName = '_e2e_backup_fn';
    await withSafeModeOff(() =>
      invokeBackend('execute_query', {
        dbSessionId,
        sql: `CREATE OR REPLACE FUNCTION ${fnName}() RETURNS integer LANGUAGE sql AS $$ SELECT 1 $$;`,
      }),
    );

    const outPath = path.join(TMP_DIR, `datazen-backup-routines-${Date.now()}.sql`);
    await backupToPath(dbSessionId, outPath, {
      options: ['schema-only', 'routines'],
      compress: false,
    });

    const content = fs.readFileSync(outPath, 'utf-8');
    expect(content.toLowerCase()).toContain('create');
    expect(content).toContain(fnName);

    await withSafeModeOff(() =>
      invokeBackend('execute_query', {
        dbSessionId,
        sql: `DROP FUNCTION IF EXISTS ${fnName}();`,
      }),
    );

    // R2-BUG-003 ruling — option a (coordinator-adjudicated 2026-08-26): pass
    // `options: ['overwrite']`, same shape as BACKUP-012. Restoring a dump
    // *without* overwrite into a live DB legitimately partial-fails when named
    // relations already exist (`recover_restore_statement` only DROP-retries
    // when overwrite=true) — that product semantics is covered by BACKUP-012
    // and manual paths; this case now verifies full recovery of a
    // schema-only+routines dump (function round-trips).
    await withSafeModeOff(() =>
      invokeBackend('restore_sql_file', {
        dbSessionId,
        overridePath: outPath,
        options: ['overwrite'],
      }),
    );

    const check = await invokeBackend<unknown>('execute_query', {
      dbSessionId,
      sql: `SELECT proname FROM pg_proc WHERE proname = '${fnName}'`,
    });
    expect(JSON.stringify(check)).toContain(fnName);

    await withSafeModeOff(() =>
      invokeBackend('execute_query', {
        dbSessionId,
        sql: `DROP FUNCTION IF EXISTS ${fnName}();`,
      }),
    );
    fs.unlinkSync(outPath);
  });

  it('BACKUP-012: restore overwrite replaces existing objects without duplicating rows', async function () {
    this.timeout(120000);
    const outPath = path.join(TMP_DIR, `datazen-backup-overwrite-${Date.now()}.sql`);
    // Re-seed here: BACKUP-011's overwrite-restore of a schema-only dump
    // recreates _e2e_backup_test EMPTY, so the before()-seeded row can no
    // longer be assumed. This case owns its precondition.
    await seedBackupTable(dbSessionId);
    await backupToPath(dbSessionId, outPath, {
      options: [],
      compress: false,
    });

    await withSafeModeOff(() =>
      invokeBackend('restore_sql_file', {
        dbSessionId,
        overridePath: outPath,
        database: PG_CONFIG.database,
        options: ['overwrite'],
      }),
    );

    const check = await invokeBackend<{
      rows?: unknown[][];
      results?: { rows?: unknown[][] }[];
    }>('execute_query', {
      dbSessionId,
      sql: `SELECT COUNT(*)::int AS c FROM ${TEST_TABLE}`,
    });
    const rows = check.rows ?? check.results?.[0]?.rows ?? [];
    expect(Number(rows[0]?.[0])).toBe(1);

    fs.unlinkSync(outPath);
  });

  it('BACKUP-010: backup with invalid connection ID fails gracefully', async () => {
    const outPath = path.join(TMP_DIR, `datazen-backup-fail-${Date.now()}.sql`);
    let errorMsg = '';
    try {
      await backupToPath('nonexistent-id', outPath, {
        options: [],
        compress: false,
      });
    } catch (e) {
      errorMsg = String(e);
    }
    expect(errorMsg.length).toBeGreaterThan(0);
    expect(fs.existsSync(outPath)).toBe(false);
  });
});
