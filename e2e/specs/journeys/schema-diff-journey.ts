/**
 * Schema Diff full user journey — direct URL entry (hidden in v0.1.0 UI).
 *
 * Flow: Open → endpoints → validate → compare diff → generate plan → review → deploy.
 * Requires PostgreSQL (`e2e/setup-sync-dbs.sh` for datazen_sync_src / datazen_sync_tgt).
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
import { seedSecondPgConnection } from '../../lib/testDataLifecycle.js';

async function openSchemaDiffWindow() {
  await browser.url('tauri://localhost/window.html?window=schema-diff');
  await browser.pause(1500);
  const body = await $('body').getText();
  expect(body).toContain(t('common.schemaDiff'));
}

async function setSchemaDiffTables(tableList: string) {
  const textarea = await $(`textarea[placeholder="${t('schemaDiff.tablesPlaceholder')}"]`);
  await textarea.waitForDisplayed({ timeout: 8000 });
  await textarea.setValue(tableList);
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

describe('结构对比完整用户旅程 (SD-JOURNEY)', () => {
  /**
   * Documented user flow:
   * 1. Open Schema Diff window via direct URL
   * 2. Verify Compare → Plan → Review step chrome
   * 3. Select source/target; Compare without tables shows validation error
   * 4. Seed PG fixtures: source table has extra column vs target
   * 5. Compare schemas and review structural diff
   * 6. Generate deploy plan from diff
   * 7. Advance to Review / Deploy step
   * 8. Deploy additive DDL and verify target column exists
   */
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_sd_src_${STAMP}`;
  const TGT_ID = `e2e_sd_tgt_${STAMP}`;
  const SRC_NAME = `SD-Src-${STAMP}`;
  const TGT_NAME = `SD-Tgt-${STAMP}`;
  const TABLE = `sd_journey_${STAMP}`;

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
        sql: `CREATE TABLE ${TABLE} (
          id int PRIMARY KEY,
          name text NOT NULL,
          extra_col text
        )`,
      });
      await invokeBackend('execute_query', {
        dbSessionId: tgtSession,
        sql: `CREATE TABLE ${TABLE} (
          id int PRIMARY KEY,
          name text NOT NULL
        )`,
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

  it('Step 1: 通过 URL 打开结构对比窗口', async () => {
    await openSchemaDiffWindow();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepCompare'));
    expect(body).toContain(t('schemaDiff.stepPlan'));
    expect(body).toContain(t('schemaDiff.stepReview'));
    await captureJourneyStep('sd-journey-01-window-open', 0, true);
  });

  it('Step 2: 选择源/目标后未填表名点对比应提示必填', async () => {
    await seedSecondPgConnection(browser);
    await openSchemaDiffWindow();
    await selectDzOption(t('sync.selectSource'), '本地 PostgreSQL');
    await selectDzOption(t('sync.selectTarget'), 'E2E-PG-目标');
    await captureJourneyStep('sd-journey-02-endpoints-selected', 0, true);

    const compareBtn = await $(`button*=${t('schemaDiff.compare')}`);
    await compareBtn.waitForDisplayed({ timeout: 8000 });
    await compareBtn.click();
    await browser.pause(500);
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.tableRequired'));
    await captureJourneyStep('sd-journey-03-table-required', 0, true);
  });

  it('Step 3: 选择同步专用连接并填写表名', async () => {
    await openSchemaDiffWindow();
    await selectDzOption(t('sync.selectSource'), SRC_NAME);
    await selectDzOption(t('sync.selectTarget'), TGT_NAME);
    await setSchemaDiffTables(TABLE);
    await captureJourneyStep('sd-journey-04-table-entered', 0, true);
  });

  it('Step 4: 对比结构应显示差异面板', async () => {
    const compareBtn = await $(`button*=${t('schemaDiff.compare')}`);
    await compareBtn.click();
    await browser.pause(2500);

    const body = await $('body').getText();
    expect(body).toContain(TABLE);
    expect(body.length).toBeGreaterThan(100);
    await captureJourneyStep('sd-journey-05-compare-result', 0, true);
  });

  it('Step 5: 生成部署计划应进入 Plan 步骤', async () => {
    const planBtn = await $(`button*=${t('schemaDiff.generatePlan')}`);
    await planBtn.waitForClickable({ timeout: 8000 });
    await planBtn.click();
    await browser.pause(2500);

    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepPlan'));
    expect(body).toContain(t('schemaDiff.statements'));
    await captureJourneyStep('sd-journey-06-plan-generated', 0, true);
  });

  it('Step 6: 进入审阅 / 部署步骤', async () => {
    const reviewBtn = await $(`button*=${t('schemaDiff.stepReview')}`);
    await reviewBtn.waitForClickable({ timeout: 8000 });
    await reviewBtn.click();
    await browser.pause(800);

    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.reviewTarget'));
    expect(body).toContain(t('schemaDiff.deploy'));
    await captureJourneyStep('sd-journey-07-review-step', 0, true);
  });

  it('Step 7: 部署到目标并验证 extra_col 列已添加', async () => {
    const deployBtn = await $(`button*=${t('schemaDiff.deploy')}`);
    await deployBtn.waitForClickable({ timeout: 15000 });
    await deployBtn.click();
    await browser.pause(3000);

    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.deployStatus'));
    await captureJourneyStep('sd-journey-08-deploy-complete', 0, true);

    const tgtSession = await invokeBackend<string>('connect', { connectionId: TGT_ID });
    const colCheck = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: tgtSession,
      sql: `SELECT count(*)::int AS c FROM information_schema.columns
            WHERE table_name = '${TABLE}' AND column_name = 'extra_col'`,
    });
    expect(queryScalar(colCheck, 'c')).toBe(1);
    await captureJourneyStep('sd-journey-09-column-verified', 0, true);
  });
});
