/**
 * Schema Diff cross-dialect journey: MySQL → PostgreSQL.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  invokeBackend,
  openSchemaDiffWindow,
  selectSchemaDiffEndpoints,
  setSchemaDiffTables,
  clickSchemaDiffCompare,
  clickSchemaDiffGeneratePlan,
  advanceSchemaDiffToReview,
  deploySchemaDiffPlan,
} from '../../helpers.js';
import {
  MYSQL_SYNC_DB,
  PG_SYNC_TGT_DB,
  columnExists,
  mysqlConnectionConfig,
  pgConnectionConfig,
  setupMysqlWideSourceMinimalPgTarget,
  teardownSchemaDiffFixture,
} from '../../lib/schemaDiffFixtures.js';

describe('结构对比 MySQL→PG 跨方言旅程 (SD-MYSQL-PG-JOURNEY)', function () {
  this.timeout(120000);
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_sd_jmysql_src_${STAMP}`;
  const TGT_ID = `e2e_sd_jpg_tgt_${STAMP}`;
  const SRC_NAME = `SD-J-MySQL-${STAMP}`;
  const TGT_NAME = `SD-J-PG-${STAMP}`;
  const TABLE = `sd_j_mysql_pg_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
    await invokeBackend('save_connection', {
      config: mysqlConnectionConfig(SRC_ID, SRC_NAME, MYSQL_SYNC_DB),
    });
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(TGT_ID, TGT_NAME, PG_SYNC_TGT_DB),
    });
    await setupMysqlWideSourceMinimalPgTarget(SRC_ID, TGT_ID, TABLE);
  });

  after(async () => {
    await teardownSchemaDiffFixture([SRC_ID, TGT_ID], TABLE);
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('Step 1: 打开结构对比窗口', async () => {
    await openSchemaDiffWindow();
    await captureJourneyStep('sd-jmysql-01-open', 0, true);
  });

  it('Step 2: 选择 MySQL→PG 端点', async () => {
    await selectSchemaDiffEndpoints(SRC_NAME, TGT_NAME);
    await setSchemaDiffTables(TABLE);
    await captureJourneyStep('sd-jmysql-02-endpoints', 0, true);
  });

  it('Step 3~4: 对比→计划→部署并验证 label 列', async () => {
    await clickSchemaDiffCompare();
    await clickSchemaDiffGeneratePlan();
    let body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepPlan'));
    await captureJourneyStep('sd-jmysql-03-plan', 0, true);

    await advanceSchemaDiffToReview();
    await deploySchemaDiffPlan();
    const exists = await columnExists(TGT_ID, TABLE, 'label', 'postgresql');
    expect(exists).toBe(true);
    await captureJourneyStep('sd-jmysql-04-verified', 0, true);
  });
});
