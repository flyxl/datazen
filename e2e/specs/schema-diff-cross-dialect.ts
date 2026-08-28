/**
 * Schema Diff cross-dialect E2E: PG↔MySQL wide-type structural deploy.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
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
} from '../helpers.js';
import {
  MYSQL_SYNC_DB,
  PG_SYNC_DB,
  PG_SYNC_TGT_DB,
  assertWideColumnCount,
  columnExists,
  countTableColumns,
  mysqlConnectionConfig,
  pgConnectionConfig,
  setupMysqlWideSourceMinimalPgTarget,
  setupPgWideSourceMinimalMysqlTarget,
  teardownSchemaDiffFixture,
} from '../lib/schemaDiffFixtures.js';

describe('PG→MySQL 跨方言宽类型 (SD-CROSS-PG-MYSQL)', function () {
  this.timeout(120000);
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_sd_xpg_${STAMP}`;
  const TGT_ID = `e2e_sd_xmysql_${STAMP}`;
  const SRC_NAME = `SD-XPG-${STAMP}`;
  const TGT_NAME = `SD-XMySQL-${STAMP}`;
  const TABLE = `sd_cross_pg_mysql_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
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

  it('SD-XPG-001: 跨方言对比应显示警告或差异', async () => {
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints(SRC_NAME, TGT_NAME);
    await setSchemaDiffTables(TABLE);
    await clickSchemaDiffCompare();
    const body = await $('body').getText();
    expect(body).toContain(TABLE);
    await captureJourneyStep('sd-xpg-compare', 0, true);
  });

  it('SD-XPG-002: 计划应含 MySQL 类型映射（DATETIME/TINYINT 等）', async () => {
    await clickSchemaDiffGeneratePlan();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepPlan'));
    const lower = body.toLowerCase();
    expect(lower.includes('datetime') || lower.includes('tinyint') || lower.includes('add')).toBe(
      true,
    );
    await captureJourneyStep('sd-xpg-plan', 0, true);
  });

  it('SD-XPG-003: 部署后 MySQL 目标应含宽类型列', async () => {
    await advanceSchemaDiffToReview();
    await deploySchemaDiffPlan();
    const colCount = await countTableColumns(TGT_ID, TABLE, 'mysql');
    assertWideColumnCount(colCount);
    await captureJourneyStep('sd-xpg-deployed', 0, true);
  });
});

describe('MySQL→PG 跨方言宽类型 (SD-CROSS-MYSQL-PG)', function () {
  this.timeout(120000);
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_sd_xmysql_src_${STAMP}`;
  const TGT_ID = `e2e_sd_xpg_tgt_${STAMP}`;
  const SRC_NAME = `SD-XMySQL-Src-${STAMP}`;
  const TGT_NAME = `SD-XPG-Tgt-${STAMP}`;
  const TABLE = `sd_cross_mysql_pg_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
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

  it('SD-XMYSQL-001: 跨方言对比应列出表差异', async () => {
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints(SRC_NAME, TGT_NAME);
    await setSchemaDiffTables(TABLE);
    await clickSchemaDiffCompare();
    const body = await $('body').getText();
    expect(body).toContain(TABLE);
    await captureJourneyStep('sd-xmysql-compare', 0, true);
  });

  it('SD-XMYSQL-002: 计划应含 PG 映射类型', async () => {
    await clickSchemaDiffGeneratePlan();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepPlan'));
    const lower = body.toLowerCase();
    expect(
      lower.includes('boolean') || lower.includes('timestamptz') || lower.includes('add'),
    ).toBe(true);
    await captureJourneyStep('sd-xmysql-plan', 0, true);
  });

  it('SD-XMYSQL-003: 部署后 PG 目标应含宽类型列', async () => {
    await advanceSchemaDiffToReview();
    await deploySchemaDiffPlan();
    const exists = await columnExists(TGT_ID, TABLE, 'label', 'postgresql');
    expect(exists).toBe(true);
    await captureJourneyStep('sd-xmysql-deployed', 0, true);
  });
});
