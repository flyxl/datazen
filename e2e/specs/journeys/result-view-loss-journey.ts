import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  connectBackend,
  connectSeededPgInWorkspace,
  disconnectBackend,
  executeQuery,
  invokeBackend,
  openQueryTab,
  parseQueryRows,
  setEditorContent,
} from '../../helpers.js';

/**
 * CM-64 / CM-56 at the real UI layer: the result view going away is **not** a
 * cancel.
 *
 * `ContentView` unmounts the inactive panel when the user switches tabs, so
 * "关闭结果视图" has a real, user-reachable meaning without inventing a new
 * control: the consumer disappears while the statement keeps running. The
 * contract is threefold and all three are asserted here against PostgreSQL:
 *   1. the statement is still executing while nothing is subscribed;
 *   2. it runs to completion (a cancel on unsubscribe would leave it short);
 *   3. coming back shows the published result and starts **no** new execution.
 */

function readExecutionSeq(): Promise<number> {
  return browser.execute(() => {
    const panel = document.querySelector('[data-testid="query-panel"]');
    return Number(panel?.getAttribute('data-execution-seq') ?? '0');
  });
}

function stopButtonVisible(): Promise<boolean> {
  return $('[data-testid="editor-stop-button"]')
    .isDisplayed()
    .catch(() => false);
}

function mountedQueryPanels(): Promise<number> {
  return browser.execute(() => document.querySelectorAll('[data-testid="query-panel"]').length);
}

function errorPanelVisible(): Promise<boolean> {
  return browser.execute(() =>
    Boolean(document.querySelector('[data-testid="query-error-message"]')),
  );
}

describe('结果视图消失不取消执行且返回后不重投 (QUERY-CONSUMER-LOSS-JOURNEY)', function () {
  this.timeout(240000);
  const adminConnectionId = 'e2e_consumer_loss_admin';
  let mainWindow: string;
  let admin: string | undefined;
  const facts = {
    activeWhileUnsubscribed: false,
    seqBefore: -1,
    seqAfter: -1,
    rowsAfterReturn: false,
    errorAfterReturn: false,
  };
  let stage = 'setup';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: {
        id: adminConnectionId,
        name: 'E2E-Consumer-Loss-Admin',
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: process.env.E2E_PG_DB || 'postgres',
        group: 'E2E 测试',
        colorTag: 'orange',
        sslMode: 'disable',
        options: {},
      },
    });
    admin = await connectBackend(adminConnectionId);
  });

  after(async () => {
    try {
      await closeExtraWindows(mainWindow);
    } catch {
      /* window already gone */
    }
    if (admin) await disconnectBackend(admin);
    await invokeBackend('delete_connection', { id: adminConnectionId }).catch(() => undefined);
  });

  /**
   * Backends **executing** the marked statement right now.
   *
   * `state = 'active'` is load-bearing: a session keeps its last statement text
   * in `pg_stat_activity.query` after it finishes, so matching the text alone
   * would count a completed statement as a running one forever.
   */
  async function markedActiveBackends(marker: string): Promise<number> {
    if (!admin) throw new Error('observation session unavailable');
    const payload = await executeQuery(
      admin,
      `SELECT count(*)::int AS c FROM pg_stat_activity WHERE state = 'active' ` +
        `AND query LIKE '%${marker}%' AND pid <> pg_backend_pid()`,
    );
    return Number(parseQueryRows(payload)[0]?.[0] ?? -1);
  }

  it('切走标签页后语句继续执行到底，返回时结果恢复且不重新提交', async () => {
    const marker = `dz_consumer_${Date.now().toString(36)}`;
    try {
      stage = 'connect-ui';
      await connectSeededPgInWorkspace();
      stage = 'open-query-tab';
      await openQueryTab();

      stage = 'start-slow-stream';
      // ~12s of server work: long enough to switch tabs and observe, short enough
      // for a real regression run.
      await setEditorContent(
        `SELECT n, pg_sleep(0.04) AS nap FROM generate_series(1, 300) n /* ${marker} */`,
      );
      await $('[data-testid="editor-execute-button"]').click();
      await browser.waitUntil(async () => stopButtonVisible(), {
        timeout: 20000,
        timeoutMsg: 'the marked statement never started running',
      });
      // Read the submission counter *after* the click: it identifies this run, and
      // it must still be this run's value when the pane comes back.
      facts.seqBefore = await readExecutionSeq();

      stage = 'consumer-goes-away';
      await openQueryTab();
      await browser.waitUntil(async () => (await mountedQueryPanels()) === 1, {
        timeout: 15000,
        timeoutMsg: 'the inactive query panel stayed mounted after switching tabs',
      });
      expect(await $$('[data-testid="panel-tab-select"]').length).toBeGreaterThanOrEqual(2);

      stage = 'still-executing-while-unsubscribed';
      await browser.waitUntil(async () => (await markedActiveBackends(marker)) > 0, {
        timeout: 10000,
        timeoutMsg: 'the statement was gone as soon as the result view disappeared',
      });
      facts.activeWhileUnsubscribed = true;

      stage = 'runs-to-completion';
      await browser.waitUntil(async () => (await markedActiveBackends(marker)) === 0, {
        timeout: 90000,
        timeoutMsg: 'the statement never finished while unsubscribed',
      });
      await captureJourneyStep('query-consumer-loss-unsubscribed');

      stage = 'return-to-tab';
      await browser.execute(() => {
        document.querySelectorAll<HTMLElement>('[data-testid="panel-tab-select"]')[0]?.click();
      });
      await browser.waitUntil(async () => (await mountedQueryPanels()) === 1, {
        timeout: 15000,
        timeoutMsg: 'the first query panel did not come back',
      });
      facts.seqAfter = await readExecutionSeq();
      facts.errorAfterReturn = await errorPanelVisible();

      stage = 'results-recovered';
      await browser.waitUntil(
        async () => (await $('body').getText()).includes(`300 ${t('common.rows')}`),
        { timeout: 30000, timeoutMsg: 'the full result set was not recovered after returning' },
      );
      facts.rowsAfterReturn = true;

      // No resubmission: the execution sequence counter is the same one the
      // original submission produced.
      expect(facts.seqAfter).toBe(facts.seqBefore);
      expect(facts.errorAfterReturn).toBe(false);
      await captureJourneyStep('query-consumer-loss-recovered');
    } catch (error) {
      const category =
        error instanceof Error &&
        ['WebDriverError', 'TimeoutError', 'AssertionError'].includes(error.name)
          ? error.name
          : 'Error';
      throw new Error(
        `consumer loss journey failed at ${stage} (${category}); ` +
          `activeWhileUnsubscribed=${facts.activeWhileUnsubscribed}; seqBefore=${facts.seqBefore}; ` +
          `seqAfter=${facts.seqAfter}; rowsRecovered=${facts.rowsAfterReturn}; ` +
          `errorAfterReturn=${facts.errorAfterReturn}`,
      );
    }
  });
});
