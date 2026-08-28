/**
 * Schema Diff options matrix: allowDestructive / includeIndexes combinations.
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
} from '../helpers.js';
import {
  PG_SYNC_DB,
  PG_SYNC_TGT_DB,
  pgConnectionConfig,
  setupDestructiveDiffFixture,
  teardownSchemaDiffFixture,
} from '../lib/schemaDiffFixtures.js';

describe('结构对比选项矩阵 (SD-OPT-MATRIX)', function () {
  this.timeout(90000);
  let mainWindow: string;
  const STAMP = Date.now().toString(36);
  const SRC_ID = `e2e_sd_opt_src_${STAMP}`;
  const TGT_ID = `e2e_sd_opt_tgt_${STAMP}`;
  const SRC_NAME = `SD-Opt-Src-${STAMP}`;
  const TGT_NAME = `SD-Opt-Tgt-${STAMP}`;
  const TABLE = `sd_opt_${STAMP}`;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(SRC_ID, SRC_NAME, PG_SYNC_DB),
    });
    await invokeBackend('save_connection', {
      config: pgConnectionConfig(TGT_ID, TGT_NAME, PG_SYNC_TGT_DB),
    });
    await setupDestructiveDiffFixture(SRC_ID, TGT_ID, TABLE);
  });

  after(async () => {
    await teardownSchemaDiffFixture([SRC_ID, TGT_ID], TABLE);
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  async function runCompareAndPlan() {
    await openSchemaDiffWindow();
    await selectDzOption(t('sync.selectSource'), SRC_NAME);
    await selectDzOption(t('sync.selectTarget'), TGT_NAME);
    await setSchemaDiffTables(TABLE);
    await clickSchemaDiffCompare();
    await clickSchemaDiffGeneratePlan();
  }

  it('SD-OPT-001: 默认计划应跳过破坏性 DROP', async () => {
    await runCompareAndPlan();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.extraOnTarget'));
    expect(body.toLowerCase()).not.toContain('drop column orphan_col');
    await captureJourneyStep('sd-opt-no-destructive', 0, true);
  });

  it('SD-OPT-002: 勾选 allowDestructive 后计划应含 DROP', async () => {
    await runCompareAndPlan();
    const destructive = await $('[data-testid="schema-diff-allow-destructive"]');
    if (!(await destructive.isSelected())) await destructive.click();
    await browser.pause(300);
    const regen = await $(`button*=${t('schemaDiff.regeneratePlan')}`);
    await regen.click();
    await browser.pause(2500);
    const body = await $('body').getText();
    expect(body.toLowerCase()).toMatch(/drop|destructive/);
    await captureJourneyStep('sd-opt-destructive', 0, true);
  });

  it('SD-OPT-003: includeIndexes 开关应可切换', async () => {
    await runCompareAndPlan();
    const includeIdx = await $('[data-testid="schema-diff-include-indexes"]');
    expect(await includeIdx.isExisting()).toBe(true);
    if (await includeIdx.isSelected()) await includeIdx.click();
    await browser.pause(300);
    const regen = await $(`button*=${t('schemaDiff.regeneratePlan')}`);
    await regen.click();
    await browser.pause(1500);
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.stepPlan'));
    await captureJourneyStep('sd-opt-indexes', 0, true);
  });
});
