/**
 * Data Transfer wizard full user journey — direct URL entry (hidden in v0.1.0 UI).
 *
 * Flow: Open → endpoints → setup → objects → mapping → preview/execute → result.
 * Requires PostgreSQL sync DBs (`e2e/setup-sync-dbs.sh`).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  connectBackend,
  disconnectBackend,
  invokeBackend,
  openDataTransferWindow,
  queryScalar,
  selectDzOptionInWrap,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../helpers.js';

function pgConfig(id: string, name: string, database: string) {
  return {
    id,
    name,
    databaseType: 'postgresql',
    host: process.env.E2E_PG_HOST || '127.0.0.1',
    port: Number(process.env.E2E_PG_PORT) || 5432,
    username: process.env.E2E_PG_USER || 'postgres',
    password: process.env.E2E_PG_PASSWORD || '',
    database,
    sslMode: 'disable',
  };
}

describe('数据传输完整用户旅程 (DT-JOURNEY)', () => {
  /**
   * Documented user flow:
   * 1. Open Data Transfer window via direct URL
   * 2. Verify wizard step chrome and transfer modes
   * 3. Validate Next without endpoints → selectBoth error
   * 4. Select source/target PG sync connections
   * 5. Advance through endpoints → setup step
   * 6. Choose data mode and advance to objects (inspect)
   * 7. Advance through mapping toward preview
   * 8. Execute transfer and verify target row count
   */
  let mainWindow: string;
  let setupSrcSession: string | undefined;
  let setupTgtSession: string | undefined;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_dt_j_src_${STAMP}`;
  const TGT_ID = `e2e_dt_j_tgt_${STAMP}`;
  const SRC_NAME = `DT-J-Src-${STAMP}`;
  const TGT_NAME = `DT-J-Tgt-${STAMP}`;
  const TABLE = `dt_journey_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });

    await invokeBackend('save_connection', {
      config: pgConfig(SRC_ID, SRC_NAME, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: pgConfig(TGT_ID, TGT_NAME, 'datazen_sync_tgt'),
    });

    setupSrcSession = await connectBackend(SRC_ID);
    setupTgtSession = await connectBackend(TGT_ID);

    await withSafeModeOff(async () => {
      await invokeBackend('execute_query', {
        dbSessionId: setupSrcSession!,
        sql: `DROP TABLE IF EXISTS ${TABLE}`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: setupTgtSession!,
        sql: `DROP TABLE IF EXISTS ${TABLE}`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: setupSrcSession!,
        sql: `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL, qty int)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: setupSrcSession!,
        sql: `INSERT INTO ${TABLE} (id, name, qty) VALUES (1,'a',10),(2,'b',20),(3,'c',30)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: setupTgtSession!,
        sql: `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL, qty int)`,
      });
    });
  });

  after(async () => {
    try {
      const srcSession = setupSrcSession ?? (await connectBackend(SRC_ID));
      const tgtSession = setupTgtSession ?? (await connectBackend(TGT_ID));
      await withSafeModeOff(async () => {
        await invokeBackend('execute_query', {
          dbSessionId: srcSession,
          sql: `DROP TABLE IF EXISTS ${TABLE}`,
        });
        await invokeBackend('execute_query', {
          dbSessionId: tgtSession,
          sql: `DROP TABLE IF EXISTS ${TABLE}`,
        });
      });
      if (!setupSrcSession) await disconnectBackend(srcSession);
      if (!setupTgtSession) await disconnectBackend(tgtSession);
    } catch {
      /* ok */
    }
    if (setupSrcSession) await disconnectBackend(setupSrcSession);
    if (setupTgtSession) await disconnectBackend(setupTgtSession);
    try {
      await invokeBackend('delete_connection', { id: SRC_ID });
    } catch {
      /* ok */
    }
    try {
      await invokeBackend('delete_connection', { id: TGT_ID });
    } catch {
      /* ok */
    }
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  async function clickNext(stepLabel: string) {
    const next = await $('[data-testid="data-transfer-next"]');
    await next.waitForClickable({ timeout: 8000 });
    await next.click();
    await browser.pause(300);
    await captureJourneyStep(stepLabel, 0, true);
  }

  it('Step 1: 选择源/目标连接', async () => {
    await openDataTransferWindow();
    await selectDzOptionInWrap('data-transfer-source', SRC_NAME);
    await selectDzOptionInWrap('data-transfer-target', TGT_NAME);
    await browser.pause(300);
    await expect(await $('[data-testid="data-transfer-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-target-database"]')).toBeDisplayed();
    await captureJourneyStep('dt-journey-04-endpoints-selected', 0, true);
  });

  it('Step 2: 进入 setup 步骤（模式与选项）', async () => {
    await clickNext('dt-journey-05-setup-step');
    await expect(await $('[data-testid="data-transfer-mode-data"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-mode-structure"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-mode-both"]')).toBeDisplayed();
  });

  it('Step 3: 选择 data 模式并进入对象选择', async () => {
    const dataMode = await $('[data-testid="data-transfer-mode-data"]');
    await dataMode.click();
    await browser.pause(300);
    await captureJourneyStep('dt-journey-06-data-mode-selected', 0, true);
    await clickNext('dt-journey-07-objects-step');
    await browser.pause(300);
  });

  it('Step 4: 推进映射与预览直至执行步骤', async () => {
    for (let i = 0; i < 8; i++) {
      const execute = await $('[data-testid="data-transfer-execute"]');
      if (await execute.isExisting().catch(() => false)) {
        await captureJourneyStep('dt-journey-09-execute-step-ready', 0, true);
        break;
      }
      const preview = await $('[data-testid="data-transfer-preview"]');
      if (await preview.isDisplayed().catch(() => false)) {
        await captureJourneyStep('dt-journey-09-preview-visible', 0, true);
      }
      const next = await $('[data-testid="data-transfer-next"]');
      const disabled = (await next.getAttribute('disabled')) === 'true';
      if (disabled) await browser.pause(300);
      await clickNext(`dt-journey-09-wizard-advance-${i}`);
    }
  });

  it('Step 5: 执行传输并验证目标行数', async () => {
    const execute = await $('[data-testid="data-transfer-execute"]');
    await execute.waitForClickable({ timeout: 15000 });
    await execute.click();
    await captureJourneyStep('dt-journey-10-executed', 0, true);

    const result = await $('[data-testid="data-transfer-result"]');
    await result.waitForDisplayed({ timeout: 15000 });
    await captureJourneyStep('dt-journey-11-result-visible', 0, true);

    const tgtSession = await connectBackend(TGT_ID);
    const rows = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: tgtSession,
      sql: `SELECT count(*)::int AS c FROM ${TABLE}`,
    });
    expect(queryScalar(rows, 'c')).toBe(3);
    await captureJourneyStep('dt-journey-12-rows-verified', 0, true);
  });
});
