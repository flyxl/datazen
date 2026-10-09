import { expect, browser, $, $$ } from '@wdio/globals';
import {
  captureJourneyStep,
  closeExtraWindows,
  disconnectBackend,
  invokeBackend,
  openDataTransferWindow,
  queryScalar,
  selectDzOptionInWrap,
  withSafeModeOff,
  type QueryResultPayload,
} from '../helpers.js';

/**
 * Data Transfer P5 Job chain, end to end (DTJ-001~DTJ-003).
 *
 * Every spec here drives the real `prepare_data_transfer_job` →
 * `apply_data_transfer_job` path against a live PG→PG pair and asserts the
 * verdict the Job replies with — not a mocked one.
 *
 * Requires `e2e/setup-sync-dbs.sh` to have created `datazen_sync_src` /
 * `datazen_sync_tgt`, and a writable PG reachable at `E2E_PG_*`.
 */
describe('数据传输 Job 闭环 (DTJ)', () => {
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_job_src_${STAMP}`;
  const TGT_ID = `e2e_job_tgt_${STAMP}`;
  const SRC_NAME = `JobSrc-${STAMP}`;
  const TGT_NAME = `JobTgt-${STAMP}`;
  /**
   * Source and target are distinct PostgreSQL databases, so the same relation
   * name is safe and is a useful regression case for endpoint identity. A
   * table name by itself does not make a cross-database transfer a self-write.
   */
  const JOB_TABLE = `xfer_job_${STAMP}`;
  /** Bulk fixture: keeps the apply in flight long enough to observe a cancel
   *  while the apply is still in flight. The same name exists in two databases. */
  const BULK_TABLE = `xfer_bulk_${STAMP}`;
  const BULK_SLOW_FN = `xfer_bulk_slow_${STAMP}`;
  const BULK_ROWS = 30000;
  const ROWS_SELECTOR = '[data-testid="data-transfer-table-row"]';

  /** Receipt identity of the Job DTJ-001 spent, handed to DTJ-002. */
  let spentPlan: {
    jobId: string;
    planId: string;
    planDigest: string;
    selectionRevision: number;
  } | null = null;

  const pgConfig = (id: string, name: string, database: string) => ({
    id,
    name,
    databaseType: 'postgresql',
    host: process.env.E2E_PG_HOST || '127.0.0.1',
    port: Number(process.env.E2E_PG_PORT) || 5432,
    username: process.env.E2E_PG_USER || 'postgres',
    password: process.env.E2E_PG_PASSWORD || '',
    database,
    sslMode: 'disable',
  });

  const dropTable = async (dbSessionId: string, table: string): Promise<void> => {
    await invokeBackend('execute_query', {
      dbSessionId,
      sql: `DROP TABLE IF EXISTS ${table}`,
    });
  };

  /** Row count of `table` in the target database, read through the real driver. */
  async function targetRowCount(table: string): Promise<number> {
    const session = (await invokeBackend<string>('connect', { connectionId: TGT_ID })) ?? '';
    expect(session).toBeTruthy();
    try {
      const rows = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: session,
        sql: `SELECT count(*)::int AS c FROM ${table}`,
      });
      return Number(queryScalar(rows, 'c'));
    } finally {
      await disconnectBackend(session);
    }
  }

  type E2eJobView = {
    jobId: string;
    state: 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';
    cancelRequested: boolean;
    progress: { read: number | string; converted: number | string; committed: number | string };
  };

  async function readJob(jobId: string): Promise<E2eJobView> {
    return invokeBackend<E2eJobView>('get_job', { jobId });
  }

  async function waitForTerminalJob(jobId: string, timeout = 180000): Promise<E2eJobView> {
    let latest: E2eJobView | null = null;
    await browser.waitUntil(
      async () => {
        latest = await readJob(jobId);
        return latest.state !== 'queued' && latest.state !== 'running';
      },
      { timeout, interval: 500, timeoutMsg: `Job ${jobId} did not settle` },
    );
    if (!latest) throw new Error(`Job ${jobId} did not return a terminal projection`);
    return latest;
  }

  async function exists(selector: string): Promise<boolean> {
    return (await $(selector)).isExisting().catch(() => false);
  }

  async function selectTransferEndpoints(): Promise<void> {
    await selectDzOptionInWrap('data-transfer-source', SRC_NAME);
    await selectDzOptionInWrap('data-transfer-target', TGT_NAME);
    await browser.pause(1500);
  }

  /**
   * Inspect lists every table the source database happens to hold, and it lists
   * them all *unchecked*: `inspect_data_transfer` calls `inspect_tables` with an
   * empty target-table list, so `effective_table_mappings` never takes the
   * `TableMapping::auto` branch and returns `enabled: false` for each row. The
   * objects gate (`tables.some(tbl => tbl.enabled)`) therefore only opens once
   * the user ticks a row — so tick `keep` and untick the rest. An unrelated table
   * left on would either fail the apply or, worse, half-succeed and leave
   * `completed=false` for reasons that have nothing to do with this spec.
   */
  async function selectOnlyTable(keep: string, scenario: string): Promise<void> {
    const rows = await $$(ROWS_SELECTOR);
    const listed: string[] = [];
    let keepBox: ChainablePromiseElement | null = null;
    for (const row of rows) {
      const text = (await row.getText()).replace(/\s+/g, ' ').trim();
      listed.push(text);
      const box = await row.$('input[type="checkbox"]');
      if (!box) continue;
      if (text.includes(keep)) {
        keepBox = box;
        continue;
      }
      if (await box.isSelected()) {
        await box.click();
        await browser.pause(300);
      }
    }
    // Deselecting is not a state a bare "Next stayed disabled" can explain, so
    // name the inspected list here instead of letting the walk die later.
    if (!keepBox) {
      const count = await rows.length;
      throw new Error(
        `${scenario}: inspect listed ${count} table(s) [${listed.join(' | ')}], none named ${keep}`,
      );
    }
    if (!(await keepBox.isSelected())) {
      await keepBox.click();
      await browser.pause(300);
    }
    const checked = await $$(ROWS_SELECTOR);
    let enabled = 0;
    for (const row of checked) {
      const box = await row.$('input[type="checkbox"]');
      if (box && (await box.isSelected())) enabled += 1;
    }
    if (enabled !== 1) {
      throw new Error(
        `${scenario}: expected exactly ${keep} enabled, ${enabled} row(s) are ticked [${listed.join(' | ')}]`,
      );
    }
  }

  /**
   * Why the walk is stuck, as facts the next reader can act on.
   *
   * A bare "button stayed disabled" hides the one thing that matters: whether
   * the list is empty, whether the rows the walk needs exist and are checked,
   * and what the window is actually showing. Cheap enough to build on failure.
   */
  async function describeStep(scenario: string, label: string): Promise<string> {
    const facts: string[] = [];
    try {
      const rows = await $$(ROWS_SELECTOR);
      facts.push(`rows=${await rows.length}`);
      const listed: string[] = [];
      let checked = 0;
      for (const row of rows) {
        listed.push((await row.getText()).replace(/\s+/g, ' ').trim());
        const box = await row.$('input[type="checkbox"]');
        if (box && (await box.isSelected())) checked += 1;
      }
      facts.push(`checked=${checked}`, `listed=[${listed.slice(0, 8).join(' | ')}]`);
      const empty = await $('[data-testid="data-transfer-objects-empty"]');
      facts.push(`objectsEmpty=${await empty.isExisting()}`);
      const win = await $('[data-testid="data-transfer-window"]');
      if (await win.isExisting()) {
        facts.push(`window="${(await win.getText()).replace(/\s+/g, ' ').trim().slice(0, 240)}"`);
      }
    } catch (e) {
      facts.push(`describe failed: ${e instanceof Error ? e.message : String(e)}`);
    }
    return `${scenario}: Next stayed disabled while leaving the ${label} step [${facts.join('; ')}]`;
  }

  /**
   * Endpoints → setup → objects → mapping → preview, without executing.
   *
   * The step order is fixed, so this walks it explicitly. Every hop waits for
   * the marker element of the step it just entered: a Next click only
   * dispatches, and the step flips after the handler's own `await` (inspect /
   * prepare) settles, so firing the next click blind lands it on the step the
   * app has not left yet and the run stalls with no clue where.
   */
  async function driveToPreview(only: string, target: string, scenario: string): Promise<void> {
    await selectTransferEndpoints();

    const waitForAny = async (selectors: string[], timeoutMs: number, what: string) => {
      await browser.waitUntil(
        async () => {
          for (const selector of selectors) {
            if (await exists(selector)) return true;
          }
          return false;
        },
        { timeout: timeoutMs, interval: 300, timeoutMsg: `${scenario}: ${what}` },
      );
    };

    const clickNext = async (label: string, arrived: string[]): Promise<void> => {
      const next = await $('[data-testid="data-transfer-next"]');
      // Polled by hand rather than through `waitForClickable`/`waitUntil` so
      // the failure can carry `describeStep`'s facts instead of only
      // "not clickable", which cannot say what the window is showing.
      const deadline = Date.now() + 60000;
      while (Date.now() < deadline) {
        if ((await next.isExisting()) && (await next.isEnabled())) break;
        await browser.pause(300);
      }
      if (!((await next.isExisting()) && (await next.isEnabled()))) {
        throw new Error(await describeStep(scenario, label));
      }
      await next.click();
      await waitForAny(arrived, 60000, `never reached the ${label} step`);
      await captureJourneyStep(`${scenario}-${label}`);
    };

    await clickNext('setup', ['[data-testid="data-transfer-write-mode"]']);
    // Entering the objects step is what triggers `inspect`; Next stays disabled
    // there until rows arrive, so the list must be awaited before going on.
    await clickNext('objects', [ROWS_SELECTOR, '[data-testid="data-transfer-objects-empty"]']);
    await selectOnlyTable(only, scenario);
    await captureJourneyStep(`${scenario}-selection`);

    await clickNext('mapping', ['[data-testid="data-transfer-mapping-step"]']);

    // `canNext('mapping')` asks for a table with an *active* column mapping, and
    // an active mapping needs two things the window does not guess for the user:
    // a committed target table (only the blur re-inspects its columns) and an
    // explicit auto-match. Fixture tables share column names, so auto-match
    // pairs every column and the gate opens.
    const targetInput = await $('[data-testid="data-transfer-target-table-input"]');
    await targetInput.waitForDisplayed({ timeout: 30000 });
    await targetInput.setValue(target);
    const typed = await targetInput.getValue();
    if (!typed.includes(target)) {
      throw new Error(
        `${scenario}: target table input did not take "${target}" (holds "${typed}")`,
      );
    }
    // Focus loss is the commit; clicking the sidebar entry for the same table
    // keeps the selection and blurs the input.
    await (await $('[data-testid="data-transfer-mapping-table-item"]')).click();

    // That blur only *starts* an async re-inspect; until it lands the editor has
    // no target columns and auto-match maps everything to `skip`. Auto-match is
    // idempotent, so click it again each second until the gate opens instead of
    // guessing which side of the inspect this landed on.
    const autoMatch = await $('[data-testid="data-transfer-auto-match"]');
    const gate = await $('[data-testid="data-transfer-next"]');
    let mapped = false;
    for (let i = 0; i < 60; i++) {
      if (await gate.isEnabled()) {
        mapped = true;
        break;
      }
      if (await autoMatch.isEnabled()) await autoMatch.click();
      await browser.pause(1000);
    }
    if (!mapped) {
      throw new Error(`${scenario}: ${only} → ${target} never reached an active column mapping`);
    }
    await captureJourneyStep(`${scenario}-mapping-ready`);

    // This click is what runs `prepare`. It may legitimately refuse (out of
    // backend scope, or over the 8 MiB pipeline budget); the refusal has to be
    // named instead of showing up later as "execute never enabled". The source
    // and target databases are distinct, so matching relation names are valid.
    const next = await $('[data-testid="data-transfer-next"]');
    const prepareDeadline = Date.now() + 60000;
    while (Date.now() < prepareDeadline) {
      if ((await next.isExisting()) && (await next.isEnabled())) break;
      await browser.pause(300);
    }
    if (!((await next.isExisting()) && (await next.isEnabled()))) {
      throw new Error(await describeStep(scenario, 'mapping (prepare)'));
    }
    await next.click();
    await waitForAny(
      ['[data-testid="data-transfer-preview"]', '[data-testid="data-transfer-preview-error"]'],
      120000,
      'prepare never settled a preview',
    );
    if (await exists('[data-testid="data-transfer-preview-error"]')) {
      const notice = await (await $('[data-testid="data-transfer-preview-error"]')).getText();
      throw new Error(`${scenario}: prepare was refused: ${notice.trim()}`);
    }
    await captureJourneyStep(`${scenario}-preview`);

    const execute = await $('[data-testid="data-transfer-execute"]');
    await execute.waitForDisplayed({ timeout: 60000 });
    // The plan is admitted by `prepare` before the preview step opens, but React
    // commits that state a tick later. Wait until the affordance is armed so a
    // click can never land on a still-disabled button — a refusal leaves it
    // disabled on purpose, and that is a failure worth naming.
    for (let i = 0; i < 30; i++) {
      if (await execute.isEnabled()) return;
      await browser.pause(1000);
    }
    throw new Error(`execute stayed disabled at preview for ${scenario}`);
  }

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);

    await invokeBackend('save_connection', {
      config: pgConfig(SRC_ID, SRC_NAME, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(TGT_ID, TGT_NAME, 'datazen_sync_tgt'),
    });
    const srcSession = await invokeBackend<string>('connect', { connectionId: SRC_ID });
    const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });

    try {
      await withSafeModeOff(async () => {
        await dropTable(srcSession, JOB_TABLE);
        await dropTable(tgtSession, JOB_TABLE);
        await dropTable(srcSession, BULK_TABLE);
        await dropTable(tgtSession, BULK_TABLE);

        await invokeBackend('execute_query', {
          dbSessionId: srcSession,
          sql: `CREATE TABLE ${JOB_TABLE} (id int PRIMARY KEY, name text NOT NULL, qty int)`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: srcSession,
          sql: `INSERT INTO ${JOB_TABLE} (id, name, qty) VALUES (1,'a',10),(2,'b',20),(3,'c',30)`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: tgtSession,
          sql: `CREATE TABLE ${JOB_TABLE} (id int PRIMARY KEY, name text NOT NULL, qty int)`,
        });

        // Narrow payload on purpose: the backend refuses a pipeline over 8 MiB,
        // and this fixture only needs to be big enough to keep the Job in flight.
        await invokeBackend('execute_query', {
          dbSessionId: srcSession,
          sql: `CREATE TABLE ${BULK_TABLE} (id int PRIMARY KEY, qty int)`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: srcSession,
          sql: `INSERT INTO ${BULK_TABLE} (id, qty) SELECT g, g * 2 FROM generate_series(1, ${BULK_ROWS}) AS g`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: tgtSession,
          sql: `CREATE TABLE ${BULK_TABLE} (id int PRIMARY KEY, qty int)`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: tgtSession,
          sql: `CREATE FUNCTION ${BULK_SLOW_FN}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.001); RETURN NEW; END $$`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: tgtSession,
          sql: `CREATE TRIGGER ${BULK_SLOW_FN}_trigger BEFORE INSERT ON ${BULK_TABLE} FOR EACH ROW EXECUTE FUNCTION ${BULK_SLOW_FN}()`,
        });
      });
    } finally {
      await disconnectBackend(srcSession);
      await disconnectBackend(tgtSession);
    }

    await closeExtraWindows(mainWindow);
  });

  after(async () => {
    try {
      const srcSession = await invokeBackend<string>('connect', { connectionId: SRC_ID });
      const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });
      try {
        await withSafeModeOff(async () => {
          await dropTable(srcSession, JOB_TABLE);
          await dropTable(srcSession, BULK_TABLE);
          await dropTable(tgtSession, JOB_TABLE);
          await dropTable(tgtSession, BULK_TABLE);
          await invokeBackend('execute_query', {
            dbSessionId: tgtSession,
            sql: `DROP FUNCTION IF EXISTS ${BULK_SLOW_FN}()`,
          });
        });
      } finally {
        await disconnectBackend(srcSession);
        await disconnectBackend(tgtSession);
      }
    } catch {
      /* teardown is best effort */
    }
    try {
      await invokeBackend('delete_connection', { id: SRC_ID });
    } catch {}
    try {
      await invokeBackend('delete_connection', { id: TGT_ID });
    } catch {}
    await closeExtraWindows(mainWindow);
  });

  it('DTJ-001: 成功结果不重跑，执行中关窗重开仍附着同一后台 Job', async () => {
    await openDataTransferWindow();
    await driveToPreview(JOB_TABLE, JOB_TABLE, 'dtj1');

    const cancel = await $('[data-testid="data-transfer-cancel"]');
    // A cancel before the apply is addressable: the prepare Job id is ours.
    expect(await cancel.getAttribute('data-cancel-addressable')).toBe('true');

    const execute = await $('[data-testid="data-transfer-execute"]');
    await execute.waitForClickable({ timeout: 20000 });
    await execute.click();
    const acceptedApply = await $('[data-testid="data-transfer-job-active-state"]');
    await acceptedApply.waitForDisplayed({ timeout: 30000 });
    const spentJobId = await acceptedApply.getAttribute('data-job-id');
    expect(spentJobId).toBeTruthy();
    await captureJourneyStep('dtj1-executed');

    const result = await $('[data-testid="data-transfer-result"]');
    await result.waitForDisplayed({ timeout: 120000 });
    await browser.waitUntil(async () => (await result.getAttribute('data-completed')) === 'true', {
      timeout: 30000,
      timeoutMsg: 'completed transfer did not publish its terminal verdict',
    });
    expect(await result.getAttribute('data-verdict-severity')).toBe('ok');
    expect(await result.getAttribute('data-replayed')).toBe('false');
    expect(await result.getAttribute('data-requires-reconcile')).toBe('false');

    const verdict = await $('[data-testid="data-transfer-job-verdict"]');
    await expect(verdict).toBeDisplayed();
    // PG→PG with a PK on both sides is snapshot-proven, so the verdict has
    // evidence to verify and may be a plain `ok`.
    expect(await verdict.getAttribute('data-uncertainty')).toBe('none');
    expect(await verdict.getAttribute('data-cancel-disposition')).toBe('none');
    expect(Number(await verdict.getAttribute('data-unverified-boundaries'))).toBe(0);
    const verified = Number(await verdict.getAttribute('data-verified-boundaries'));
    expect(verified).toBeGreaterThanOrEqual(1);
    expect(await exists('[data-testid="data-transfer-job-verdict-boundaries-empty"]')).toBe(false);

    const planId = await result.getAttribute('data-plan-id');
    const planDigest = await verdict.getAttribute('data-plan-digest');
    const revisionRaw = await verdict.getAttribute('data-selection-revision');
    expect(planId).toBeTruthy();
    expect(planDigest).toBeTruthy();
    expect(Number(revisionRaw)).toBeGreaterThanOrEqual(0);
    spentPlan = {
      jobId: spentJobId ?? '',
      planId: planId ?? '',
      planDigest: planDigest ?? '',
      selectionRevision: Number(revisionRaw),
    };

    expect(await targetRowCount(JOB_TABLE)).toBe(3);

    // A reopened window must not re-drive the Job that already settled.
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await openDataTransferWindow();
    await captureJourneyStep('dtj1-reopened');

    expect(await exists('[data-testid="data-transfer-execute"]')).toBe(false);

    const restoredResult = await $('[data-testid="data-transfer-result"][data-restored="true"]');
    await browser.waitUntil(
      async () =>
        (await $('[data-testid="data-transfer-result"][data-restored="true"]').getAttribute(
          'data-job-id',
        )) === spentPlan?.jobId,
      {
        timeout: 30000,
        interval: 250,
        timeoutMsg: 'reopened window did not restore the settled Job report',
      },
    );
    expect(await restoredResult.getAttribute('data-completed')).toBe('true');
    expect(await restoredResult.getAttribute('data-plan-id')).toBe(spentPlan?.planId);
    const restoredVerdict = await $('[data-testid="data-transfer-job-verdict"]');
    expect(await restoredVerdict.getAttribute('data-plan-digest')).toBe(spentPlan?.planDigest);
    expect(await restoredVerdict.getAttribute('data-selection-revision')).toBe(
      String(spentPlan?.selectionRevision),
    );
    expect(Number(await restoredVerdict.getAttribute('data-verified-boundaries'))).toBeGreaterThan(
      0,
    );

    // Re-driving only reaches a fresh prepare — a plan, never a write.
    await driveToPreview(JOB_TABLE, JOB_TABLE, 'dtj1-reopen');
    expect(await targetRowCount(JOB_TABLE)).toBe(3);

    // Start a deliberately slow bulk write, close its window after admission,
    // and re-open the window while the same backend Job is still running.
    await closeExtraWindows(mainWindow);
    await openDataTransferWindow();
    await driveToPreview(BULK_TABLE, BULK_TABLE, 'dtj1-background');
    await (await $('[data-testid="data-transfer-execute"]')).click();
    const activeState = await $('[data-testid="data-transfer-job-active-state"]');
    await activeState.waitForDisplayed({ timeout: 30000 });
    const applyJobId = await activeState.getAttribute('data-job-id');
    expect(applyJobId).toBeTruthy();
    const activeResult = await $('[data-testid="data-transfer-result"]');
    expect(await activeResult.getAttribute('data-verdict-severity')).toBe('active');
    expect(await activeResult.getAttribute('data-completed')).toBe('false');
    const activeCancel = await $('[data-testid="data-transfer-cancel"]');
    expect(await activeCancel.getAttribute('data-cancel-addressable')).toBe('true');
    expect(await activeCancel.getAttribute('data-job-id')).toBe(applyJobId);
    expect(['queued', 'running']).toContain((await readJob(applyJobId ?? '')).state);

    await captureJourneyStep('dtj1-background-before-close');
    await closeExtraWindows(mainWindow);
    await openDataTransferWindow();
    const attached = await $('[data-testid="data-transfer-attached-job"]');
    await browser.waitUntil(
      async () =>
        (await $('[data-testid="data-transfer-attached-job"]').getAttribute('data-job-id')) ===
        applyJobId,
      {
        timeout: 30000,
        interval: 250,
        timeoutMsg: 'reopened window did not hydrate the active apply Job',
      },
    );
    const attachedState = await $('[data-testid="data-transfer-attached-job-state"]');
    expect(['queued', 'running']).toContain(await attached.getAttribute('data-state'));
    expect(await attachedState.isDisplayed()).toBe(true);
    const attachedCancel = await $('[data-testid="data-transfer-attached-cancel"]');
    expect(await attachedCancel.getAttribute('data-job-id')).toBe(applyJobId);
    const progressText = await $('[data-testid="data-transfer-attached-job-progress"]').getText();
    expect(progressText).toMatch(/\d+\s*\/\s*\d+/);
    expect(await exists('[data-testid="data-transfer-result"]')).toBe(false);
    expect(await exists('[data-testid="data-transfer-execute"]')).toBe(false);
    await captureJourneyStep('dtj1-background-reopened');

    const settledBackgroundJob = await waitForTerminalJob(applyJobId ?? '');
    expect(settledBackgroundJob.state).toBe('succeeded');
    expect(Number(settledBackgroundJob.progress.committed)).toBe(BULK_ROWS);
    expect(await targetRowCount(BULK_TABLE)).toBe(BULK_ROWS);
    expect(await exists('[data-testid="data-transfer-attached-job"]')).toBe(false);

    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DTJ-002: apply 回执丢失后同 key 重放返回原 Job，不重复落库', async () => {
    if (!spentPlan) {
      throw new Error('DTJ-001 must settle a Job before DTJ-002 can retry its plan');
    }
    // A fresh plan is prepared in the window, but replay uses DTJ-001's exact
    // plan and stable idempotency key. The backend must return its original
    // accepted Job receipt even though that Job is already terminal.
    await openDataTransferWindow();
    await driveToPreview(JOB_TABLE, JOB_TABLE, 'dtj2');
    const cancel = await $('[data-testid="data-transfer-cancel"]');
    expect(await cancel.getAttribute('data-cancel-addressable')).toBe('true');
    expect(await cancel.isEnabled()).toBe(true);

    const replay = await invokeBackend<{ jobId: string; replayed: boolean; state: string }>(
      'apply_data_transfer_job',
      {
        request: {
          planId: spentPlan.planId,
          planDigest: spentPlan.planDigest,
          selectionRevision: spentPlan.selectionRevision,
          selection: {},
          confirmedDestructive: false,
          backendScope: {
            sourceBackendScope: 'local-desktop-backend',
            targetBackendScope: 'local-desktop-backend',
            profileBackendScopes: [],
          },
          idempotencyKey: `data-transfer/apply/${spentPlan.planId}`,
        },
      },
    );
    expect(replay.jobId).toBe(spentPlan.jobId);
    expect(replay.replayed).toBe(true);
    expect(replay.state).toBe('succeeded');

    // Replaying the receipt is the whole point: nothing was written twice.
    expect(await targetRowCount(JOB_TABLE)).toBe(3);

    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('DTJ-003: apply 执行中向真实 apply Job 记录取消意图并等待终态', async () => {
    const targetSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });
    try {
      await withSafeModeOff(() =>
        invokeBackend('execute_query', {
          dbSessionId: targetSession,
          sql: `TRUNCATE TABLE ${BULK_TABLE}`,
        }),
      );
    } finally {
      await disconnectBackend(targetSession);
    }

    await openDataTransferWindow();
    await driveToPreview(BULK_TABLE, BULK_TABLE, 'dtj3');

    const execute = await $('[data-testid="data-transfer-execute"]');
    await execute.waitForClickable({ timeout: 20000 });
    await execute.click();
    await captureJourneyStep('dtj3-executing');

    const activeState = await $('[data-testid="data-transfer-job-active-state"]');
    await activeState.waitForDisplayed({ timeout: 30000 });
    const applyJobId = await activeState.getAttribute('data-job-id');
    expect(applyJobId).toBeTruthy();
    await browser.waitUntil(async () => (await readJob(applyJobId ?? '')).state === 'running', {
      timeout: 30000,
      interval: 200,
      timeoutMsg: 'apply never reached running state before cancel',
    });

    const cancel = await $('[data-testid="data-transfer-cancel"]');
    expect(await cancel.getAttribute('data-cancel-addressable')).toBe('true');
    expect(await cancel.getAttribute('data-job-id')).toBe(applyJobId);
    await cancel.click();

    const cancelBanner = await $('[data-testid="data-transfer-job-active-cancel"]');
    await cancelBanner.waitForDisplayed({ timeout: 10000 });
    await browser.waitUntil(
      async () => (await readJob(applyJobId ?? '')).cancelRequested === true,
      { timeout: 15000, interval: 250, timeoutMsg: 'backend did not persist the cancel intent' },
    );
    expect(await exists('[data-testid="data-transfer-result"]')).toBe(true);
    expect(
      await (await $('[data-testid="data-transfer-result"]')).getAttribute('data-completed'),
    ).toBe('false');
    await captureJourneyStep('dtj3-inflight');

    const terminalJob = await waitForTerminalJob(applyJobId ?? '');
    expect(terminalJob.cancelRequested).toBe(true);
    expect(terminalJob.state).toBe('cancelled');
    const result = await $('[data-testid="data-transfer-result"]');
    await result.waitForDisplayed({ timeout: 180000 });
    expect(await result.getAttribute('data-completed')).toBe('false');
    const verdict = await $('[data-testid="data-transfer-job-verdict"]');
    await verdict.waitForDisplayed({ timeout: 10000 });
    expect(await verdict.getAttribute('data-cancel-disposition')).not.toBe('none');

    // Once settled the step moves on: the cancel affordance is gone and the
    // only legal next action is a fresh review (there is no resume token).
    expect(await exists('[data-testid="data-transfer-cancel"]')).toBe(false);
    expect(await exists('[data-testid="data-transfer-cancel-pending-id"]')).toBe(false);
    const rereview = await $('[data-testid="data-transfer-rereview"]');
    expect(await rereview.getAttribute('data-blocked')).toBe('false');
    expect(await rereview.isEnabled()).toBe(true);

    expect(await targetRowCount(BULK_TABLE)).toBeLessThan(BULK_ROWS);
    // The other fixture was deselected in this run, so it must be untouched.
    expect(await targetRowCount(JOB_TABLE)).toBe(3);
    await captureJourneyStep('dtj3-settled');

    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });
});
