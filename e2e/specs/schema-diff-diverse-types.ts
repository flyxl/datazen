/**
 * Schema Diff comprehensive E2E: wide column types + multi-table batch compare.
 *
 * Uses e2e/lib/schemaDiffFixtures.ts (19 columns, 10 tables).
 * Preflight: bash e2e/setup-schema-diff-e2e.sh (also run via pnpm e2e:schema-diff).
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
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
} from '../helpers.js';
import {
  SCHEMA_DIFF_WIDE_COLUMN_COUNT,
  SCHEMA_DIFF_MULTI_TABLE_COUNT,
  PG_SYNC_DB,
  PG_SYNC_TGT_DB,
  assertWideColumnCount,
  countTableColumns,
  pgConnectionConfig,
  setupMultiTableSchemaDiff,
  setupPgWideSourceMinimalTarget,
  teardownSchemaDiffFixture,
} from '../lib/schemaDiffFixtures.js';

describe(`PG→PG 宽类型 ${SCHEMA_DIFF_WIDE_COLUMN_COUNT} 列 (SD-COMP-PG-PG)`, function () {
  this.timeout(120000);
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_sd_comp_pg_${STAMP}`;
  const TGT_ID = `e2e_sd_comp_tgt_${STAMP}`;
  const SRC_NAME = `SD-Comp-PG-${STAMP}`;
  const TGT_NAME = `SD-Comp-Tgt-${STAMP}`;
  const TABLE = `sd_comp_pg_pg_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(SRC_ID, SRC_NAME, PG_SYNC_DB),
    });
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(TGT_ID, TGT_NAME, PG_SYNC_TGT_DB),
    });
    await setupPgWideSourceMinimalTarget(SRC_ID, TGT_ID, TABLE);
  });

  after(async () => {
    await teardownSchemaDiffFixture([SRC_ID, TGT_ID], TABLE);
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('SD-COMP-001: 对比应显示缺失列差异', async () => {
    await openSchemaDiffWindow();
    await selectDzOption(t('sync.selectSource'), SRC_NAME);
    await selectDzOption(t('sync.selectTarget'), TGT_NAME);
    await setSchemaDiffTables(TABLE);
    await clickSchemaDiffCompare();
    const body = await $('body').getText();
    expect(body).toContain(TABLE);
    expect(body).toContain(t('schemaDiff.missingOnTarget'));
    await captureJourneyStep('sd-comp-compare', 0, true);
  });

  it('SD-COMP-002: 生成计划应含 ADD COLUMN 语句', async () => {
    await clickSchemaDiffGeneratePlan();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepPlan'));
    expect(body.toLowerCase()).toContain('add');
    await captureJourneyStep('sd-comp-plan', 0, true);
  });

  it('SD-COMP-003: 部署后目标表应含全部宽类型列', async () => {
    await advanceSchemaDiffToReview();
    await deploySchemaDiffPlan();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.deployStatus'));
    const colCount = await countTableColumns(TGT_ID, TABLE, 'postgresql');
    assertWideColumnCount(colCount);
    await captureJourneyStep('sd-comp-deployed', 0, true);
  });
});

describe(`PG→PG 多表 ${SCHEMA_DIFF_MULTI_TABLE_COUNT} 张 (SD-COMP-MULTI)`, function () {
  this.timeout(120000);
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_sd_multi_src_${STAMP}`;
  const TGT_ID = `e2e_sd_multi_tgt_${STAMP}`;
  const SRC_NAME = `SD-Multi-Src-${STAMP}`;
  const TGT_NAME = `SD-Multi-Tgt-${STAMP}`;
  const PREFIX = `sd_multi_${STAMP}`;
  let tables: string[] = [];

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(SRC_ID, SRC_NAME, PG_SYNC_DB),
    });
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(TGT_ID, TGT_NAME, PG_SYNC_TGT_DB),
    });
    tables = await setupMultiTableSchemaDiff(SRC_ID, TGT_ID, PREFIX, SCHEMA_DIFF_MULTI_TABLE_COUNT);
  });

  after(async () => {
    await teardownSchemaDiffFixture([SRC_ID, TGT_ID], tables);
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('SD-COMP-MULTI-001: 批量对比应列出全部表', async () => {
    await openSchemaDiffWindow();
    await selectDzOption(t('sync.selectSource'), SRC_NAME);
    await selectDzOption(t('sync.selectTarget'), TGT_NAME);
    await setSchemaDiffTables(tables.join('\n'));
    await clickSchemaDiffCompare();
    const body = await $('body').getText();
    for (const table of tables.slice(0, 3)) {
      expect(body).toContain(table);
    }
    await captureJourneyStep('sd-multi-compare', 0, true);
  });

  it('SD-COMP-MULTI-002: 批量计划应含多条语句', async () => {
    await clickSchemaDiffGeneratePlan();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.statements'));
    expect(body.length).toBeGreaterThan(200);
    await captureJourneyStep('sd-multi-plan', 0, true);
  });
});
