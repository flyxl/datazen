/**
 * SQL Server driver E2E — live journey through the app's own IPC surface.
 *
 * Unlike `sqlserver-smoke.ts` (a reachability placeholder) this spec drives the
 * *host* command path — `save_connection` → `connect` → `execute_query` →
 * `get_table_data` — against a real SQL Server / Azure SQL Database instance,
 * because that is where the driver's dialect behaviour becomes user-visible:
 *
 *  1. table browsing pages with `ORDER BY … OFFSET n ROWS FETCH NEXT m ROWS ONLY`
 *     and must never emit `LIMIT` (T-SQL rejects it: error 102);
 *  2. scratch DDL such as `CREATE SCHEMA` must be sent as a real batch, not
 *     through `sp_executesql` (error 156 otherwise);
 *  3. date/time columns must render as text, not as tiberius debug output.
 *
 * Skips unless `E2E_SQLSERVER_HOST`, `E2E_SQLSERVER_USER` and
 * `E2E_SQLSERVER_PASSWORD` are set — see `README.md`. Every object this spec
 * creates carries the `dz_e2e_` prefix and is dropped in `after`.
 *
 * Run:
 *   E2E_SQLSERVER_HOST=… E2E_SQLSERVER_USER=… E2E_SQLSERVER_PASSWORD=… \
 *     pnpm e2e:skip-build -- --spec packages/drivers/sqlserver/e2e/sqlserver-live-e2e.ts
 */
import { expect } from '@wdio/globals';
import { dbSessionIdOf } from '../../../../e2e/lib/sessionId.js';

const HOST = (process.env.E2E_SQLSERVER_HOST || '').trim();
const PORT = Number(process.env.E2E_SQLSERVER_PORT || '1433');
const USER = (process.env.E2E_SQLSERVER_USER || '').trim();
const PASSWORD = process.env.E2E_SQLSERVER_PASSWORD || '';
const DATABASE = (process.env.E2E_SQLSERVER_DATABASE || '').trim();
const SCHEMA = (process.env.E2E_SQLSERVER_SCHEMA || 'dbo').trim();
const SSL_MODE = (process.env.E2E_SQLSERVER_SSL_MODE || 'require').trim();
const TRUST_CERT = process.env.E2E_SQLSERVER_TRUST_CERT !== '0';

const CONNECTION_ID = 'e2e-sqlserver-driver';
const CONNECTION_NAME = 'E2E SQL Server';
const ROW_COUNT = 12;
const PAGE_SIZE = 5;
/** 40613 = Azure SQL serverless is still resuming. */
const RESUME_CODE = 'not currently available';

interface ColumnPayload {
  name: string;
  dataType: string;
}

interface StatementPayload {
  sql: string;
  columns: ColumnPayload[];
  rows: Array<Array<string | number | boolean | null>>;
  rowsAffected?: number;
  truncated?: boolean;
}

/** `execute_query` answers with the driver-command `MultiQueryResult`. */
interface MultiQueryPayload {
  results: StatementPayload[];
  totalTimeMs?: number;
}

interface TableDataPayload {
  columns: ColumnPayload[];
  rows: Array<Array<string | number | boolean | null>>;
  totalRows?: number;
  page: number;
  pageSize: number;
}

const scratchSchema = `dz_e2e_${Date.now().toString(36)}`;
const scratchTableName = 'e2e_rows';
const scratchViewName = 'e2e_view';

function skipReason(): string | null {
  if (process.env.E2E_SKIP_SQLSERVER === '1') return 'E2E_SKIP_SQLSERVER=1';
  if (!HOST || !USER || !PASSWORD) {
    return 'set E2E_SQLSERVER_HOST / E2E_SQLSERVER_USER / E2E_SQLSERVER_PASSWORD (see README.md)';
  }
  return null;
}

/** Invoke a Tauri command from the page context (mirrors e2e/helpers.ts). */
async function invoke<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      const internals = (
        window as unknown as {
          __TAURI_INTERNALS__?: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__;
      if (!internals) {
        done({ __error: '__TAURI_INTERNALS__ is unavailable (not a webdriver build?)' });
        return;
      }
      internals
        .invoke(c, JSON.parse(a))
        .then((r) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as Record<string, unknown>)) {
    throw new Error(String((result as { __error: string }).__error));
  }
  return result as T;
}

async function ensureMainWindow(): Promise<void> {
  const handles = await browser.getWindowHandles();
  if (handles[0]) await browser.switchToWindow(handles[0]);
}

