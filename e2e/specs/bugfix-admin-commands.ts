/**
 * E2E tests for admin command bugfixes — IPC-based approach.
 *
 * Creates connections via save_connection IPC, connects via IPC,
 * and verifies admin commands work correctly.
 * After each operation, re-calls get_databases / get_tables to verify
 * that the backend returns updated results (simulating the frontend
 * refresh flow: loadForConnection → getDatabases; loadTables → getTables).
 *
 * Tests also verify cleanup: all created resources are dropped at the end.
 */
import { expect, browser } from '@wdio/globals';
import { createConnection } from 'node:net';
import { closeExtraWindows } from '../helpers.js';

const MYSQL_HOST = process.env.E2E_MYSQL_HOST || '127.0.0.1';
const MYSQL_PORT = Number(process.env.E2E_MYSQL_PORT) || 3306;
const PG_HOST = process.env.E2E_PG_HOST || '127.0.0.1';
const PG_PORT = Number(process.env.E2E_PG_PORT) || 5432;

const UNIQUE = Date.now();
const MYSQL_TEST_DB = `e2e_db_${UNIQUE}`;
const PG_TEST_DB = `e2e_pgdb_${UNIQUE}`;
const PG_TEST_SCHEMA = `e2e_sch_${UNIQUE}`;
const PG_TEST_USER = `e2e_usr_${UNIQUE}`;

const MYSQL_USER = process.env.E2E_MYSQL_USER || 'root';
const MYSQL_PASSWORD = process.env.E2E_MYSQL_PASSWORD || '';

const PG_USER = process.env.E2E_PG_USER || 'wuxiaolong';
const PG_PASSWORD = process.env.E2E_PG_PASSWORD || '';
const PG_DB = process.env.E2E_PG_DB || 'goecoride';

async function tcpReachable(host: string, port: number, timeoutMs = 2000): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = createConnection({ host, port });
    const timer = setTimeout(() => {
      sock.destroy();
      resolve(false);
    }, timeoutMs);
    sock.on('connect', () => {
      clearTimeout(timer);
      sock.destroy();
      resolve(true);
    });
    sock.on('error', () => {
      clearTimeout(timer);
      resolve(false);
    });
  });
}

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__?: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
        ?.invoke(c, JSON.parse(a))
        .then((r) => done(r))
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

async function saveAndConnect(config: Record<string, unknown>): Promise<string> {
  await invokeBackend('save_connection', { config });
  return invokeBackend<string>('connect', { connectionId: config.id as string });
}

// ─── MySQL tests ───────────────────────────────────────────────

