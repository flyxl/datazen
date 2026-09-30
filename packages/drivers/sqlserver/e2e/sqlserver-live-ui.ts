/**
 * SQL Server driver GUI journey — the real window, the real navigator and the
 * real table data grid, against a live instance.
 *
 * `sqlserver-live-e2e.ts` covers the host command path (`get_table_data`, …) via
 * IPC; this spec covers what the user actually sees, because two of the defects
 * found in this round were only visible in the UI:
 *
 *  1. opening a table used to fail with `Incorrect syntax near 'LIMIT'`, so the
 *     data grid never rendered at all;
 *  2. the schema/column enumeration returned a single column per table, so the
 *     grid (and the structure view) was missing columns and the primary key.
 *
 * The scratch objects are created over IPC (deterministic, no form typing), then
 * the connection is opened **through the navigator** so the grid is reached the
 * way a user reaches it. Safe Mode is disabled for the duration because the host
 * SQL guard blocks the `DROP` statements used for cleanup.
 *
 * Skips unless `E2E_SQLSERVER_HOST` / `USER` / `PASSWORD` are set — see README.md.
 *
 * Run:
 *   pnpm e2e:skip-build -- --spec packages/drivers/sqlserver/e2e/sqlserver-live-ui.ts
 */
import { mkdirSync } from 'node:fs';
import { browser, $, expect } from '@wdio/globals';
import {
  clickCardConnectButton,
  ensureMainWindowForIpc,
  expandAllGroups,
  expandConnectedConnectionInNavigator,
  expandSchemaCategory,
  openConnectionsWorkspace,
  openQueryTab,
  setEditorContent,
  waitForConnectionToolbar,
  waitForSchemaTreeLoaded,
} from '../../../../e2e/helpers.js';
import { selectSqlEditorSubstring } from '../../../../e2e/helpers/sqlEditorHelper.js';

const HOST = (process.env.E2E_SQLSERVER_HOST || '').trim();
const PORT = Number(process.env.E2E_SQLSERVER_PORT || '1433');
const USER = (process.env.E2E_SQLSERVER_USER || '').trim();
const PASSWORD = process.env.E2E_SQLSERVER_PASSWORD || '';
const DATABASE = (process.env.E2E_SQLSERVER_DATABASE || '').trim();
const SCHEMA = (process.env.E2E_SQLSERVER_SCHEMA || 'dbo').trim();
const SSL_MODE = (process.env.E2E_SQLSERVER_SSL_MODE || 'require').trim();
const TRUST_CERT = process.env.E2E_SQLSERVER_TRUST_CERT !== '0';

const CONNECTION_ID = 'e2e-sqlserver-ui';
const CONNECTION_NAME = 'E2E SQL Server UI';
const COLUMNS = ['id', 'label', 'created_on', 'recorded_at', 'amount', 'payload', 'note'];
const ROW_COUNT = 120;
const SCREENSHOT_DIR = 'e2e/screenshots/sqlserver-driver-ui';
const RESUME_CODE = 'not currently available';
const FUNCTION_NAME = 'fn_NormalizeCode';

const scratchSchema = `dz_e2e_ui_${Date.now().toString(36)}`;
const scratchTableName = 'e2e_ui_rows';

interface MultiQueryPayload {
  results: Array<{ rows: Array<Array<string | number | boolean | null>> }>;
}

function skipReason(): string | null {
  if (process.env.E2E_SKIP_SQLSERVER === '1') return 'E2E_SKIP_SQLSERVER=1';
  if (!HOST || !USER || !PASSWORD) {
    return 'set E2E_SQLSERVER_HOST / E2E_SQLSERVER_USER / E2E_SQLSERVER_PASSWORD (see README.md)';
  }
  return null;
}

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

/** Text of every rendered data-grid cell, in row order. */
async function gridCells(): Promise<string[]> {
  return browser.execute(() =>
    Array.from(document.querySelectorAll('[data-testid="data-table-cell"]')).map((cell) =>
      (cell.textContent || '').trim(),
    ),
  );
}

