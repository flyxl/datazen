/**
 * Schema Diff full user journey — PG→PG compare/deploy.
 *
 * Flow: Open → endpoints → compare → plan → review → deploy.
 * Requires PostgreSQL (`e2e/setup-sync-dbs.sh` for datazen_sync_src / datazen_sync_tgt).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  invokeBackend,
  openSchemaDiffWindow,
  queryScalar,
  selectDzOption,
  setSchemaDiffTables,
  clickSchemaDiffCompare,
  clickSchemaDiffGeneratePlan,
  advanceSchemaDiffToReview,
  deploySchemaDiffPlan,
  withSafeModeOff,
  type QueryResultPayload,
} from '../../helpers.js';
import { pgConnectionConfig, teardownSchemaDiffFixture } from '../../lib/schemaDiffFixtures.js';
import { seedSecondPgConnection } from '../../lib/testDataLifecycle.js';

describe('结构对比完整用户旅程 (SD-JOURNEY)', () => {
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
      config: pgConnectionConfig(SRC_ID, SRC_NAME, 'datazen_sync_src'),
    });
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(TGT_ID, TGT_NAME, 'datazen_sync_tgt'),
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
    await teardownSchemaDiffFixture([SRC_ID, TGT_ID], TABLE);
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
    await clickSchemaDiffCompare();
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
    await clickSchemaDiffCompare();
    const body = await $('body').getText();
    expect(body).toContain(TABLE);
    expect(body.length).toBeGreaterThan(100);
    await captureJourneyStep('sd-journey-05-compare-result', 0, true);
  });

  it('Step 5~7: 生成计划→审阅→部署并验证 extra_col', async () => {
    await clickSchemaDiffGeneratePlan();
    let body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepPlan'));
    expect(body).toContain(t('schemaDiff.statements'));
    await captureJourneyStep('sd-journey-06-plan-generated', 0, true);

    await advanceSchemaDiffToReview();
    body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.reviewTarget'));
    expect(body).toContain(t('schemaDiff.deploy'));
    await captureJourneyStep('sd-journey-07-review-step', 0, true);

    await deploySchemaDiffPlan();
    body = await $('body').getText();
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