describe('MySQL admin commands (IPC)', () => {
  let connId: string;
  const connectionId = `e2e-mysql-admin-${UNIQUE}`;

  before(async function () {
    const reachable = await tcpReachable(MYSQL_HOST, MYSQL_PORT);
    if (!reachable) {
      console.log('MySQL unreachable, skipping');
      return this.skip();
    }

    connId = await saveAndConnect({
      id: connectionId,
      name: `E2E-MySQL-Admin-${UNIQUE}`,
      host: MYSQL_HOST,
      port: MYSQL_PORT,
      user: MYSQL_USER,
      password: MYSQL_PASSWORD,
      database: '',
      databaseType: 'mysql',
    });
  });

  it('should create a new database and list it', async function () {
    if (!connId) return this.skip();

    const dbsBefore = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });

    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'create_database',
        input: { name: MYSQL_TEST_DB },
      },
    });

    const dbsAfter = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });
    expect(dbsAfter).toContain(MYSQL_TEST_DB);
    expect(dbsAfter.length).toBeGreaterThan(dbsBefore.length);
  });

  it('should show user-level privileges via USER_PRIVILEGES', async function () {
    if (!connId) return this.skip();

    const result = await invokeBackend<{
      data: { results: { rows: unknown[][] }[] };
    }>('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'query',
        input: {
          sql: 'SELECT GRANTEE, PRIVILEGE_TYPE FROM information_schema.USER_PRIVILEGES LIMIT 5',
        },
      },
    });
    expect(result.data.results[0].rows.length).toBeGreaterThan(0);
  });

  it('should show new DB after simulated loadForConnection refresh', async function () {
    if (!connId) return this.skip();

    // Simulate the frontend refresh flow: get_databases is session-neutral
    // (F1 removed use_database) → must include the new DB.
    const dbs = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });
    expect(dbs).toContain(MYSQL_TEST_DB);
    expect(dbs.length).toBeGreaterThanOrEqual(2);
  });

  it('should show ALL databases (not locked to one) when no preferred DB', async function () {
    if (!connId) return this.skip();

    const dbs = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });
    expect(dbs.length).toBeGreaterThanOrEqual(2);
    expect(dbs).toContain(MYSQL_TEST_DB);
  });

  it('should list tables for a specific database via explicit param', async function () {
    if (!connId) return this.skip();

    // F1: get_tables takes the database explicitly — no use_database needed.
    const tables = await invokeBackend<{ name: string }[]>('get_tables', {
      dbSessionId: connId,
      database: MYSQL_TEST_DB,
    });
    // New DB has no tables yet — just verify it doesn't error
    expect(tables).toBeDefined();
    expect(Array.isArray(tables)).toBe(true);
  });

  it('should drop a database via driver command', async function () {
    if (!connId) return this.skip();

    const dropDb = `e2e_drop_${UNIQUE}`;
    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'create_database',
        input: { name: dropDb },
      },
    });

    const dbsBefore = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });
    expect(dbsBefore).toContain(dropDb);

    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'drop_database',
        input: { name: dropDb },
      },
    });

    const dbsAfter = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });
    expect(dbsAfter).not.toContain(dropDb);
  });

  after(async () => {
    if (connId) {
      try {
        await invokeBackend('execute_driver_command', {
          request: {
            dbSessionId: connId,
            command: 'execute',
            input: { sql: `DROP DATABASE IF EXISTS \`${MYSQL_TEST_DB}\`` },
          },
        });
      } catch {
        /* best effort */
      }
      try {
        await invokeBackend('disconnect', { dbSessionId: connId });
      } catch {
        /* best effort */
      }
      try {
        await invokeBackend('delete_connection', { id: connectionId });
      } catch {
        /* best effort */
      }
    }
  });
});

// ─── PostgreSQL tests ──────────────────────────────────────────

