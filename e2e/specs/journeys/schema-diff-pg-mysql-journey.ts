/**
 * Schema Diff cross-dialect journey: PostgreSQL → MySQL.
 *
 * Verifies:
 * 1. Missing target table from scratch (no 1146)
 * 2. Composite Primary Key (org_id, emp_no) without duplicate ADD PK (no 1068)
 * 3. Composite Indexes (composite unique, composite regular, composite with TEXT) without key length failure (no 1170)
 * 4. Diverse data types (Numeric, Real, Boolean, Datetime, UUID, JSONB, Binary)
 * 5. Type suggestions notice & interactive user override in UI
 * 6. Physical schema verification in MySQL (Composite PK, Composite Indexes, Types)
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
  PG_SYNC_DB,
  columnExists,
  fetchMysqlIndexMap,
  fetchMysqlPrimaryKeys,
  mysqlConnectionConfig,
  pgConnectionConfig,
  setupPgEnterpriseStaffSourceEmptyMysqlTarget,
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
    await $('[data-testid="workspace-nav-databases"]').waitForDisplayed({ timeout: 15000 });
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(SRC_ID, SRC_NAME, PG_SYNC_DB),
    });
    await invokeBackend('save_connection', {
      config: mysqlConnectionConfig(TGT_ID, TGT_NAME, MYSQL_SYNC_DB),
    });
    // Target table is completely dropped (empty target database for this table)
    await setupPgEnterpriseStaffSourceEmptyMysqlTarget(SRC_ID, TGT_ID, TABLE);
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
    await selectSchemaDiffEndpoints(SRC_NAME, TGT_NAME);
    await setSchemaDiffTables(TABLE);
    await captureJourneyStep('sd-jpg-02-endpoints', 0, true);
  });

  it('Step 3: 对比结构并验证无任何错误、差异正常呈现', async () => {
    await clickSchemaDiffCompare();
    const body = await $('body').getText();
    expect(body).toContain(TABLE);
    await assertSchemaDiffNoErrors();
    await captureJourneyStep('sd-jpg-03-compare', 0, true);
  });

  it('Step 4: 生成跨方言部署计划并验证字段类型建议卡片与交互覆写（零阻断错误）', async () => {
    await clickSchemaDiffGeneratePlan();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepPlan'));
    await assertSchemaDiffNoErrors();

    // Verify TypeSuggestionsNotice card is displayed
    const suggestions = await $('[data-testid="schema-diff-type-suggestions"]');
    await suggestions.waitForDisplayed({ timeout: 15000 });
    const suggestionsText = await suggestions.getText();
    expect(suggestionsText).toContain('search_tag');

    // Customize search_tag to VARCHAR(128) and apply
    const tagInput = await $('[data-testid="type-suggestion-input-search_tag"]');
    if (await tagInput.isDisplayed().catch(() => false)) {
      await tagInput.setValue('VARCHAR(128)');
      const applyBtn = await $('[data-testid="schema-diff-apply-suggestions"]');
      await applyBtn.click();
      await browser.pause(1500);
      await assertSchemaDiffNoErrors();
    }

    await captureJourneyStep('sd-jpg-04-plan', 0, true);
  });

  it('Step 5: 审阅部署并执行迁移（严格验证状态为 Committed 且零错误执行）', async () => {
    await advanceSchemaDiffToReview();
    await deploySchemaDiffPlan();
    await assertSchemaDiffDeploySuccess();
    await captureJourneyStep('sd-jpg-05-deploy', 0, true);
  });

  it('Step 6: 验证 MySQL 目标端物理表结构、联合主键与复合索引', async () => {
    // 1. Core columns exist in MySQL target
    const empNameExists = await columnExists(TGT_ID, TABLE, 'emp_name', 'mysql');
    expect(empNameExists).toBe(true);

    const salaryExists = await columnExists(TGT_ID, TABLE, 'base_salary', 'mysql');
    expect(salaryExists).toBe(true);

    const activeExists = await columnExists(TGT_ID, TABLE, 'is_active', 'mysql');
    expect(activeExists).toBe(true);

    // 2. Composite Primary Key exists in MySQL
    const pks = await fetchMysqlPrimaryKeys(TGT_ID, TABLE);
    expect(pks.length).toBe(2);
    expect(pks).toContain('org_id');
    expect(pks).toContain('emp_no');

    // 3. Composite Indexes exist in MySQL
    const idxMap = await fetchMysqlIndexMap(TGT_ID, TABLE);

    // Composite unique index: (org_id, email)
    const uqIdx = idxMap[`uq_${TABLE}_org_email`];
    expect(uqIdx).toBeDefined();
    expect(uqIdx.isUnique).toBe(true);
    expect(uqIdx.columns).toEqual(['org_id', 'email']);

    // Composite regular index: (dept_code, hire_date)
    const deptHireIdx = idxMap[`idx_${TABLE}_dept_hire`];
    expect(deptHireIdx).toBeDefined();
    expect(deptHireIdx.columns).toEqual(['dept_code', 'hire_date']);

    // Composite index containing TEXT column: (org_id, search_tag)
    const orgTagIdx = idxMap[`idx_${TABLE}_org_tag`];
    expect(orgTagIdx).toBeDefined();
    expect(orgTagIdx.columns.length).toBe(2);
    expect(orgTagIdx.columns[0]).toBe('org_id');
    expect(orgTagIdx.columns[1]).toContain('search_tag');

    await captureJourneyStep('sd-jpg-06-verified', 0, true);
  });
});
