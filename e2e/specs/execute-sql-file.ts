/**
 * E2E: Execute SQL File via webdriver-only path IPC (TS-SF-E01/E02).
 * Uses execute_sql_file (direct path, no native dialog) to drive the
 * same streaming pipeline as the dialog entry.
 */
import { expect, browser } from '@wdio/globals';
import { mkdtempSync, writeFileSync, rmSync } from 'node:fs';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { closeExtraWindows, openSeededPgConnectionWindow } from '../helpers.js';

async function getConnectionId(): Promise<string | null> {
  const conns = await browser.executeAsync((done: (r: unknown) => void) => {
    (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
      ?.invoke('get_connections')
      .then(done)
      .catch(() => done([]));
  });
  const arr = (conns ?? []) as Array<{ id: string; name: string }>;
  return arr.find((c) => c.name === '本地 PostgreSQL')?.id ?? null;
}

async function runQuery(connId: string, sql: string): Promise<unknown> {
  return browser.executeAsync(
    (cid: string, s: string, done: (r: unknown) => void) => {
      (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
        ?.invoke('execute_driver_command', {
          request: { connectionId: cid, command: 'query', input: { sql: s } },
        })
        .then(done)
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    connId,
    sql,
  );
}

describe('Execute SQL File (SF)', () => {
  let mainWindow: string;
  let connId = '';
  let dir = '';
  const STAMP = Date.now().toString(36);
  const T1 = 'e2e_sf_ok_' + STAMP;
  const T2 = 'e2e_sf_err_' + STAMP;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await openSeededPgConnectionWindow(mainWindow);
    const cid = await getConnectionId();
    if (!cid) throw new Error('seeded PG connection not found');
    connId = cid;
    dir = mkdtempSync(join(tmpdir(), 'datazen-sf-'));
  });

  after(async () => {
    if (connId) {
      await runQuery(connId, 'DROP TABLE IF EXISTS ' + T1);
      await runQuery(connId, 'DROP TABLE IF EXISTS ' + T2);
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
      (cid: string, path: string, done: (r: unknown) => void) => {
        (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
          ?.invoke('execute_sql_file', { connectionId: cid, inputPath: path })
          .then(done)
          .catch((e: unknown) => done({ __error: String(e) }));
      },
      connId,
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
    const ok = (await invokeSqlFile(file)) as { __error?: string };
    expect(ok.__error).toBeUndefined();
    expect(ok).toBe(true);
    const res = (await runQuery(connId, 'SELECT COUNT(*) AS n FROM ' + T1)) as {
      data?: { results?: Array<{ rows?: Array<{ n?: number }> }> };
      __error?: string;
    };
    expect(res.__error).toBeUndefined();
    const rows = res.data?.results?.[0]?.rows ?? [];
    expect(rows[0]?.n).toBe(3);
  });

  it('SF-E02: a failing statement rejects', async () => {
    const file = join(dir, T2 + '.sql');
    const stmts = [
      'CREATE TABLE ' + T2 + ' (id INT PRIMARY KEY);',
      'INSERT INTO no_such_table_' + STAMP + ' VALUES (1);',
    ].join('\n');
    writeFileSync(file, stmts);
    const res = (await invokeSqlFile(file)) as { __error?: string };
    expect(res.__error).toBeDefined();
  });
});
