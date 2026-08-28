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
import { seedSecondPgConnection } from '../lib/testDataLifecycle.js';

/**
 * Schema Diff window shell + primary controls (SD-001~SD-004).
 */

describe('结构对比窗口 (SD-001~SD-004)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('SD-001: 应能通过 URL 打开结构对比窗口', async () => {
    await openSchemaDiffWindow();
    await expect(await $('[data-testid="schema-diff-window"]')).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('common.schemaDiff'));
    expect(body).toContain(t('schemaDiff.stepCompare'));
    await captureJourneyStep('schema-diff-window-open');
  });

  it('SD-002: 应显示对比 / 生成计划等主操作', async () => {
    await openSchemaDiffWindow();
    await expect(await $('[data-testid="schema-diff-compare"]')).toBeDisplayed();
    await expect(await $('[data-testid="schema-diff-generate-plan"]')).toBeDisplayed();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.tables'));
  });

  it('SD-003: 未填表名点对比应提示必填', async () => {
    await seedSecondPgConnection(browser);
    await openSchemaDiffWindow();
    await selectDzOption(t('sync.selectSource'), '本地 PostgreSQL');
    await selectDzOption(t('sync.selectTarget'), 'E2E-PG-目标');
    await clickSchemaDiffCompare();
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.tableRequired'));
    await captureJourneyStep('schema-diff-table-required');
  });

  it('SD-004: 应通过弹窗显示当前版本限制说明', async () => {
    await openSchemaDiffWindow({ dismissLimitations: false });
    const dialog = await $('[data-testid="schema-diff-limitations-dialog"]');
    await dialog.waitForDisplayed({ timeout: 8000 });
    const panel = await dialog.$('[data-testid="schema-diff-limitations"]');
    await panel.waitForDisplayed({ timeout: 8000 });
    expect(await panel.getText()).toContain(t('schemaDiff.limitations.noViews'));
    await expect(await $('[data-testid="schema-diff-window"]')).toBeDisplayed();
  });
});

describe('结构对比限制说明 (SD-LIM)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 15000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('SD-LIM-001: 勾选「不再显示」后再次打开不应弹出限制说明', async () => {
    await openSchemaDiffWindow({ dismissLimitations: false });
    const dialog = await $('[data-testid="schema-diff-limitations-dialog"]');
    await dialog.waitForDisplayed({ timeout: 8000 });

    const dismiss = await $('[data-testid="schema-diff-limitations-dismiss"]');
    await dismiss.click();
    const closeBtn = await $('[data-testid="schema-diff-limitations-close"]');
    await closeBtn.click();
    await browser.waitUntil(async () => !(await dialog.isDisplayed().catch(() => false)), {
      timeout: 8000,
      timeoutMsg: '等待限制说明弹窗关闭超时',
    });

    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
    await openSchemaDiffWindow({ dismissLimitations: false, clearLimitationsPref: false });

    const dialogAgain = await $('[data-testid="schema-diff-limitations-dialog"]');
    expect(await dialogAgain.isExisting().catch(() => false)).toBe(false);
  });
});
