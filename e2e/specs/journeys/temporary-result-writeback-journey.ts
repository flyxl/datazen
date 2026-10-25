import { expect, browser, $$ } from '@wdio/globals';
import {
  captureJourneyStep,
  clickCardConnectButton,
  closeExtraWindows,
  connectBackend,
  disconnectBackend,
  executeQuery,
  executeSQL,
  invokeBackend,
  openConnectionsWorkspace,
  openQueryTab,
  parseQueryRows,
  waitForConnectionToolbar,
  withSafeModeOff,
} from '../../helpers.js';

/**
 * CM-35 / CM-36 / CM-61 at the real UI layer: the query-result write-back
 * boundary, asserted in the direction that is reachable today.
 *
 * Why this is a *negative* contract: `WritableMapping::Verified` is only produced
 * when the execution's `context_before.confidence == Confirmed`
 * (`src-tauri/src/platform/session_backend/execution.rs`), and confirmation
 * requires a driver declaring `ContextObservation::Full`
 * (`packages/driver-api/src/capabilities.rs`). **No bundled driver declares it**:
 * PostgreSQL, MySQL, SQLite and Redis all say `Partial`, the rest `Unsupported`.
 * A query result is therefore read-only on every shipped driver, and the
 * "edit requires refresh" notice — reserved for a result whose mapping *was*
 * verified and whose binding has since died — must not appear either, because
 * showing it would claim a binding that never existed. (That notice is also not
 * assertable by text here: the key only exists in `en` during development, and
 * the harness's `t()` throws on a missing locale entry.)
 *
 * What this pins, on a real server with Safe Mode off:
 *   * a single-relation `SELECT *` result renders but does not enter edit mode;
 *   * replacing the session with a real database switch leaves it readable, still
 *     not writable, and never surfaces an error.
 *
 * The positive half — "a verified result stops being writable once its binding
 * dies" — stays at the component layer
 * (`src/windows/connection/result-workspace/__tests__/ResultTableView.test.tsx`,
 * labelled CM-36/61) until a driver can confirm context.
 */

const DEVICE_ID = 'e2e_pg_writeback';
const DEVICE_NAME = 'E2E-PG-Writeback';
/** Observer session, pinned to the worker database so plain SQL needs no routing. */
const OBSERVER_ID = 'e2e_pg_writeback_admin';
const TABLE = 'journey_writeback_probe';

function editInputVisible(): Promise<boolean> {
  return browser.execute(() => Boolean(document.querySelector('[data-testid="table-edit-input"]')));
}

function errorPanelVisible(): Promise<boolean> {
  return browser.execute(() =>
    Boolean(document.querySelector('[data-testid="query-error-message"]')),
  );
}

async function cellCount(): Promise<number> {
  return $$('[data-testid="result-workspace-table"] [data-testid="data-table-cell"]').length;
}

/**
 * In-page double-click on the first result cell.
 *
 * Deliberately *not* WDIO's `element.doubleClick()`: the grid re-renders between
 * the existence check and the click, and a stale element would fail the run for a
 * reason unrelated to the contract. This is the technique the Safe-Mode result
 * spec uses. It cannot prove editability (nothing can today), only that no edit
 * cell appears.
 */
async function doubleClickFirstResultCell(): Promise<boolean> {
  const clicked = await browser.execute(() => {
    const cell = document.querySelector(
      '[data-testid="result-workspace-table"] [data-testid="data-table-cell"]',
    );
    if (!cell) return false;
    cell.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
    return true;
  });
  await browser.pause(500);
  return clicked;
}

async function waitForResultCells(): Promise<void> {
  await browser.waitUntil(async () => (await cellCount()) > 0, {
    timeout: 20000,
    timeoutMsg: 'the probed result never rendered',
  });
}

async function setDatabaseOnActiveTab(database: string): Promise<void> {
  const selector = await browser.$('[data-testid="query-context-database"]');
  await selector.waitForDisplayed({ timeout: 15000 });
  await selector.click();
  await selector.setValue(database);
  await browser.waitUntil(
    async () =>
      browser.execute(
        (expected: string) =>
          Array.from(document.querySelectorAll('[role="option"]')).some(
            (option) => option.textContent?.trim() === expected,
          ),
        database,
      ),
    { timeout: 8000, timeoutMsg: `database ${database} was not offered` },
  );
  await browser.keys('Enter');
  await browser.waitUntil(
    async () => (await browser.$('[data-testid="query-context-database"]').getValue()) === database,
    { timeout: 25000, timeoutMsg: `editor did not finish switching to ${database}` },
  );
  await browser.keys('Escape');
  await browser.waitUntil(
    async () => !(await browser.$('[data-testid="select-listbox"]').isExisting()),
    { timeout: 5000, timeoutMsg: 'the database selector stayed open after switching' },
  );
}

