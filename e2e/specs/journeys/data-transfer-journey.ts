/**
 * Data Transfer wizard full user journey — direct URL entry (hidden in v0.1.0 UI).
 *
 * Flow: Open → endpoints → mode → objects → mapping → preview → execute → verify rows.
 * Requires PostgreSQL sync DBs (`e2e/setup-sync-dbs.sh`).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  invokeBackend,
  queryScalar,
  selectDzOption,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../helpers.js';

async function openTransferWindow() {
  await browser.url('tauri://localhost/window.html?window=data-transfer');
  await browser.pause(1500);
  await $('[data-testid="data-transfer-window"]').waitForDisplayed({ timeout: 10000 });
  await $('[data-testid="data-transfer-step-endpoints"]').waitForDisplayed({ timeout: 10000 });
}

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
   * 5. Advance through endpoints → mode step
   * 6. Choose data mode and advance to objects (inspect)
   * 7. Advance through mapping / options toward preview
   * 8. Execute transfer and verify target row count
   */
  let mainWindow: string;
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

    const srcSession = await invokeBackend<string>('connect', { connectionId: SRC_ID });
    const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });

    await withSafeModeOff(async () => {
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `DROP TABLE IF EXISTS ${TABLE}`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: tgtSession,
        sql: `DROP TABLE IF EXISTS ${TABLE}`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL, qty int)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: srcSession,
        sql: `INSERT INTO ${TABLE} (id, name, qty) VALUES (1,'a',10),(2,'b',20),(3,'c',30)`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: tgtSession,
        sql: `CREATE TABLE ${TABLE} (id int PRIMARY KEY, name text NOT NULL, qty int)`,
      });
    });
  });

  after(async () => {
    try {
      const srcSession = await invokeBackend<string>('connect', { connectionId: SRC_ID });
      const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });
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
    } catch {
      /* ok */
    }
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
    await browser.pause(1200);
    await captureJourneyStep(stepLabel, 0, true);
  }

  it('Step 1: 通过 URL 打开数据传输窗口', async () => {
    await openTransferWindow();
    const root = await $('[data-testid="data-transfer-window"]');
    await expect(root).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('transfer.title'));
    await captureJourneyStep('dt-journey-01-window-open', 0, true);
  });

  it('Step 2: 应显示向导步骤与传输模式', async () => {
    await expect(await $('[data-testid="data-transfer-step-endpoints"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-mode-structure"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-mode-data"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-mode-both"]')).toBeDisplayed();
    await captureJourneyStep('dt-journey-02-wizard-chrome', 0, true);
  });

  it('Step 3: 未选两端点 Next 应提示错误', async () => {
    await openTransferWindow();
    const next = await $('[data-testid="data-transfer-next"]');
    await next.click();
    await browser.pause(500);
    const err = await $('[data-testid="data-transfer-error"]');
    await expect(err).toBeDisplayed();
    expect(await err.getText()).toContain(t('transfer.selectBoth'));
    await captureJourneyStep('dt-journey-03-select-both-error', 0, true);
  });

  it('Step 4: 选择源/目标连接', async () => {
    await openTransferWindow();
    await selectDzOption(t('transfer.pickConnection'), SRC_NAME);
    await selectDzOption(t('transfer.pickConnection'), TGT_NAME);
    await browser.pause(1500);
    await expect(await $('[data-testid="data-transfer-source-database"]')).toBeDisplayed();
    await expect(await $('[data-testid="data-transfer-target-database"]')).toBeDisplayed();
    await captureJourneyStep('dt-journey-04-endpoints-selected', 0, true);
  });

  it('Step 5: 进入模式选择步骤', async () => {
    await clickNext('dt-journey-05-mode-step');
    await expect(await $('[data-testid="data-transfer-mode-data"]')).toBeDisplayed();
  });

  it('Step 6: 选择 data 模式并进入对象选择', async () => {
    const dataMode = await $('[data-testid="data-transfer-mode-data"]');
    await dataMode.click();
    await browser.pause(300);
    await captureJourneyStep('dt-journey-06-data-mode-selected', 0, true);
    await clickNext('dt-journey-07-objects-step');
    await browser.pause(2000);
  });

  it('Step 7: 推进映射与预览步骤', async () => {
    await clickNext('dt-journey-08-mapping-step');
    await browser.pause(1500);

    for (let i = 0; i < 6; i++) {
      const execute = await $('[data-testid="data-transfer-execute"]');
      if (await execute.isExisting().catch(() => false)) break;
      const preview = await $('[data-testid="data-transfer-preview"]');
      if (await preview.isDisplayed().catch(() => false)) {
        await captureJourneyStep('dt-journey-09-preview-visible', 0, true);
        break;
      }
      const next = await $('[data-testid="data-transfer-next"]');
      const disabled = (await next.getAttribute('disabled')) === 'true';
      if (disabled) await browser.pause(1000);
      await clickNext(`dt-journey-09-wizard-advance-${i}`);
    }
  });

  it('Step 8: 执行传输并验证目标行数', async () => {
    const execute = await $('[data-testid="data-transfer-execute"]');
    await execute.waitForClickable({ timeout: 15000 });
    await execute.click();
    await browser.pause(1500);
    await captureJourneyStep('dt-journey-10-executed', 0, true);

    const result = await $('[data-testid="data-transfer-result"]');
    await result.waitForDisplayed({ timeout: 15000 });
    await captureJourneyStep('dt-journey-11-result-visible', 0, true);

    const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });
    const rows = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: tgtSession,
      sql: `SELECT count(*)::int AS c FROM ${TABLE}`,
    });
    expect(queryScalar(rows, 'c')).toBe(3);
    await captureJourneyStep('dt-journey-12-rows-verified', 0, true);
  });
});
