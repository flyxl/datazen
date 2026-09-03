/**
 * Schema Diff safety E2E: never invent business values for NOT NULL additions.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
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
  PG_SYNC_DB,
  PG_SYNC_TGT_DB,
  columnNullable,
  pgConnectionConfig,
  setupNotNullNoDefaultDiffFixture,
  teardownSchemaDiffFixture,
} from '../lib/schemaDiffFixtures.js';

describe('Schema Diff 安全性 (SD-SAFETY)', function () {
  this.timeout(120000);
  let mainWindow: string;
  const stamp = Date.now().toString(36);
  const srcId = `e2e_sd_safe_src_${stamp}`;
  const tgtId = `e2e_sd_safe_tgt_${stamp}`;
  const srcName = `SD-Safe-Src-${stamp}`;
  const tgtName = `SD-Safe-Tgt-${stamp}`;
  const table = `sd_safe_${stamp}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(srcId, srcName, PG_SYNC_DB),
    });
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(tgtId, tgtName, PG_SYNC_TGT_DB),
    });
    await setupNotNullNoDefaultDiffFixture(srcId, tgtId, table);
  });

  after(async () => {
    await teardownSchemaDiffFixture([srcId, tgtId], table);
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('SD-SAFE-001: NOT NULL 无默认值时计划不得伪造 0/空字符串默认值', async () => {
    await openSchemaDiffWindow();
    await selectSchemaDiffEndpoints(srcName, tgtName);
    await setSchemaDiffTables(table);
    await clickSchemaDiffCompare();
    await clickSchemaDiffGeneratePlan();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepPlan'));
    expect(body).not.toMatch(/DEFAULT\s+(0|''|false)/i);
    expect(body).toMatch(/backfill|default|NOT NULL/i);
  });

  it('SD-SAFE-002: 部署安全计划后先保持可回填的 nullable 状态', async () => {
    await advanceSchemaDiffToReview();
    await deploySchemaDiffPlan();
    expect(await columnNullable(tgtId, table, 'status')).toBe(true);
  });
});