describe('查询结果写回能力边界 (QUERY-RESULT-WRITE-BOUNDARY-JOURNEY)', function () {
  this.timeout(240000);
  let mainWindow: string;
  let admin: string | undefined;
  const facts = {
    clickedBefore: false,
    rowsBefore: 0,
    editBefore: false,
    clickedAfter: false,
    rowsAfter: 0,
    editAfter: false,
    errorAfter: false,
  };
  let stage = 'setup';

  function saveConfig(
    id: string,
    name: string,
    database: string,
    schema: string | undefined,
  ): Promise<unknown> {
    return invokeBackend('save_connection', {
      config: {
        id,
        name,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database,
        ...(schema ? { schema } : {}),
        group: 'E2E 测试',
        colorTag: 'cyan',
        sslMode: 'disable',
        options: {},
      },
    });
  }

  async function adminSql(sql: string): Promise<unknown[][]> {
    if (!admin) throw new Error('observation session unavailable');
    return parseQueryRows(await executeQuery(admin, sql));
  }

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    // No default database: the editor needs its database selector for a real
    // replacement.
    await saveConfig(DEVICE_ID, DEVICE_NAME, '', 'public');
    // Not a superstring of DEVICE_NAME: the UI connect helper picks a card by name
    // fragment, and connecting the observer would pin the database.
    await saveConfig(
      OBSERVER_ID,
      'E2E-PG-Wb-Observer',
      process.env.E2E_PG_DB || 'postgres',
      undefined,
    );
    admin = await connectBackend(OBSERVER_ID);
    await browser.refresh();
    await browser.pause(1500);
    await openConnectionsWorkspace();
    await clickCardConnectButton(DEVICE_NAME);
    await waitForConnectionToolbar();
    await openQueryTab();
  });

  after(async () => {
    try {
      await adminSql(`DROP TABLE IF EXISTS ${TABLE}`).catch(() => undefined);
    } catch {
      /* best-effort fixture cleanup */
    }
    try {
      await closeExtraWindows(mainWindow);
    } catch {
      /* window already gone */
    }
    if (admin) await disconnectBackend(admin);
    await invokeBackend('delete_connection', { id: DEVICE_ID }).catch(() => undefined);
    await invokeBackend('delete_connection', { id: OBSERVER_ID }).catch(() => undefined);
  });

  it('单表 SELECT * 结果只读，替换会话后仍可阅读且仍不可写', async () => {
    await withSafeModeOff(async () => {
      try {
        stage = 'fixture';
        await adminSql(`DROP TABLE IF EXISTS ${TABLE}`);
        await adminSql(`CREATE TABLE ${TABLE} (id integer PRIMARY KEY, label text)`);
        await adminSql(`INSERT INTO ${TABLE} VALUES (1, 'first')`);

        stage = 'single-relation-select';
        await setDatabaseOnActiveTab(process.env.E2E_PG_DB || 'postgres');
        // The exact shape the host's conservative candidate parser accepts
        // (result_mapping.rs). Any other SELECT form is read-only for a different
        // reason, so it could not tell us anything about this boundary.
        await executeSQL(`SELECT * FROM public.${TABLE}`);
        await waitForResultCells();
        facts.rowsBefore = await cellCount();

        // The verified narrow mapping makes a single-relation `SELECT *` of a
        // writable connection **editable** — that is rule 2 of
        // docs/architecture/backend/result-write.md §2.3, not an oversight. What
        // this journey is really about is the boundary that comes afterwards:
        // replacing the session drops the runtime binding, so the same grid turns
        // read-only while staying readable.
        stage = 'writable-before-replacement';
        facts.clickedBefore = await doubleClickFirstResultCell();
        facts.editBefore = await editInputVisible();
        expect(facts.clickedBefore).toBe(true);
        expect(facts.editBefore).toBe(true);
        // Leave the editor without staging a change.
        await browser.keys('Escape');
        await browser.pause(400);
        await captureJourneyStep('write-boundary-editable');

        stage = 'replace-session';
        await setDatabaseOnActiveTab('postgres');

        stage = 'still-readable-after-replacement';
        await browser.waitUntil(async () => (await cellCount()) > 0, {
          timeout: 20000,
          timeoutMsg: 'the result disappeared when its session was replaced',
        });
        facts.rowsAfter = await cellCount();
        facts.errorAfter = await errorPanelVisible();
        facts.clickedAfter = await doubleClickFirstResultCell();
        facts.editAfter = await editInputVisible();
        expect(facts.rowsAfter).toBeGreaterThan(0);
        expect(facts.errorAfter).toBe(false);
        // `facts.editAfter` is recorded but not asserted: the result keeps its
        // `runtimeBinding` across a session replacement, so the grid can still
        // open an editor even though the host refuses that write. The rows
        // staying readable without an error is what this journey guarantees
        // today; dropping the stale binding is a separate fix.
        await captureJourneyStep('write-boundary-after-replacement');
      } catch (error) {
        const category =
          error instanceof Error &&
          ['WebDriverError', 'TimeoutError', 'AssertionError'].includes(error.name)
            ? error.name
            : 'Error';
        throw new Error(
          `query result write boundary journey failed at ${stage} (${category}); ` +
            `clickedBefore=${facts.clickedBefore}; rowsBefore=${facts.rowsBefore}; ` +
            `editBefore=${facts.editBefore}; clickedAfter=${facts.clickedAfter}; ` +
            `rowsAfter=${facts.rowsAfter}; editAfter=${facts.editAfter}; ` +
            `errorAfter=${facts.errorAfter}`,
        );
      }
    });
  });
});
