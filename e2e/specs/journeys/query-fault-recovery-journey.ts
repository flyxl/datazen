import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  closeExtraWindows,
  connectBackend,
  connectSeededPgInWorkspace,
  disconnectBackend,
  executeQuery,
  executeSQL,
  invokeBackend,
  openQueryTab,
  parseQueryRows,
  setEditorContent,
  withSafeModeOff,
} from '../../helpers.js';

/**
 * P4 §12.1 — one continuous journey over a **real** driver and the real UI:
 * execute → partial rows → cancel → lose the physical backend → explicit
 * replacement session → execute again.
 *
 * The fault is injected by PostgreSQL itself (`pg_terminate_backend` on the
 * backend running the marked statement), not by a scripted barrier, and every
 * assertion is read from the rendered UI. This is the real-layer counterpart of
 * the runtime fixture journey
 * `published_prefix_disconnect_cancel_recover_missing_chunk_and_execute_again`;
 * that fixture proves the runtime keeps the published prefix, this one proves
 * the editor the user is looking at notices the loss, says so, recovers through
 * an explicit new session and executes again — and that the replacement is a
 * genuinely new physical backend rather than the old id transparently re-bound.
 *
 * Split-pane ("result view") close has no UI affordance yet (`openPane` /
 * `closePane` are store-only seams), so the release-on-close half of the
 * journey is covered by the panel-close specs instead of being faked here.
 */

/** A second, independent session used only to inject the fault. */
const ADMIN_CONNECTION_ID = 'e2e_pg_fault_admin';
const ADMIN_CONNECTION_NAME = 'E2E-PG-Fault-Admin';

function readResultCellCount(): Promise<number> {
  return browser.execute(() => document.querySelectorAll('[data-testid="data-table-cell"]').length);
}

function readFirstResultCell(): Promise<string | null> {
  return browser.execute(() => {
    const cell = document.querySelector('[data-testid="data-table-cell"]');
    return cell?.textContent?.trim() ?? null;
  });
}

/** Structural failure marker plus its length — the message itself is never echoed. */
function readErrorProbe(): Promise<{ present: boolean; length: number }> {
  return browser.execute(() => {
    const el = document.querySelector('[data-testid="query-error-message"]');
    const text = el?.textContent?.trim() ?? '';
    return { present: Boolean(el), length: text.length };
  });
}

/** The editor's explicit replacement affordance (no transparent re-connect). */
function readReplacementBanner(): Promise<boolean> {
  return browser.execute(() =>
    Boolean(document.querySelector('[data-testid="editor-new-session-button"]')),
  );
}

function stopButtonVisible(): Promise<boolean> {
  return $('[data-testid="editor-stop-button"]')
    .isDisplayed()
    .catch(() => false);
}

function inPageClick(selector: string): Promise<void> {
  return browser.execute((sel: string) => {
    const el = document.querySelector(sel);
    if (el instanceof HTMLElement) el.click();
  }, selector);
}

async function clickExecute(): Promise<void> {
  await $('[data-testid="editor-execute-button"]').click();
}

async function waitForStopButton(timeout: number): Promise<void> {
  await browser.waitUntil(async () => stopButtonVisible(), {
    timeout,
    timeoutMsg: 'the execute button never turned into a stop button',
  });
}

