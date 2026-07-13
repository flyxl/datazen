import { expect, browser } from '@wdio/globals';
import path from 'node:path';
import fs from 'node:fs';
import os from 'node:os';

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

async function seedBackupTable(connId: string) {
  await invokeBackend('execute_query', {
    connectionId: connId,
    sql: `
      DROP TABLE IF EXISTS ${TEST_TABLE};
      CREATE TABLE ${TEST_TABLE} (
        id SERIAL PRIMARY KEY,
        name TEXT NOT NULL
      );
      INSERT INTO ${TEST_TABLE} (name) VALUES ('alice');
    `,
  });
}

async function dropBackupTable(connId: string) {
  await invokeBackend('execute_query', {
    connectionId: connId,
    sql: `DROP TABLE IF EXISTS ${TEST_TABLE}`,
  });
}

// ═════════════════════════════════════════════════════════════════════
// Group 1: Backend backup command tests
// ═════════════════════════════════════════════════════════════════════

describe('数据库备份功能 (BACKUP)', () => {
  let connectionId: string;

  before(async () => {
    await browser.setTimeout({ script: 120000 });
    await browser.pause(3000);
    await invokeBackend('save_connection', { config: PG_CONFIG });
    connectionId = await invokeBackend<string>('connect', { configId: PG_CONFIG.id });
    await seedBackupTable(connectionId);
  });

  after(async () => {
    try { await dropBackupTable(connectionId); } catch { /* ok */ }
    try { await invokeBackend('disconnect', { connectionId }); } catch { /* ok */ }
    try { await invokeBackend('delete_connection', { id: PG_CONFIG.id }); } catch { /* ok */ }
  });

  it('BACKUP-001: connect returns a valid connection ID string', async () => {
    expect(typeof connectionId).toBe('string');
    expect(connectionId.length).toBeGreaterThan(0);
  });

  it('BACKUP-002: get_databases returns database list', async () => {
    const dbs = await invokeBackend<string[]>('get_databases', { connectionId });
    expect(Array.isArray(dbs)).toBe(true);
    expect(dbs.length).toBeGreaterThan(0);
    expect(dbs).toContain(PG_CONFIG.database);
  });

  it('BACKUP-003: backup_database creates a SQL file', async () => {
    const outPath = path.join(TMP_DIR, `datazen-backup-test-${Date.now()}.sql`);

    await invokeBackend('backup_database', {
      connectionId,
      database: PG_CONFIG.database,
      outputPath: outPath,
      options: [],
      compress: false,
    });

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

    await invokeBackend('backup_database', {
      connectionId,
      database: PG_CONFIG.database,
      outputPath: outPath,
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
    await invokeBackend('backup_database', {
      connectionId,
      database: PG_CONFIG.database,
      outputPath: outPath,
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

    await invokeBackend('backup_database', {
      connectionId,
      database: PG_CONFIG.database,
      outputPath: outPath,
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

    await invokeBackend('backup_database', {
      connectionId,
      database: PG_CONFIG.database,
      outputPath: outPath,
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

    await invokeBackend('backup_database', {
      connectionId,
      database: PG_CONFIG.database,
      outputPath: outPath,
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

    await invokeBackend('backup_database', {
      connectionId,
      database: PG_CONFIG.database,
      outputPath: outPath,
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

  it('BACKUP-010: backup with invalid connection ID fails gracefully', async () => {
    const outPath = path.join(TMP_DIR, `datazen-backup-fail-${Date.now()}.sql`);
    let errorMsg = '';
    try {
      await invokeBackend('backup_database', {
        connectionId: 'nonexistent-id',
        database: PG_CONFIG.database,
        outputPath: outPath,
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