describe('SQL Server driver E2E (live)', () => {
  let dbSessionId = '';
  let reason: string | null = null;

  const run = (sql: string) =>
    invoke<MultiQueryPayload>('execute_query', {
      dbSessionId,
      sql,
      ...(DATABASE ? { database: DATABASE } : {}),
    });

  /** First cell of the first result set of an `execute_query` call. */
  const firstCell = async (sql: string): Promise<string | number | boolean | null> => {
    const payload = await run(sql);
    const rows = payload.results?.[0]?.rows ?? [];
    if (rows.length === 0) throw new Error(`statement returned no rows: ${sql}`);
    return rows[0][0];
  };

  const readTable = (page: number, pageSize = PAGE_SIZE) =>
    invoke<TableDataPayload>('get_table_data', {
      request: {
        manualHandle: null,
        context: {
          connectionId: CONNECTION_ID,
          dbSessionId,
          driverType: 'sqlserver',
          database: DATABASE || null,
          schema: scratchSchema,
          table: scratchTableName,
        },
        page,
        pageSize,
        filters: null,
        sorts: null,
        skipCount: false,
        filterLogic: null,
      },
    });

  /**
   * Drop one scratch schema. Safe Mode blocks `DROP`/`TRUNCATE` in the host's
   * SQL guard, so callers run this with Safe Mode disabled.
   */
  const dropSchema = async (name: string) => {
    for (const statement of [
      `DROP VIEW IF EXISTS [${name}].[${scratchViewName}]`,
      `DROP TABLE IF EXISTS [${name}].[${scratchTableName}]`,
      `DROP SCHEMA IF EXISTS [${name}]`,
    ]) {
      try {
        await run(statement);
      } catch (error) {
        console.warn(`ℹ️  cleanup "${statement}" reported: ${String(error)}`);
      }
    }
  };

  /** Best-effort removal of `dz_e2e_` scratch schemas left by an earlier run. */
  const sweepStaleScratch = async () => {
    const payload = await run(
      "SELECT [name] FROM sys.schemas WHERE [name] LIKE 'dz\\_e2e\\_%' ESCAPE '\\'",
    );
    const names = (payload.results?.[0]?.rows ?? [])
      .map((row) => String(row[0]))
      .filter((name) => name !== scratchSchema);
    for (const name of names) {
      console.warn(`ℹ️  removing stale scratch schema from an earlier run: ${name}`);
      await dropSchema(name);
    }
  };

  /** Toggle the host SQL guard; the isolated E2E data dir starts with it on. */
  const setSafeMode = async (enabled: boolean) => {
    const settings = await invoke<Record<string, unknown>>('get_host_settings');
    if ((settings.safeMode !== false) === enabled) return;
    await invoke('save_host_settings', { settings: { ...settings, safeMode: enabled } });
  };

  before(async function () {
    this.timeout(120_000);
    reason = skipReason();
    if (reason) {
      console.warn(`⏩ Skipping SQL Server live E2E: ${reason}`);
      this.skip();
    }

    await ensureMainWindow();
    await invoke('save_connection', {
      config: {
        id: CONNECTION_ID,
        name: CONNECTION_NAME,
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

    // Azure SQL serverless may still be resuming after an idle period.
    let lastError: unknown;
    for (let attempt = 1; attempt <= 4 && !dbSessionId; attempt += 1) {
      try {
        dbSessionId = dbSessionIdOf(
          await invoke<unknown>('connect', { connectionId: CONNECTION_ID }),
        );
      } catch (error) {
        lastError = error;
        if (!String(error).toLowerCase().includes(RESUME_CODE) || attempt === 4) throw error;
        console.warn(`⏳ database resuming (attempt ${attempt}/4), retrying in 8s`);
        await browser.pause(8000);
      }
    }
    if (!dbSessionId) throw lastError ?? new Error('connect returned no dbSessionId');

    // Safe Mode blocks DROP, so scratch-object housekeeping needs it off.
    await setSafeMode(false);
    await sweepStaleScratch();
    await run(`CREATE SCHEMA [${scratchSchema}]`);
    await run(
      `CREATE TABLE [${scratchSchema}].[${scratchTableName}] (` +
        '[id] INT NOT NULL PRIMARY KEY, ' +
        '[label] NVARCHAR(40) NOT NULL, ' +
        '[created_on] DATE NOT NULL, ' +
        '[recorded_at] DATETIME2(3) NOT NULL, ' +
        '[amount] DECIMAL(12,2) NULL, ' +
        '[payload] VARBINARY(8) NULL, ' +
        '[note] NVARCHAR(60) NULL)',
    );
    const values = Array.from({ length: ROW_COUNT }, (_, i) => {
      const day = String((i % 9) + 1).padStart(2, '0');
      return (
        `(${i + 1}, N'row-${i + 1}', '2026-03-${day}', ` +
        `'2026-03-${day}T0${i % 10}:15:30.${String(i).padStart(3, '0')}', ` +
        `${(i + 1) * 10}.5, 0x0${i % 10}a, ` +
        `${i % 3 === 0 ? 'NULL' : `N'note ${i + 1}'`})`
      );
    }).join(', ');
    await run(
      `INSERT INTO [${scratchSchema}].[${scratchTableName}] ` +
        '([id], [label], [created_on], [recorded_at], [amount], [payload], [note]) ' +
        `VALUES ${values}`,
    );
  });

  after(async function () {
    this.timeout(60_000);
    if (!dbSessionId) return;
    try {
      await dropSchema(scratchSchema);
      await invoke('disconnect', { dbSessionId });
    } finally {
      await setSafeMode(true);
      await invoke('delete_connection', { id: CONNECTION_ID }).catch(() => undefined);
    }
  });

  it('creates scratch objects through the batch-DDL path', async () => {
    // `CREATE SCHEMA` and `CREATE VIEW` are only accepted as the first statement
    // of a real batch; a driver that wraps them in sp_executesql fails here.
    expect(
      Number(
        await firstCell(`SELECT COUNT(*) AS n FROM sys.schemas WHERE name = '${scratchSchema}'`),
      ),
    ).toBe(1);

    await run(
      `CREATE VIEW [${scratchSchema}].[${scratchViewName}] AS ` +
        `SELECT [id], [label], [created_on] FROM [${scratchSchema}].[${scratchTableName}] WHERE [id] <= 3`,
    );
    expect(
      Number(await firstCell(`SELECT COUNT(*) AS n FROM [${scratchSchema}].[${scratchViewName}]`)),
    ).toBe(3);
  });

  it('reads the first page without emitting LIMIT', async () => {
    const page = await readTable(0);
    expect(page.rows).toHaveLength(PAGE_SIZE);
    const names = page.columns.map((c) => c.name);
    expect(names).toContain('id');
    expect(names).toContain('created_on');
    expect(names).toContain('recorded_at');
    expect(page.rows.map((row) => Number(row[names.indexOf('id')]))).toEqual([1, 2, 3, 4, 5]);
  });

  it('pages the second slice with OFFSET … FETCH NEXT', async () => {
    const page = await readTable(1);
    expect(page.rows).toHaveLength(PAGE_SIZE);
    const names = page.columns.map((c) => c.name);
    expect(page.rows.map((row) => Number(row[names.indexOf('id')]))).toEqual([6, 7, 8, 9, 10]);

    const last = await readTable(2);
    expect(last.rows).toHaveLength(ROW_COUNT - 2 * PAGE_SIZE);
    expect(last.rows.map((row) => Number(row[names.indexOf('id')]))).toEqual([11, 12]);
  });

  it('renders temporal columns as text, not tiberius debug output', async () => {
    const page = await readTable(0);
    const names = page.columns.map((c) => c.name);
    const date = page.rows[0][names.indexOf('created_on')];
    const stamp = page.rows[0][names.indexOf('recorded_at')];

    expect(typeof date).toBe('string');
    expect(String(date)).toMatch(/^\d{4}-\d{2}-\d{2}$/);
    expect(String(stamp)).toMatch(/^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}/);
    for (const cell of [date, stamp]) {
      expect(String(cell)).not.toContain('Date(');
      expect(String(cell)).not.toContain('increments');
    }
  });

  it('round-trips a filtered, sorted read through the driver', async () => {
    const filtered = await invoke<TableDataPayload>('get_table_data', {
      request: {
        manualHandle: null,
        context: {
          connectionId: CONNECTION_ID,
          dbSessionId,
          driverType: 'sqlserver',
          database: DATABASE || null,
          schema: scratchSchema,
          table: scratchTableName,
        },
        page: 0,
        pageSize: PAGE_SIZE,
        filters: [{ column: 'id', operator: 'gt', value: 4 }],
        sorts: [{ column: 'id', descending: true }],
        skipCount: false,
        filterLogic: null,
      },
    });
    const names = filtered.columns.map((c) => c.name);
    expect(filtered.rows.map((row) => Number(row[names.indexOf('id')]))).toEqual([
      12, 11, 10, 9, 8,
    ]);
  });
});
