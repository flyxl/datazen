/**
 * Schema Diff cross-dialect journey: PostgreSQL → MySQL.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  invokeBackend,
  openSchemaDiffWindow,
  selectDzOption,
  setSchemaDiffTables,
  clickSchemaDiffCompare,
  clickSchemaDiffGeneratePlan,
  advanceSchemaDiffToReview,
  deploySchemaDiffPlan,
} from '../../helpers.js';
import {
  MYSQL_SYNC_DB,
  PG_SYNC_DB,
  columnExists,
  mysqlConnectionConfig,
  pgConnectionConfig,
  setupPgWideSourceMinimalMysqlTarget,
  teardownSchemaDiffFixture,
} from '../../lib/schemaDiffFixtures.js';

describe('结构对比 PG→MySQL 跨方言旅程 (SD-PG-MYSQL-JOURNEY)', function () {
  this.timeout(120000);
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_sd_jpg_src_${STAMP}`;
  const TGT_ID = `e2e_sd_jmysql_tgt_${STAMP}`;
  const SRC_NAME = `SD-J-PG-${STAMP}`;
  const TGT_NAME = `SD-J-MySQL-${STAMP}`;
  const TABLE = `sd_j_pg_mysql_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(SRC_ID, SRC_NAME, PG_SYNC_DB),
    });
    await invokeBackend('save_connection', {
      config: mysqlConnectionConfig(TGT_ID, TGT_NAME, MYSQL_SYNC_DB),
    });
    await setupPgWideSourceMinimalMysqlTarget(SRC_ID, TGT_ID, TABLE);
  });

  after(async () => {
    await teardownSchemaDiffFixture([SRC_ID, TGT_ID], TABLE);
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('Step 1: 打开结构对比窗口', async () => {
    await openSchemaDiffWindow();
    await captureJourneyStep('sd-jpg-01-open', 0, true);
  });

  it('Step 2: 选择 PG→MySQL 端点并填写表名', async () => {
    await selectDzOption(t('sync.selectSource'), SRC_NAME);
    await selectDzOption(t('sync.selectTarget'), TGT_NAME);
    await setSchemaDiffTables(TABLE);
    await captureJourneyStep('sd-jpg-02-endpoints', 0, true);
  });

  it('Step 3: 对比结构', async () => {
    await clickSchemaDiffCompare();
    const body = await $('body').getText();
    expect(body).toContain(TABLE);
    await captureJourneyStep('sd-jpg-03-compare', 0, true);
  });

  it('Step 4: 生成跨方言部署计划', async () => {
    await clickSchemaDiffGeneratePlan();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepPlan'));
    await captureJourneyStep('sd-jpg-04-plan', 0, true);
  });

  it('Step 5~6: 审阅部署并验证 MySQL label 列', async () => {
    await advanceSchemaDiffToReview();
    await deploySchemaDiffPlan();
    await captureJourneyStep('sd-jpg-05-deploy', 0, true);
    const exists = await columnExists(TGT_ID, TABLE, 'label', 'mysql');
    expect(exists).toBe(true);
    await captureJourneyStep('sd-jpg-06-verified', 0, true);
  });
});
