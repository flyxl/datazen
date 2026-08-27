/**
 * Shared E2E app-data setup / teardown via Tauri IPC.
 * Database reset lives in e2e/setup-e2e-env.sh + e2e/teardown-e2e-env.sh (run.mjs).
 */
import type { Browser } from '@wdio/globals';

type ConnectionRow = { id: string; name?: string };
type WorkflowRow = { id: string };

/** Seeded in global before hook; never deleted by global teardown. */
export const KEEP_CONNECTION_IDS = new Set(['conn_e2e_pg']);

export function isEphemeralConnection(conn: ConnectionRow): boolean {
  if (KEEP_CONNECTION_IDS.has(conn.id)) return false;
  const id = conn.id;
  const name = conn.name ?? '';
  if (/^e2e[-_]/i.test(id)) return true;
  if (/^sync_/i.test(id)) return true;
  if (/^E2E-/i.test(name)) return true;
  if (name === 'E2E-自动测试' || name === 'E2E-测试连接') return true;
  return false;
}

export function isEphemeralWorkflow(wf: WorkflowRow): boolean {
  return /^e2e[-_]/i.test(wf.id);
}

async function invoke<T>(
  browser: Browser,
  cmd: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as unknown as { __TAURI_INTERNALS__: { invoke: Function } }).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as object)) {
    throw new Error(String((result as { __error: string }).__error));
  }
  return result as T;
}

/** Upsert a second PG connection for schema-diff / sync window specs. */
export async function seedSecondPgConnection(browser: Browser): Promise<void> {
  const pgHost = process.env.E2E_PG_HOST || process.env.PG_HOST || '127.0.0.1';
  const pgPort = Number(process.env.E2E_PG_PORT || process.env.PG_PORT) || 5432;
  const pgUser = process.env.E2E_PG_USER || process.env.PG_USER || 'postgres';
  const pgPassword = process.env.E2E_PG_PASSWORD || process.env.PG_PASSWORD || '';
  const pgDatabase = process.env.E2E_PG_DB || process.env.PG_DATABASE || 'postgres';

  await browser.executeAsync(
    (
      host: string,
      port: number,
      user: string,
      pw: string,
      db: string,
      done: (r: unknown) => void,
    ) => {
      const config = {
        id: 'conn_e2e_pg_tgt',
        name: 'E2E-PG-目标',
        databaseType: 'postgresql',
        host,
        port,
        username: user,
        password: pw,
        database: db,
        group: 'E2E 测试',
        colorTag: 'green',
        sslMode: 'disable',
      };
      (window as unknown as { __TAURI_INTERNALS__: { invoke: Function } }).__TAURI_INTERNALS__
        .invoke('save_connection', { config })
        .then(() => done(null))
        .catch((e: unknown) => done(String(e)));
    },
    pgHost,
    pgPort,
    pgUser,
    pgPassword,
    pgDatabase,
  );
}

/** Upsert the default PostgreSQL connection used by most DB specs. */
export async function seedDefaultPgConnection(browser: Browser): Promise<void> {
  const pgHost = process.env.E2E_PG_HOST || process.env.PG_HOST || '127.0.0.1';
  const pgPort = Number(process.env.E2E_PG_PORT || process.env.PG_PORT) || 5432;
  const pgUser = process.env.E2E_PG_USER || process.env.PG_USER || 'postgres';
  const pgPassword = process.env.E2E_PG_PASSWORD || process.env.PG_PASSWORD || '';
  const pgDatabase = process.env.E2E_PG_DB || process.env.PG_DATABASE || 'postgres';

  await browser.executeAsync(
    (
      host: string,
      port: number,
      user: string,
      pw: string,
      db: string,
      done: (r: unknown) => void,
    ) => {
      const config = {
        id: 'conn_e2e_pg',
        name: '本地 PostgreSQL',
        databaseType: 'postgresql',
        host,
        port,
        username: user,
        password: pw,
        database: db,
        group: 'E2E 测试',
        colorTag: 'blue',
        sslMode: 'disable',
      };
      (window as unknown as { __TAURI_INTERNALS__: { invoke: Function } }).__TAURI_INTERNALS__
        .invoke('save_connection', { config })
        .then(() => done(null))
        .catch((e: unknown) => done(String(e)));
    },
    pgHost,
    pgPort,
    pgUser,
    pgPassword,
    pgDatabase,
  );
}

/** Remove connections / workflows / query history created during the suite. */
export async function cleanupAppDataViaIpc(browser: Browser): Promise<void> {
  try {
    await browser.url('tauri://localhost');
    await browser.pause(300);
  } catch {
    /* app may already be shutting down */
  }

  let conns: ConnectionRow[] = [];
  try {
    conns = await invoke<ConnectionRow[]>(browser, 'get_connections');
  } catch (err) {
    console.warn('[e2e-teardown] get_connections failed:', err);
  }

  for (const conn of conns) {
    if (!isEphemeralConnection(conn)) continue;
    try {
      await invoke(browser, 'delete_connection', { id: conn.id });
    } catch {
      /* ignore per-connection failures */
    }
  }

  let workflows: WorkflowRow[] = [];
  try {
    workflows = await invoke<WorkflowRow[]>(browser, 'workflow_list');
  } catch (err) {
    console.warn('[e2e-teardown] workflow_list failed:', err);
  }

  for (const wf of workflows) {
    if (!isEphemeralWorkflow(wf)) continue;
    try {
      await invoke(browser, 'workflow_delete', { workflowId: wf.id });
    } catch {
      /* ignore */
    }
  }

  try {
    await invoke(browser, 'workflow_history_clear', { workflowId: null });
  } catch {
    /* ignore */
  }

  try {
    await invoke(browser, 'clear_query_history');
  } catch {
    /* ignore */
  }
}
