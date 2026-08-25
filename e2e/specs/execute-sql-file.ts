/**
 * E2E: Execute SQL File via webdriver-only path IPC (TS-SF-E01/E02).
 * Uses execute_sql_file (direct path, no native dialog) to drive the
 * same streaming pipeline as the dialog entry.
 */
import { expect, browser } from '@wdio/globals';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { closeExtraWindows, openSeededPgConnectionWindow, withSafeModeOff } from '../helpers.js';

async function getConnectionConfigId(): Promise<string | null> {
  const conns = await browser.executeAsync((done: (r: unknown) => void) => {
    (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
      ?.invoke('get_connections')
      .then(done)
      .catch(() => done([]));
  });
  const arr = (conns ?? []) as Array<{ id: string; name: string }>;
  return arr.find((c) => c.name === '本地 PostgreSQL')?.id ?? null;
}

async function runQuery(dbSessionId: string, sql: string): Promise<unknown> {
  return browser.executeAsync(
    (sessionId: string, s: string, done: (r: unknown) => void) => {
      (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
        ?.invoke('execute_driver_command', {
          request: { dbSessionId: sessionId, command: 'query', input: { sql: s } },
        })
        .then(done)
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    dbSessionId,
    sql,
  );
}

describe('Execute SQL File (SF)', () => {
  let mainWindow: string;
  let dbSessionId = '';
  let dir = '';
  const STAMP = Date.now().toString(36);
  const T1 = 'e2e_sf_ok_' + STAMP;
  const T2 = 'e2e_sf_err_' + STAMP;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await openSeededPgConnectionWindow(mainWindow);
    const connectionId = await getConnectionConfigId();
    if (!connectionId) throw new Error('seeded PG connection not found');
    // Persisted config id → live db session id via connect (backup-database pattern).
    dbSessionId = (await browser.executeAsync((cid: string, done: (r: unknown) => void) => {
      (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
        ?.invoke('connect', { connectionId: cid })
        .then(done)
        .catch((e: unknown) => done({ __error: String(e) }));
    }, connectionId)) as string;
    if (typeof dbSessionId !== 'string' || !dbSessionId) {
      throw new Error(`connect(${connectionId}) failed: ${JSON.stringify(dbSessionId)}`);
    }
    dir = mkdtempSync(join(tmpdir(), 'datazen-sf-'));
  });

  after(async () => {
    try {
      if (dbSessionId) {
        // Safe mode blocks bare DROP statements — lift it only for cleanup.
        await withSafeModeOff(async () => {
          await runQuery(dbSessionId, 'DROP TABLE IF EXISTS ' + T1);
          await runQuery(dbSessionId, 'DROP TABLE IF EXISTS ' + T2);
        });
        await browser.executeAsync((sid: string, done: (r: unknown) => void) => {
          (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
            ?.invoke('disconnect', { dbSessionId: sid })
            .then(done)
            .catch(() => done(null));
        }, dbSessionId);
      }
    } catch {
      /* ignore */
    }
    try {
      rmSync(dir, { recursive: true, force: true });
    } catch {
      /* ignore */
    }
    await closeExtraWindows(mainWindow);
  });

  function invokeSqlFile(p: string) {
    return browser.executeAsync(
      (sessionId: string, path: string, done: (r: unknown) => void) => {
        (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
          ?.invoke('execute_sql_file', { dbSessionId: sessionId, inputPath: path })
          .then(done)
          .catch((e: unknown) => done({ __error: String(e) }));
      },
      dbSessionId,
      p,
    );
  }

  it('SF-E01: executes a valid file end to end', async () => {
    const file = join(dir, T1 + '.sql');
    const stmts = [
      'CREATE TABLE ' + T1 + ' (id INT PRIMARY KEY);',
      'INSERT INTO ' + T1 + ' VALUES (1), (2), (3);',
    ].join('\n');
    writeFileSync(file, stmts);
    const ok = (await withSafeModeOff(() => invokeSqlFile(file))) as { __error?: string };
    expect(ok.__error).toBeUndefined();
    expect(ok).toBe(true);
    const res = (await runQuery(dbSessionId, 'SELECT COUNT(*) FROM ' + T1)) as {
      data?: { results?: Array<{ rows?: unknown[][] }> };
      __error?: string;
    };
    expect(res.__error).toBeUndefined();
    // The driver `query` Command returns positional rows (see bugfix-admin-commands).
    const rows = res.data?.results?.[0]?.rows ?? [];
    expect(rows[0]?.[0]).toBe(3);
  });

  it('SF-E02: a failing statement rejects', async () => {
    const file = join(dir, T2 + '.sql');
    const stmts = [
      'CREATE TABLE ' + T2 + ' (id INT PRIMARY KEY);',
      'INSERT INTO no_such_table_' + STAMP + ' VALUES (1);',
    ].join('\n');
    writeFileSync(file, stmts);
    const res = (await withSafeModeOff(() => invokeSqlFile(file))) as { __error?: string };
    expect(res.__error).toBeDefined();
  });
});