describe('查询故障恢复完整旅程 (QUERY-FAULT-RECOVERY-JOURNEY)', function () {
  this.timeout(180000);
  let mainWindow: string;
  let admin: string | undefined;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: {
        id: ADMIN_CONNECTION_ID,
        name: ADMIN_CONNECTION_NAME,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: process.env.E2E_PG_DB || 'postgres',
        group: 'E2E 测试',
        colorTag: 'red',
        sslMode: 'disable',
        options: {},
      },
    });
    admin = await connectBackend(ADMIN_CONNECTION_ID);
  });

  after(async () => {
    try {
      await closeExtraWindows(mainWindow);
    } catch {
      /* window already gone */
    }
    if (admin) await disconnectBackend(admin);
    try {
      await invokeBackend('delete_connection', { id: ADMIN_CONNECTION_ID });
    } catch {
      /* cleanup best-effort */
    }
  });

  it('执行→部分结果→取消→断线→替代 session→再次执行 后同一编辑器仍可用', async () => {
    const stamp = process.hrtime.bigint().toString(36);
    const marker = `dz_fault_${stamp}`;
    const facts = {
      partialRows: 0,
      cancelled: false,
      terminatedRows: 0,
      errorBeforeKill: false,
      errorPresent: false,
      errorLength: 0,
      bannerPresent: false,
      recoveredCell: '',
      backendBefore: '',
      backendAfter: '',
    };
    let stage = 'connect-ui';

    try {
      await withSafeModeOff(async () => {
        stage = 'connect-ui';
        await connectSeededPgInWorkspace();
        stage = 'open-query-tab';
        await openQueryTab();

        // ① Partial results: a multi-batch stream must land rows before anything dies.
        stage = 'partial-results';
        await executeSQL('SELECT generate_series(1, 600) AS fault_rows');
        await browser.waitUntil(async () => (await readResultCellCount()) > 0, {
          timeout: 15000,
          timeoutMsg: 'streamed rows never reached the result table',
        });
        facts.partialRows = await readResultCellCount();

        // ② Identity of the physical backend this editor session is on.
        stage = 'identity-before';
        await executeSQL(
          'SELECT (extract(epoch from backend_start) * 1000)::bigint::text AS fault_backend ' +
            'FROM pg_stat_activity WHERE pid = pg_backend_pid()',
        );
        await browser.waitUntil(async () => (await readFirstResultCell()) !== null, {
          timeout: 15000,
          timeoutMsg: 'backend identity never rendered',
        });
        facts.backendBefore = (await readFirstResultCell()) ?? '';
        await captureJourneyStep('query-fault-partial-rows');

        // ③ Cancel: a real, precise cancel of a running statement.
        stage = 'cancel';
        await setEditorContent('SELECT pg_sleep(30) AS fault_cancel_probe');
        await clickExecute();
        await waitForStopButton(15000);
        await inPageClick('[data-testid="editor-stop-button"]');
        await browser.waitUntil(async () => !(await stopButtonVisible()), {
          timeout: 20000,
          timeoutMsg: 'stop button never cleared after the cancel request',
        });
        facts.cancelled = true;
        facts.errorBeforeKill = (await readErrorProbe()).present;
        await captureJourneyStep('query-fault-cancelled');

        // ④ Disconnect: PostgreSQL ends the marked backend from an independent session.
        stage = 'disconnect';
        await setEditorContent(`SELECT pg_sleep(30) /* ${marker} */`);
        await clickExecute();
        await waitForStopButton(15000);
        if (!admin) throw new Error('fault-injection session unavailable');
        const terminated = await executeQuery(
          admin,
          `SELECT pg_terminate_backend(pid) AS terminated FROM pg_stat_activity ` +
            `WHERE query LIKE '%${marker}%' AND pid <> pg_backend_pid()`,
        );
        facts.terminatedRows = parseQueryRows(terminated).length;
        expect(facts.terminatedRows).toBeGreaterThan(0);

        // ⑤ The loss must be visible: the failure panel *and* the explicit
        //    replacement banner, never a silent end that looks like success.
        stage = 'failure-visible';
        await browser.waitUntil(
          async () => {
            const error = await readErrorProbe();
            return error.present || (await readReplacementBanner());
          },
          { timeout: 30000, timeoutMsg: 'killed execution left no visible failure marker' },
        );
        const error = await readErrorProbe();
        facts.errorPresent = error.present;
        facts.errorLength = error.length;
        facts.bannerPresent = await readReplacementBanner();
        // Both markers are the contract, not a fallback pair: the failure panel
        // says the statement did not succeed, the banner says the session is gone
        // and must be re-opened by the user. Asserting "either" would accept a
        // silent end that merely happens to leave one of them around.
        expect(facts.errorPresent).toBe(true);
        expect(facts.errorLength).toBeGreaterThan(0);
        expect(facts.bannerPresent).toBe(true);
        await browser.waitUntil(async () => !(await stopButtonVisible()), {
          timeout: 20000,
          timeoutMsg: 'stop button stayed visible after the backend was terminated',
        });
        await captureJourneyStep('query-fault-disconnected');

        // ⑥ Replacement is explicit: the user opens a new session; the old id is
        //    never silently re-bound.
        stage = 'replacement-session';
        await inPageClick('[data-testid="editor-new-session-button"]');
        await browser.waitUntil(async () => !(await readReplacementBanner()), {
          timeout: 25000,
          timeoutMsg: 'editor stayed disconnected after the explicit new session',
        });

        // ⑦ Execute again on the same editor tab.
        stage = 'execute-again';
        await executeSQL('SELECT 1 AS fault_recovered');
        await browser.waitUntil(async () => (await readFirstResultCell()) === '1', {
          timeout: 25000,
          timeoutMsg: 'the recovered execution produced no rows',
        });
        facts.recoveredCell = (await readFirstResultCell()) ?? '';

        // ⑧ The recovery must be a **new** physical backend: same backend_start
        //    would mean the dead session was reused rather than replaced.
        stage = 'identity-after';
        await executeSQL(
          'SELECT (extract(epoch from backend_start) * 1000)::bigint::text AS fault_backend ' +
            'FROM pg_stat_activity WHERE pid = pg_backend_pid()',
        );
        await browser.waitUntil(async () => (await readFirstResultCell()) !== null, {
          timeout: 15000,
          timeoutMsg: 'post-recovery backend identity never rendered',
        });
        facts.backendAfter = (await readFirstResultCell()) ?? '';
        expect(facts.backendAfter).not.toBe('');
        // Strictly later than the killed backend: the recovery ran on a *new*
        // physical backend, not on the dead session silently re-bound.
        expect(Number(facts.backendAfter)).toBeGreaterThan(Number(facts.backendBefore));
        await captureJourneyStep('query-fault-recovered');
      });
    } catch (error) {
      const category =
        error instanceof Error &&
        ['WebDriverError', 'TimeoutError', 'AssertionError', 'Error'].includes(error.name)
          ? error.name
          : 'Error';
      throw new Error(
        `P4 query fault recovery failed at ${stage} (${category}); ` +
          `partialRows=${facts.partialRows}; cancelled=${facts.cancelled}; ` +
          `terminatedRows=${facts.terminatedRows}; errorBeforeKill=${facts.errorBeforeKill}; ` +
          `errorPresent=${facts.errorPresent}; errorLength=${facts.errorLength}; ` +
          `bannerPresent=${facts.bannerPresent}; recoveredCell=${facts.recoveredCell}; ` +
          `backendChanged=${facts.backendAfter !== '' && facts.backendAfter !== facts.backendBefore}`,
      );
    }
  });
});
