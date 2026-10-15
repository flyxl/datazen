import { expect, browser, $ } from '@wdio/globals';
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
  openQueryTab as createQueryTab,
  parseQueryRows,
  waitForConnectionToolbar,
  withSafeModeOff,
} from '../../helpers.js';

/**
 * CM-08 / CM-09 / CM-11 / CM-12 / CM-17 at the real UI layer: two editor tabs on
 * one connection own two runtime sessions, and everything that is session-scoped
 * really is session-scoped.
 *
 * The runtime already proves the resource/journey half with fixtures
 * (`cm08_cm15_cm16_replacement_journal_and_100_interleaved_reads`). What cannot
 * be proved there is that the *editor the user is looking at* keeps its own
 * observed context, that a temporary table created in one tab is invisible in
 * the other tab of the same connection, and that a statement failure never moves
 * the observed context. Every assertion below is read from the rendered UI or
 * from PostgreSQL itself through a second, independent session.
 */

const ISOLATION_ID = 'e2e_pg_isolation';
const ISOLATION_NAME = 'E2E-PG-Isolation';
/** Observation session: pinned to one database, so plain SQL needs no routing. */
const OBSERVER_ID = 'e2e_pg_isolation_admin';
const TABLE = 'journey_tx_probe';

function readFirstResultCell(): Promise<string | null> {
  return browser.execute(() => {
    const cell = document.querySelector('[data-testid="data-table-cell"]');
    return cell?.textContent?.trim() ?? null;
  });
}

function errorPanelVisible(): Promise<boolean> {
  return browser.execute(() =>
    Boolean(document.querySelector('[data-testid="query-error-message"]')),
  );
}

async function waitForResultValue(expected: string, timeout = 20000): Promise<void> {
  await browser.waitUntil(async () => (await readFirstResultCell()) === expected, {
    timeout,
    timeoutMsg: `expected the result grid to show "${expected}"`,
  });
}

async function closeQueryTabs(): Promise<void> {
  for (;;) {
    const count = await $$('[data-testid="panel-tab-close"]').length;
    if (count === 0) break;
    await browser.execute(() => {
      document.querySelector<HTMLButtonElement>('[data-testid="panel-tab-close"]')?.click();
    });
    await browser.waitUntil(
      async () => (await $$('[data-testid="panel-tab-close"]').length) < count,
      { timeout: 15000, timeoutMsg: 'a query tab did not close' },
    );
  }
}

async function selectTab(index: number): Promise<void> {
  await browser.execute((i: number) => {
    const tabs = document.querySelectorAll<HTMLElement>('[data-testid="panel-tab-select"]');
    tabs[i]?.click();
  }, index);
  await browser.pause(300);
}

/** Activate the query tab whose title matches; tabs are identified by index otherwise. */
async function tabCount(): Promise<number> {
  return $$('[data-testid="panel-tab-select"]').length;
}

async function setDatabaseOnActiveTab(database: string): Promise<void> {
  const selector = await $('[data-testid="query-context-database"]');
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
    async () => (await $('[data-testid="query-context-database"]').getValue()) === database,
    { timeout: 20000, timeoutMsg: `editor did not finish switching to ${database}` },
  );
  // Escape dismisses the searchable list; the editor's own input handling waits
  // for the listbox to be gone before it accepts typing, so leaving it open makes
  // the *next* step fail with a confusing "selector stayed open" error.
  await browser.keys('Escape');
  await browser.waitUntil(async () => !(await $('[data-testid="select-listbox"]').isExisting()), {
    timeout: 5000,
    timeoutMsg: 'the database selector stayed open after switching',
  });
}