describe('PostgreSQL admin commands (IPC)', () => {
  let connId: string;
  const connectionId = `e2e-pg-admin-${UNIQUE}`;

  before(async function () {
    const reachable = await tcpReachable(PG_HOST, PG_PORT);
    if (!reachable) {
      console.log('PostgreSQL unreachable, skipping');
      return this.skip();
    }

    connId = await saveAndConnect({
      id: connectionId,
      name: `E2E-PG-Admin-${UNIQUE}`,
      host: PG_HOST,
      port: PG_PORT,
      user: PG_USER,
      password: PG_PASSWORD,
      database: PG_DB,
      databaseType: 'postgresql',
    });
  });

  it('should create a database via driver command', async function () {
    if (!connId) return this.skip();

    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'create_database',
        input: { name: PG_TEST_DB },
      },
    });

    const dbs = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });
    expect(dbs).toContain(PG_TEST_DB);
  });

  it('should create a schema and return it via get_tables (simulating loadTables)', async function () {
    if (!connId) return this.skip();

    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'create_schema',
        input: { name: PG_TEST_SCHEMA },
      },
    });

    // Simulate frontend loadTables flow: get_tables with explicit database (F1)
    const tablesResult = await invokeBackend<{ name: string; schema: string; tableType: string }[]>(
      'get_tables',
      { dbSessionId: connId, database: PG_DB },
    );

    // Verify new schema appears (either via SCHEMA_MARKER or table entries)
    const allSchemas = [...new Set(tablesResult.map((t) => t.schema).filter(Boolean))];
    expect(allSchemas).toContain(PG_TEST_SCHEMA);
    expect(allSchemas).toContain('public');

    // Verify schemaNames extraction (same logic as frontend setLoadedTables)
    const schemaNames = [
      ...new Set(tablesResult.map((t) => t.schema).filter((s): s is string => !!s)),
    ];
    expect(schemaNames).toContain(PG_TEST_SCHEMA);
    expect(schemaNames.length).toBeGreaterThanOrEqual(2);
  });

  it('should show ALL schemas after full loadForConnection+loadTables cycle', async function () {
    if (!connId) return this.skip();

    // Simulate full loadForConnection → getDatabases → loadTables flow
    const dbs = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });
    expect(dbs).toContain(PG_DB);

    const tablesResult = await invokeBackend<{ name: string; schema: string; tableType: string }[]>(
      'get_tables',
      { dbSessionId: connId, database: PG_DB },
    );

    const schemas = [...new Set(tablesResult.map((t) => t.schema).filter(Boolean))];
    expect(schemas).toContain('public');
    expect(schemas).toContain(PG_TEST_SCHEMA);
    expect(schemas.length).toBeGreaterThanOrEqual(2);
  });

  it('should create a user and verify in pg_roles', async function () {
    if (!connId) return this.skip();

    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'create_user',
        input: { username: PG_TEST_USER, password: 'test123' },
      },
    });

    const result = await invokeBackend<{
      data: { results: { rows: unknown[][] }[] };
    }>('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'query',
        input: {
          sql: `SELECT rolname FROM pg_roles WHERE rolname = '${PG_TEST_USER}'`,
        },
      },
    });
    expect(result.data.results[0].rows.length).toBe(1);
  });

  it('should grant privileges and verify', async function () {
    if (!connId) return this.skip();

    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'grant_privileges',
        input: {
          username: PG_TEST_USER,
          database: PG_DB,
          privileges: ['CONNECT'],
          grantOption: false,
        },
      },
    });

    const grantCheck = await invokeBackend<{
      data: { results: { rows: unknown[][] }[] };
    }>('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'query',
        input: {
          sql: `SELECT has_database_privilege('${PG_TEST_USER}', '${PG_DB}', 'CONNECT') AS has_priv`,
        },
      },
    });
    expect(grantCheck.data.results[0].rows[0][0]).toBe(true);
  });

  it('should drop a database via driver command', async function () {
    if (!connId) return this.skip();

    const dropDb = `e2e_pgdrop_${UNIQUE}`;
    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'create_database',
        input: { name: dropDb },
      },
    });

    const dbsBefore = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });
    expect(dbsBefore).toContain(dropDb);

    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'drop_database',
        input: { name: dropDb },
      },
    });

    const dbsAfter = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });
    expect(dbsAfter).not.toContain(dropDb);
  });

  it('should list newly created user in privileges query', async function () {
    if (!connId) return this.skip();

    const privResult = await invokeBackend<{
      data: { results: { rows: unknown[][] }[] };
    }>('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'query',
        input: {
          sql: `SELECT rolname AS grantee FROM pg_roles WHERE rolname NOT LIKE 'pg_%' AND rolcanlogin ORDER BY 1`,
        },
      },
    });
    const grantees = privResult.data.results[0].rows.map((r: unknown[]) => r[0]);
    expect(grantees).toContain(PG_TEST_USER);
  });

  it('should grant mixed privileges (table + database) without error', async function () {
    if (!connId) return this.skip();

    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'grant_privileges',
        input: {
          username: PG_TEST_USER,
          database: PG_DB,
          privileges: ['CONNECT', 'CREATE', 'SELECT', 'INSERT'],
          grantOption: false,
        },
      },
    });

    const createCheck = await invokeBackend<{
      data: { results: { rows: unknown[][] }[] };
    }>('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'query',
        input: {
          sql: `SELECT has_database_privilege('${PG_TEST_USER}', '${PG_DB}', 'CREATE') AS has_priv`,
        },
      },
    });
    expect(createCheck.data.results[0].rows[0][0]).toBe(true);
  });

  it('should drop a schema via driver command', async function () {
    if (!connId) return this.skip();

    const dropSchema = `e2e_drop_sch_${UNIQUE}`;
    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'create_schema',
        input: { name: dropSchema },
      },
    });

    const tablesBefore = await invokeBackend<{ name: string; schema: string }[]>('get_tables', {
      dbSessionId: connId,
      database: PG_DB,
    });
    const schemasBefore = [...new Set(tablesBefore.map((t) => t.schema).filter(Boolean))];
    expect(schemasBefore).toContain(dropSchema);

    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'drop_schema',
        input: { name: dropSchema, cascade: true },
      },
    });

    const tablesAfter = await invokeBackend<{ name: string; schema: string }[]>('get_tables', {
      dbSessionId: connId,
      database: PG_DB,
    });
    const schemasAfter = [...new Set(tablesAfter.map((t) => t.schema).filter(Boolean))];
    expect(schemasAfter).not.toContain(dropSchema);
  });

  it('should revoke privileges via driver command', async function () {
    if (!connId) return this.skip();

    // CREATE is not granted to PUBLIC by default, so revoking it actually works
    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'revoke_privileges',
        input: {
          username: PG_TEST_USER,
          database: PG_DB,
          privileges: ['CREATE'],
        },
      },
    });

    const check = await invokeBackend<{
      data: { results: { rows: unknown[][] }[] };
    }>('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'query',
        input: {
          sql: `SELECT has_database_privilege('${PG_TEST_USER}', '${PG_DB}', 'CREATE') AS has_priv`,
        },
      },
    });
    expect(check.data.results[0].rows[0][0]).toBe(false);
  });

  it('should drop a user via driver command', async function () {
    if (!connId) return this.skip();

    const dropUser = `e2e_drop_usr_${UNIQUE}`;
    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'create_user',
        input: { username: dropUser, password: 'test123' },
      },
    });

    const existsBefore = await invokeBackend<{
      data: { results: { rows: unknown[][] }[] };
    }>('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'query',
        input: { sql: `SELECT 1 FROM pg_roles WHERE rolname = '${dropUser}'` },
      },
    });
    expect(existsBefore.data.results[0].rows.length).toBe(1);

    await invokeBackend('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'drop_user',
        input: { username: dropUser },
      },
    });

    const existsAfter = await invokeBackend<{
      data: { results: { rows: unknown[][] }[] };
    }>('execute_driver_command', {
      request: {
        dbSessionId: connId,
        command: 'query',
        input: { sql: `SELECT 1 FROM pg_roles WHERE rolname = '${dropUser}'` },
      },
    });
    expect(existsAfter.data.results[0].rows.length).toBe(0);
  });

  it('should show created database after full refresh cycle', async function () {
    if (!connId) return this.skip();

    const dbs = await invokeBackend<string[]>('get_databases', { dbSessionId: connId });
    expect(dbs).toContain(PG_TEST_DB);
    expect(dbs).toContain(PG_DB);
    expect(dbs.length).toBeGreaterThanOrEqual(2);
  });

  after(async () => {
    if (connId) {
      try {
        await invokeBackend('execute_driver_command', {
          request: {
            dbSessionId: connId,
            command: 'execute',
            input: { sql: `DROP SCHEMA IF EXISTS "${PG_TEST_SCHEMA}"` },
          },
        });
      } catch {
        /* best effort */
      }
      try {
        await invokeBackend('execute_driver_command', {
          request: {
            dbSessionId: connId,
            command: 'execute',
            input: { sql: `REVOKE ALL ON DATABASE "${PG_DB}" FROM "${PG_TEST_USER}"` },
          },
        });
      } catch {
        /* best effort */
      }
      try {
        await invokeBackend('execute_driver_command', {
          request: {
            dbSessionId: connId,
            command: 'execute',
            input: { sql: `DROP ROLE IF EXISTS "${PG_TEST_USER}"` },
          },
        });
      } catch {
        /* best effort */
      }
      // Drop test database BEFORE disconnecting (uses drop_database driver command)
      try {
        await invokeBackend('execute_driver_command', {
          request: {
            dbSessionId: connId,
            command: 'drop_database',
            input: { name: PG_TEST_DB },
          },
        });
      } catch {
        /* best effort - may fail if DB wasn't created */
      }
      try {
        await invokeBackend('disconnect', { dbSessionId: connId });
      } catch {
        /* best effort */
      }
      try {
        await invokeBackend('delete_connection', { id: connectionId });
      } catch {
        /* best effort */
      }
    }
  });
});

// ─── Redis: no SQL file menu ───────────────────────────────────

describe('Redis context menu', () => {
  it('should NOT have Execute SQL File in main window connection context menu', async () => {
    const mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);

    const found = await browser.execute(() => {
      const items = document.querySelectorAll('[data-conn-item]');
      for (const item of items) {
        if (item.textContent?.includes('Redis')) {
          const rect = (item as HTMLElement).getBoundingClientRect();
          item.dispatchEvent(
            new MouseEvent('contextmenu', {
              bubbles: true,
              cancelable: true,
              clientX: rect.left + rect.width / 2,
              clientY: rect.top + rect.height / 2,
            }),
          );
          return true;
        }
      }
      return false;
    });

    if (!found) {
      console.log('No Redis connection, skipping');
      return;
    }

    await browser.pause(500);
    const menu = await browser.$('[data-testid="web-context-menu"]');
    if (await menu.isExisting()) {
      const text = await menu.getText();
      expect(text).not.toContain('执行 SQL 文件');
    }
    await browser.execute(() => document.dispatchEvent(new MouseEvent('mousedown')));
  });
});