/**
 * Text of every rendered column header.
 *
 * The grid header is a div row (not a `<thead>`); its first cell is the `#`
 * row-number gutter, so the header row is the parent of that cell.
 */
async function gridHeaders(): Promise<string[]> {
  return browser.execute(() => {
    const gutter = Array.from(document.querySelectorAll('div')).find(
      (el) => el.children.length === 0 && (el.textContent || '').trim() === '#',
    );
    const row = gutter?.parentElement;
    if (!row) return [];
    return Array.from(row.children)
      .slice(1)
      .map((cell) => (cell.textContent || '').trim());
  });
}

describe('SQL Server driver GUI (live)', () => {
  let dbSessionId = '';
  let reason: string | null = null;

  const run = (sql: string) =>
    invoke<MultiQueryPayload>('execute_query', {
      dbSessionId,
      sql,
      ...(DATABASE ? { database: DATABASE } : {}),
    });

  const setSafeMode = async (enabled: boolean) => {
    const settings = await invoke<Record<string, unknown>>('get_settings');
    if ((settings.safeMode !== false) === enabled) return;
    await invoke('save_settings', { settings: { ...settings, safeMode: enabled } });
  };

  const dropScratch = async () => {
    for (const statement of [
      `DROP FUNCTION IF EXISTS [${scratchSchema}].[${FUNCTION_NAME}]`,
      `DROP TABLE IF EXISTS [${scratchSchema}].[${scratchTableName}]`,
      `DROP SCHEMA IF EXISTS [${scratchSchema}]`,
    ]) {
      try {
        await run(statement);
      } catch (error) {
        console.warn(`ℹ️  cleanup "${statement}" reported: ${String(error)}`);
      }
    }
  };

  /**
   * Execute a selected SQL string through the actual SQL Editor gate, then
   * return whichever observable outcome it produced. Selecting the full text
   * avoids the editor's current-statement mode splitting routine bodies at
   * their internal semicolons before the SQL reaches the driver.
   */
  const executeSelectedSql = async (
    sql: string,
  ): Promise<{
    executionAdvanced: boolean;
    gateMessage: string | null;
    queryError: string | null;
  }> => {
    await setEditorContent(sql);
    if (!(await selectSqlEditorSubstring(sql))) {
      throw new Error('the SQL Editor did not select the complete SQL text');
    }

    const queryPanel = await $('[data-testid="query-panel"]');
    const beforeSeq = Number((await queryPanel.getAttribute('data-execution-seq')) ?? '0');
    await $('[data-testid="editor-execute-button"]').click();
    await browser.waitUntil(
      async () => {
        const currentSeq = Number((await queryPanel.getAttribute('data-execution-seq')) ?? '0');
        const gate = await $('[data-testid="result-message-ok"]');
        const error = await $('[data-testid="query-error-message"]');
        const executeButton = await $('[data-testid="editor-execute-button"]');
        return (
          (currentSeq > beforeSeq && (await executeButton.isDisplayed().catch(() => false))) ||
          (await gate.isDisplayed().catch(() => false)) ||
          (await error.isDisplayed().catch(() => false))
        );
      },
      { timeout: 20000, timeoutMsg: 'the SQL Editor produced no execution or diagnostic result' },
    );

    const gateButton = await $('[data-testid="result-message-ok"]');
    let gateMessage: string | null = null;
    if (await gateButton.isDisplayed().catch(() => false)) {
      gateMessage = await $('body').getText();
      await gateButton.click();
    }
    const queryError = await browser.execute(() => {
      const error = Array.from(
        document.querySelectorAll<HTMLElement>('[data-testid="query-error-message"]'),
      ).find((element) => element.getClientRects().length > 0);
      return error?.innerText.trim() || null;
    });
    const afterSeq = Number((await queryPanel.getAttribute('data-execution-seq')) ?? '0');
    return { executionAdvanced: afterSeq > beforeSeq, gateMessage, queryError };
  };

  before(async function () {
    this.timeout(180_000);
    reason = skipReason();
    if (reason) {
      console.warn(`⏩ Skipping SQL Server live GUI spec: ${reason}`);
      this.skip();
    }

    // The frontend must be mounted before any IPC call; a cold Azure serverless
    // resume can also push `connect` past the default 30s script timeout.
    await ensureMainWindowForIpc();
    await browser.setTimeout({ script: 120_000 });
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

    // Seed the scratch table over IPC, then hand the session back so the grid is
    // opened through the navigator exactly like a user would.
    for (let attempt = 1; attempt <= 4 && !dbSessionId; attempt += 1) {
      try {
        dbSessionId = await invoke<string>('connect', { connectionId: CONNECTION_ID });
      } catch (error) {
        if (!String(error).toLowerCase().includes(RESUME_CODE) || attempt === 4) throw error;
        console.warn(`⏳ database resuming (attempt ${attempt}/4), retrying in 8s`);
        await browser.pause(8000);
      }
    }
    if (!dbSessionId) throw new Error('connect returned no dbSessionId');

    await setSafeMode(false);
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
        `${(i + 1) * 10}.5, 0x0${i % 10}a, ${i % 3 === 0 ? 'NULL' : `N'note ${i + 1}'`})`
      );
    }).join(', ');
    await run(
      `INSERT INTO [${scratchSchema}].[${scratchTableName}] ` +
        '([id], [label], [created_on], [recorded_at], [amount], [payload], [note]) ' +
        `VALUES ${values}`,
    );
    await invoke('disconnect', { dbSessionId });
    dbSessionId = '';
  });

  after(async function () {
    this.timeout(120_000);
    try {
      // Reusing the connection returns the session the UI opened.
      dbSessionId = await invoke<string>('connect', { connectionId: CONNECTION_ID });
      await dropScratch();
    } catch (error) {
      console.warn(`ℹ️  cleanup reported: ${String(error)}`);
    } finally {
      if (dbSessionId) await invoke('disconnect', { dbSessionId }).catch(() => undefined);
      await setSafeMode(true);
      await invoke('delete_connection', { id: CONNECTION_ID }).catch(() => undefined);
    }
  });

  it('opens the table data grid from the navigator and renders every column', async function () {
    this.timeout(180_000);

    // 1. Connect through the navigator card, like a user. The connection was
    //    persisted over IPC while the window was already mounted, so reload once
    //    for the sidebar to pick it up. The predicate swallows the
    //    "execution context destroyed" window a reload opens.
    await ensureMainWindowForIpc();
    const navMounted = async (): Promise<boolean> => {
      try {
        return (
          (await browser.execute(
            () => !!document.querySelector('[data-testid="workspace-nav-databases"]'),
          )) === true
        );
      } catch {
        return false;
      }
    };
    await browser.execute(() => location.reload());
    try {
      await browser.waitUntil(navMounted, {
        timeout: 60000,
        interval: 500,
        timeoutMsg: 'the workspace never re-mounted after the reload',
      });
    } catch {
      await ensureMainWindowForIpc();
      await browser.waitUntil(navMounted, {
        timeout: 30000,
        interval: 500,
        timeoutMsg: 'the workspace never re-mounted after the reload',
      });
    }
    await expandAllGroups();
    await browser.waitUntil(
      async () =>
        (await browser.execute(() => document.querySelectorAll('[data-conn-item]').length)) > 0,
      { timeout: 20000, timeoutMsg: 'the connection card never appeared in the navigator' },
    );
    await clickCardConnectButton(CONNECTION_NAME);
    await waitForConnectionToolbar();
    await expandConnectedConnectionInNavigator(CONNECTION_NAME);

    // 2. Expand <database> → <schema> → Tables → click the scratch table.
    await expandSchemaCategory('tables', scratchSchema, DATABASE || undefined);
    const treeDump = await browser.execute(() =>
      Array.from(document.querySelectorAll('[data-testid="schema-tree-node"]'))
        .map((node) => (node.textContent || '').trim())
        .slice(0, 80),
    );
    console.warn(`TREE: ${JSON.stringify(treeDump)}`);
    // 3. Open the table with a real WebDriver click (the tree is virtualized, so
    //    an in-page synthetic event can miss React's handlers).
    const nodeSelector = `[data-testid="schema-tree-node"][data-item-name="${scratchTableName}"]`;
    const tableNode = await $(nodeSelector);
    await tableNode.waitForExist({ timeout: 30000 });
    await tableNode.scrollIntoView({ block: 'center' });
    if (await tableNode.isDisplayed().catch(() => false)) {
      await tableNode.click();
    } else {
      await browser.execute((selector: string) => {
        const el = document.querySelector<HTMLElement>(selector);
        el?.scrollIntoView({ block: 'center' });
        el?.click();
      }, nodeSelector);
    }

    // 4. The data grid must mount — this is the path that used to fail with
    //    "Incorrect syntax near 'LIMIT'".
    await browser.waitUntil(
      async () =>
        (await browser.execute(
          () => document.querySelectorAll('[data-testid="data-table-cell"]').length,
        )) > 0,
      { timeout: 30000, timeoutMsg: 'the table data grid never rendered a row' },
    );

    const headers = await gridHeaders();
    console.warn(`HEADERS: ${JSON.stringify(headers)}`);
    const missing = COLUMNS.filter(
      (column) => !headers.some((header) => header.toLowerCase().includes(column)),
    );
    if (missing.length > 0) {
      throw new Error(
        `the data grid is missing columns ${missing.join(', ')} — headers: ${headers.join(' | ')}`,
      );
    }

    const cells = await gridCells();
    console.warn(`CELLS: ${JSON.stringify(cells.slice(0, 24))}`);
    if (cells.length < COLUMNS.length * 5) {
      throw new Error(`the grid rendered only ${cells.length} cells for ${ROW_COUNT} rows`);
    }
    for (const cell of cells) {
      if (cell.includes('Date(') || cell.includes('increments')) {
        throw new Error(`a temporal cell leaked tiberius debug output: ${cell}`);
      }
    }
    // The DATE column must reach the grid as wall-clock text: the host used to
    // round-trip it through `new Date()` and render a shifted instant with `Z`.
    if (!cells.includes('2026-03-01')) {
      throw new Error(
        `the DATE column never rendered as ISO text: ${JSON.stringify(cells.slice(0, 24))}`,
      );
    }
    if (!cells.some((cell) => /^\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}(\.\d+)?$/.test(cell))) {
      throw new Error(
        `the DATETIME2 column must render as zone-less wall clock: ${JSON.stringify(cells.slice(0, 24))}`,
      );
    }
    const shifted = cells.find((cell) => /(?:[zZ]|[+-]\d{2}:?\d{2})$/.test(cell));
    if (shifted) {
      throw new Error(`a zone-less datetime cell was converted to a UTC instant: ${shifted}`);
    }
  });

  it('pages to the next slice from the grid toolbar', async function () {
    this.timeout(120_000);
    const before = await gridCells();
    if (before.length === 0) throw new Error('the grid is empty before paging');

    const clicked = await browser.execute(() => {
      const next = Array.from(document.querySelectorAll('button[aria-label]')).find((button) => {
        const label = button.getAttribute('aria-label') || '';
        return label === 'Next' || label === '下一页';
      }) as HTMLButtonElement | undefined;
      if (!next || next.disabled) return false;
      next.click();
      return true;
    });
    if (!clicked) throw new Error('the grid exposes no enabled next-page button');

    await browser.waitUntil(
      async () => {
        const cells = await gridCells();
        return cells.length > 0 && cells[0] !== before[0];
      },
      { timeout: 20000, timeoutMsg: 'the grid never advanced to the next page' },
    );

    const after = await gridCells();
    // The first column is `id`, so the first rendered cell is the first row's id.
    if (after[0] !== '51') {
      throw new Error(`page 2 must start at id 51, got ${JSON.stringify(after.slice(0, 8))}`);
    }
    if (before[0] !== '1') {
      throw new Error(`page 1 must start at id 1, got ${before[0]}`);
    }
  });

  it('captures the rendered grid as evidence', async function () {
    this.timeout(60_000);
    mkdirSync(SCREENSHOT_DIR, { recursive: true });
    await browser.saveScreenshot(`${SCREENSHOT_DIR}/table-grid.png`);
  });

  it('creates and calls a scalar function with a T-SQL local parameter in the SQL Editor', async function () {
    this.timeout(120_000);

    // The `execute_query` IPC helper bypasses the SQL Editor's bind-parameter
    // gate, so this regression deliberately enters through the real UI. The
    // connection was seeded after the initial app mount; reload to refresh the
    // navigator, then connect through the same path a user follows.
    await ensureMainWindowForIpc();
    await browser.execute(() => location.reload());
    await browser.waitUntil(
      async () =>
        browser.execute(() => !!document.querySelector('[data-testid="workspace-nav-databases"]')),
      {
        timeout: 60000,
        timeoutMsg: 'the workspace did not remount after refreshing the navigator',
      },
    );
    await openConnectionsWorkspace();
    await expandAllGroups();
    await browser.waitUntil(
      async () =>
        browser.execute(
          (name: string) =>
            Array.from(document.querySelectorAll('[data-conn-item]')).some((item) =>
              (item.getAttribute('data-conn-name') || item.textContent || '').includes(name),
            ),
          CONNECTION_NAME,
        ),
      { timeout: 20000, timeoutMsg: 'the SQL Server connection did not appear in the navigator' },
    );
    await clickCardConnectButton(CONNECTION_NAME);
    await waitForConnectionToolbar();
    await openQueryTab();

    const createFunctionSql =
      `CREATE FUNCTION [${scratchSchema}].[${FUNCTION_NAME}]\n` +
      '(\n' +
      '    @value NVARCHAR(64)\n' +
      ')\n' +
      'RETURNS NVARCHAR(64)\n' +
      'AS\n' +
      'BEGIN\n' +
      '    RETURN UPPER(LTRIM(RTRIM(@value)));\n' +
      'END;';
    const createResult = await executeSelectedSql(createFunctionSql);
    expect(
      createResult.gateMessage,
      'T-SQL @value must not be treated as an unbound editor parameter',
    ).toBeNull();
    expect(
      createResult.queryError,
      'SQL Server must accept the scalar function definition',
    ).toBeNull();
    expect(createResult.executionAdvanced).toBe(true);

    const callSql = `SELECT [${scratchSchema}].[${FUNCTION_NAME}](N'  Mixed Code  ') AS [normalized]`;
    const callResult = await executeSelectedSql(callSql);
    expect(callResult.gateMessage).toBeNull();
    expect(callResult.queryError).toBeNull();
    expect(callResult.executionAdvanced).toBe(true);
    await browser.waitUntil(
      async () =>
        browser.execute(() =>
          Array.from(document.querySelectorAll<HTMLElement>('[data-testid="data-table-cell"]'))
            .filter((cell) => cell.getClientRects().length > 0)
            .some((cell) => cell.innerText.trim() === 'MIXED CODE'),
        ),
      { timeout: 15000, timeoutMsg: 'the scalar function did not return trimmed uppercase text' },
    );
  });

  it('shows a SQL Server missing-object diagnostic for invalid SQL in the editor', async function () {
    this.timeout(60_000);
    const missingObject = 'dz_e2e_intentionally_missing_object';
    const result = await executeSelectedSql(`SELECT * FROM [${scratchSchema}].[${missingObject}]`);

    expect(
      result.gateMessage,
      'invalid SQL must reach SQL Server instead of stopping at the bind gate',
    ).toBeNull();
    expect(result.executionAdvanced).toBe(true);
    expect(result.queryError).toContain(missingObject);
    expect(result.queryError).toMatch(/invalid object name/i);
  });
});
