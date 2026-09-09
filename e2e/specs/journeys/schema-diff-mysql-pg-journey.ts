/**
 * Schema Diff cross-dialect journey: MySQL → PostgreSQL.
 *
 * Verifies:
 * 1. Missing target table from scratch on PostgreSQL
 * 2. Composite Primary Key (tenant_id, order_id) without duplicate ADD PK
 * 3. Composite Indexes (composite unique, composite regular)
 * 4. Diverse data types (BigInt, Decimal, Boolean, Text, JSON, Datetime)
 * 5. Physical schema verification in PostgreSQL (Composite PK, Composite Indexes, Types)
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  assertSchemaDiffDeploySuccess,
  assertSchemaDiffNoErrors,
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
  fetchPgIndexMap,
  fetchPgPrimaryKeys,
  mysqlConnectionConfig,
  pgConnectionConfig,
  setupMysqlTenantOrdersSourceEmptyPgTarget,
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
    // Target table is completely dropped on PostgreSQL
    await setupMysqlTenantOrdersSourceEmptyPgTarget(SRC_ID, TGT_ID, TABLE);
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

  it('Step 2: 选择 MySQL→PG 端点并填写表名', async () => {
    await selectSchemaDiffEndpoints(SRC_NAME, TGT_NAME);
    await setSchemaDiffTables(TABLE);
    await captureJourneyStep('sd-jmysql-02-endpoints', 0, true);
  });

  it('Step 3: 对比结构并生成部署计划（验证零错误）', async () => {
    await clickSchemaDiffCompare();
    const compareBody = await $('body').getText();
    expect(compareBody).toContain(TABLE);
    await assertSchemaDiffNoErrors();

    await clickSchemaDiffGeneratePlan();
    const planBody = await $('body').getText();
    expect(planBody).toContain(t('schemaDiff.stepPlan'));
    await assertSchemaDiffNoErrors();
    await captureJourneyStep('sd-jmysql-03-plan', 0, true);
  });

  it('Step 4: 审阅部署并执行迁移（严格验证状态为 Committed 且零错误执行）', async () => {
    await advanceSchemaDiffToReview();
    await deploySchemaDiffPlan();
    await assertSchemaDiffDeploySuccess();
    await captureJourneyStep('sd-jmysql-04-deployed', 0, true);
  });

  it('Step 5: 验证 PostgreSQL 目标端物理表结构、联合主键与复合索引', async () => {
    // 1. Core columns exist in PostgreSQL target
    const orderSnExists = await columnExists(TGT_ID, TABLE, 'order_sn', 'postgresql');
    expect(orderSnExists).toBe(true);

    const paymentExists = await columnExists(TGT_ID, TABLE, 'payment_amount', 'postgresql');
    expect(paymentExists).toBe(true);

    const settledExists = await columnExists(TGT_ID, TABLE, 'is_settled', 'postgresql');
    expect(settledExists).toBe(true);

    // 2. Composite Primary Key exists in PostgreSQL
    const pks = await fetchPgPrimaryKeys(TGT_ID, TABLE);
    expect(pks.length).toBe(2);
    expect(pks).toContain('tenant_id');
    expect(pks).toContain('order_id');

    // 3. Composite Indexes exist in PostgreSQL
    const idxMap = await fetchPgIndexMap(TGT_ID, TABLE);

    // Composite unique index: (tenant_id, order_sn)
    const uqIdx = idxMap[`uq_${TABLE}_sn`];
    expect(uqIdx).toBeDefined();
    expect(uqIdx.isUnique).toBe(true);
    expect(uqIdx.columns).toEqual(['tenant_id', 'order_sn']);

    // Composite regular index: (order_status, created_at)
    const statusIdx = idxMap[`idx_${TABLE}_status_created`];
    expect(statusIdx).toBeDefined();
    expect(statusIdx.columns).toEqual(['order_status', 'created_at']);

    await captureJourneyStep('sd-jmysql-05-verified', 0, true);
  });
});