describe('编辑器会话隔离完整旅程 (EDITOR-SESSION-ISOLATION-JOURNEY)', function () {
  this.timeout(240000);
  let mainWindow: string;
  let admin: string | undefined;
  const facts = { tempVisibleInOwner: '', tempErrorInSibling: false, contextA: '', contextB: '' };
  let stage = 'setup';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: {
        id: ISOLATION_ID,
        name: ISOLATION_NAME,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: '',
        group: 'E2E 测试',
        colorTag: 'green',
        sslMode: 'disable',
        options: {},
      },
    });
    await invokeBackend('save_connection', {
      config: {
        id: OBSERVER_ID,
        // Deliberately NOT a superstring of ISOLATION_NAME: the UI connect helper
        // picks a card by name *fragment*, so a prefix collision would connect the
        // observer's pinned database and suppress the editor's database selector.
        name: 'E2E-PG-Observer',
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: process.env.E2E_PG_DB || 'postgres',
        group: 'E2E 测试',
        colorTag: 'green',
        sslMode: 'disable',
        options: {},
      },
    });
    admin = await connectBackend(OBSERVER_ID);
    await browser.refresh();
    await browser.pause(1500);
    await openConnectionsWorkspace();
    await clickCardConnectButton(ISOLATION_NAME);
    await waitForConnectionToolbar();
    await createQueryTab();
  });

  after(async () => {
    try {
      await closeExtraWindows(mainWindow);
    } catch {
      /* window already gone */
    }
    if (admin) await disconnectBackend(admin);
    await invokeBackend('delete_connection', { id: ISOLATION_ID }).catch(() => undefined);
    await invokeBackend('delete_connection', { id: OBSERVER_ID }).catch(() => undefined);
  });

  async function adminScalar(sql: string): Promise<string> {
    if (!admin) throw new Error('observation session unavailable');
    const payload = await executeQuery(admin, sql);
    return String(parseQueryRows(payload)[0]?.[0] ?? '');
  }

  it('临时会话状态与观察上下文按 tab 隔离，失败语句不移动上下文', async () => {
    try {
      // ── CM-09: temporary state belongs to one session only. Both tabs stay on
      //    the same database, so a miss in the sibling cannot be blamed on the
      //    target database.
      stage = 'temp-state-own-tab';
      await executeSQL(`CREATE TEMP TABLE ${TABLE}_tmp (v integer)`);
      await executeSQL(`INSERT INTO ${TABLE}_tmp VALUES (42)`);
      await executeSQL(`SELECT v FROM ${TABLE}_tmp`);
      await waitForResultValue('42');
      facts.tempVisibleInOwner = (await readFirstResultCell()) ?? '';

      stage = 'temp-state-sibling-tab';
      await createQueryTab();
      await selectTab((await tabCount()) - 1);
      await executeSQL(`SELECT v FROM ${TABLE}_tmp`);
      await browser.waitUntil(async () => errorPanelVisible(), {
        timeout: 20000,
        timeoutMsg: 'the sibling tab did not report the missing temporary relation',
      });
      facts.tempErrorInSibling = true;
      // The sibling is still usable afterwards.
      await executeSQL('SELECT 1 AS sibling_alive');
      await waitForResultValue('1');
      await captureJourneyStep('editor-isolation-temp-state');

      // ── CM-08: give the second tab a different database and interleave reads.
      stage = 'two-contexts';
      await selectTab(0);
      await setDatabaseOnActiveTab(process.env.E2E_PG_DB || 'postgres');
      await executeSQL('SELECT current_database() AS ctx_a');
      facts.contextA = (await readFirstResultCell()) ?? '';
      await selectTab(1);
      await setDatabaseOnActiveTab('postgres');
      await executeSQL('SELECT current_database() AS ctx_b');
      facts.contextB = (await readFirstResultCell()) ?? '';
      expect(facts.contextA).not.toBe('');
      expect(facts.contextB).not.toBe('');
      expect(facts.contextA).not.toBe(facts.contextB);

      stage = 'interleaved-reads';
      for (let round = 0; round < 5; round += 1) {
        await selectTab(0);
        await executeSQL('SELECT current_database() AS ctx_a');
        await waitForResultValue(facts.contextA);
        await selectTab(1);
        await executeSQL('SELECT current_database() AS ctx_b');
        await waitForResultValue(facts.contextB);
      }
      // Each tab's selector still shows its own database (UI context independence).
      await selectTab(0);
      await expect(await $('[data-testid="query-context-database"]').getValue()).toBe(
        facts.contextA,
      );
      await selectTab(1);
      await expect(await $('[data-testid="query-context-database"]').getValue()).toBe(
        facts.contextB,
      );
      await captureJourneyStep('editor-isolation-two-contexts');

      // ── CM-11 / CM-12 at the UI layer: a qualified read and a *failing*
      //    statement must not move the observed context of either tab.
      stage = 'failure-does-not-move-context';
      await selectTab(0);
      await executeSQL('SELECT * FROM pg_catalog.pg_tables LIMIT 1');
      await executeSQL('SELECT * FROM journey_missing_relation_probe');
      await browser.waitUntil(async () => errorPanelVisible(), {
        timeout: 20000,
        timeoutMsg: 'the failing statement produced no visible error',
      });
      await executeSQL('SELECT current_database() AS ctx_after_failure');
      await waitForResultValue(facts.contextA);
      await expect(await $('[data-testid="query-context-database"]').getValue()).toBe(
        facts.contextA,
      );

      // ── CM-17: a hand-written transaction journey on the migrated path.
      //    Uncommitted rows must be invisible to an independent session, and a
      //    rollback must remove them for both.
      // Back on tab A: it holds the same database the observation session is
      // connected to, so "the other session cannot see the row" is a statement
      // about the transaction, not about two different databases.
      stage = 'manual-transaction';
      await selectTab(0);
      await withSafeModeOff(async () => {
        await executeSQL(`DROP TABLE IF EXISTS ${TABLE}`);
        await executeSQL(`CREATE TABLE ${TABLE} (v integer)`);
      });
      // CM-17's own form: the user writes BEGIN/ROLLBACK as SQL. The editor's
      // transaction heuristic asks before running inside an open transaction and
      // `executeSQL` answers that dialog, so this is the real user path rather
      // than a store-level shortcut.
      await executeSQL('BEGIN');
      await executeSQL(`INSERT INTO ${TABLE} VALUES (7)`);
      await executeSQL(`SELECT count(*)::int AS own FROM ${TABLE}`);
      await waitForResultValue('1');
      expect(await adminScalar(`SELECT count(*)::int FROM ${TABLE}`)).toBe('0');

      await executeSQL('ROLLBACK');
      await executeSQL(`SELECT count(*)::int AS after_rollback FROM ${TABLE}`);
      await waitForResultValue('0');
      expect(await adminScalar(`SELECT count(*)::int FROM ${TABLE}`)).toBe('0');
      await withSafeModeOff(async () => {
        await executeSQL(`DROP TABLE IF EXISTS ${TABLE}`);
      });
      await captureJourneyStep('editor-isolation-transaction');
    } catch (error) {
      const category =
        error instanceof Error &&
        ['WebDriverError', 'TimeoutError', 'AssertionError'].includes(error.name)
          ? error.name
          : 'Error';
      throw new Error(
        `editor isolation journey failed at ${stage} (${category}); ` +
          `tempOwn=${facts.tempVisibleInOwner}; tempSiblingError=${facts.tempErrorInSibling}; ` +
          `ctxA=${facts.contextA}; ctxB=${facts.contextB}`,
      );
    } finally {
      await adminScalar(`DROP TABLE IF EXISTS ${TABLE}`).catch(() => undefined);
      await closeQueryTabs().catch(() => undefined);
    }
  });
});
