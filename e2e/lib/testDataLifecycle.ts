/**
 * Shared E2E app-data setup / teardown via Tauri IPC.
 * Database reset lives in e2e/setup-e2e-env.sh + e2e/teardown-e2e-env.sh (run.mjs).
 */
import { execSync } from 'node:child_process';
import type { Browser } from '@wdio/globals';

type ConnectionRow = { id: string; name?: string };
type WorkflowRow = { id: string };

// ---------------------------------------------------------------------------
// Per-worker isolated database lifecycle
// ---------------------------------------------------------------------------
// Each WDIO worker creates a disposable PostgreSQL database named
// `e2e_wd_<N>`.  Specs create their own tables inside; the database is
// dropped after all specs in the worker finish.

let _workerCounter = 0;

/** PG connection params resolved once from env. */
function pgConn() {
  return {
    host: process.env.E2E_PG_HOST || '127.0.0.1',
    port: process.env.E2E_PG_PORT || '5432',
    user: process.env.E2E_PG_USER || process.env.PG_USER || 'wuxiaolong',
    password: process.env.E2E_PG_PASSWORD || process.env.PG_PASSWORD || '',
  };
}

/** SQL-escape a database identifier for use inside psql -c '...' (single-quoted shell string). */
function q(name: string): string {
  return `"${name.replace(/"/g, '""')}"`;
}

/**
 * Generate a unique worker database name.  Each WDIO worker is a separate
 * Node.js process, so we combine PID + monotonic counter + random suffix
 * to guarantee uniqueness even when multiple workers start simultaneously.
 */
function workerDbName(): string {
  const pid = process.pid;
  const seq = _workerCounter++;
  const rand = Math.random().toString(36).slice(2, 6);
  return `e2e_w${pid}_${seq}_${rand}`;
}

/**
 * Create a fresh, empty PostgreSQL database for the current worker,
 * then seed it with the standard E2E fixture tables (product, e2e_contract_*).
 * Returns the database name (`e2e_wd_0`, `e2e_wd_1`, …).
 */
export function createWorkerDatabase(): string {
  const db = workerDbName();
  const { host, port, user, password } = pgConn();
  const env = { ...process.env, PGPASSWORD: password };
  try {
    execSync(
      `psql -h ${host} -p ${port} -U ${user} -d postgres -v ON_ERROR_STOP=1 -c 'CREATE DATABASE ${q(db)}'`,
      { env, stdio: 'pipe' },
    );
  } catch (err: unknown) {
    const msg = String(err);
    if (!msg.includes('already exists')) throw err;
  }

  // PostgreSQL 15+ no longer grants CREATE on the public schema to PUBLIC.
  // Ensure the connecting user can create tables in the new database.
  try {
    execSync(
      `psql -h ${host} -p ${port} -U ${user} -d ${q(db)} -v ON_ERROR_STOP=1 -c 'GRANT ALL PRIVILEGES ON SCHEMA public TO PUBLIC; ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON TABLES TO PUBLIC; ALTER DEFAULT PRIVILEGES IN SCHEMA public GRANT ALL ON SEQUENCES TO PUBLIC;'`,
      { env, stdio: 'pipe' },
    );
  } catch {
    // best-effort: older PG versions already grant this
  }

  // Seed standard fixture tables so specs that expect `product` etc. work.
  const seedSql = `
CREATE TABLE IF NOT EXISTS product (
  id SERIAL PRIMARY KEY,
  name TEXT NOT NULL DEFAULT 'item',
  status TEXT NOT NULL
);
DELETE FROM product;
INSERT INTO product (name, status) VALUES
  ('Widget', 'active'), ('Gadget', 'active'),
  ('Thing', 'pending'),  ('Gizmo', 'inactive');

DO $$
DECLARE table_name text;
BEGIN
  FOREACH table_name IN ARRAY ARRAY[
    'e2e_contract_conn', 'e2e_contract_data', 'e2e_contract_filter',
    'e2e_contract_edit', 'e2e_contract_struct', 'e2e_contract_index',
    'e2e_contract_export'
  ] LOOP
    EXECUTE format('CREATE TABLE IF NOT EXISTS %I (id SERIAL PRIMARY KEY, name TEXT NOT NULL, status TEXT NOT NULL)', table_name);
  END LOOP;
END $$;
`;
  try {
    execSync(`psql -h ${host} -p ${port} -U ${user} -d ${q(db)} -v ON_ERROR_STOP=1`, {
      env,
      input: seedSql,
      stdio: 'pipe',
    });
  } catch (err: unknown) {
    console.warn(`[e2e] seed warning for ${db}:`, String(err).slice(0, 200));
  }

  console.log(`[e2e] created worker database: ${db}`);
  return db;
}

/**
 * Drop a worker database (best-effort, called in global after hook).
 */
export function dropWorkerDatabase(db: string): void {
  const { host, port, user, password } = pgConn();
  const env = { ...process.env, PGPASSWORD: password };
  try {
    // Terminate existing connections before dropping.
    execSync(
      `psql -h ${host} -p ${port} -U ${user} -d postgres -v ON_ERROR_STOP=1 -c 'SELECT pg_terminate_backend(pid) FROM pg_stat_activity WHERE datname = ${q(db)} AND pid <> pg_backend_pid()'`,
      { env, stdio: 'pipe' },
    );
    execSync(
      `psql -h ${host} -p ${port} -U ${user} -d postgres -v ON_ERROR_STOP=1 -c 'DROP DATABASE IF EXISTS ${q(db)}'`,
      { env, stdio: 'pipe' },
    );
    console.log(`[e2e] dropped worker database: ${db}`);
  } catch {
    // best-effort
  }
}

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

/**
 * Upsert a second PG connection for schema-diff / sync window specs.
 * @param workerDb  Optional per-worker database name (overrides env).
 */
export async function seedSecondPgConnection(browser: Browser, workerDb?: string): Promise<void> {
  const pgHost = process.env.E2E_PG_HOST || process.env.PG_HOST || '127.0.0.1';
  const pgPort = Number(process.env.E2E_PG_PORT || process.env.PG_PORT) || 5432;
  const pgUser = process.env.E2E_PG_USER || process.env.PG_USER || 'postgres';
  const pgPassword = process.env.E2E_PG_PASSWORD || process.env.PG_PASSWORD || '';
  const pgDatabase = workerDb || process.env.E2E_PG_DB || process.env.PG_DATABASE || 'postgres';

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

/**
 * Upsert the default PostgreSQL connection used by most DB specs.
 * @param workerDb  Optional per-worker database name (overrides env).
 */
export async function seedDefaultPgConnection(browser: Browser, workerDb?: string): Promise<void> {
  const pgHost = process.env.E2E_PG_HOST || process.env.PG_HOST || '127.0.0.1';
  const pgPort = Number(process.env.E2E_PG_PORT || process.env.PG_PORT) || 5432;
  const pgUser = process.env.E2E_PG_USER || process.env.PG_USER || 'postgres';
  const pgPassword = process.env.E2E_PG_PASSWORD || process.env.PG_PASSWORD || '';
  const pgDatabase = workerDb || process.env.E2E_PG_DB || process.env.PG_DATABASE || 'postgres';
  const pgSchema = process.env.E2E_WORKER_SCHEMA || undefined;

  await browser.executeAsync(
    (
      host: string,
      port: number,
      user: string,
      pw: string,
      db: string,
      schema: string | undefined,
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
        schema: schema || undefined,
        group: 'E2E 测试',
        colorTag: 'blue',
        sslMode: 'disable',
        options: {},
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
    pgSchema,
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
