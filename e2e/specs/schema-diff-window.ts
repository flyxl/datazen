import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeExtraWindows, captureJourneyStep } from '../helpers.js';

/**
 * Schema Diff window shell + primary controls (SD-001~SD-003).
 * Full compare/deploy against two DBs can be extended when dual fixtures are stable.
 */

describe('结构对比窗口 (SD-001~SD-003)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $(`button*=${t('action.newConnection')}`).waitForDisplayed({ timeout: 10000 });
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('应能通过 URL 打开结构对比窗口 (SD-001)', async () => {
    await browser.url('tauri://localhost/window.html?window=schema-diff');
    await browser.pause(1500);
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.title'));
    expect(body).toContain(t('schemaDiff.stepCompare'));
    await captureJourneyStep('schema-diff-window-open');
  });

  it('应显示对比 / 生成计划等主操作 (SD-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.compare'));
    expect(body).toContain(t('schemaDiff.generatePlan'));
    expect(body).toContain(t('schemaDiff.tables'));
  });

  it('未填表名点对比应提示必填 (SD-003)', async () => {
    const compareBtn = await $(`button*=${t('schemaDiff.compare')}`);
    await compareBtn.waitForDisplayed({ timeout: 8000 });
    await compareBtn.click();
    await browser.pause(500);
    const body = await $('body').getText();
    expect(body).toContain(t('schemaDiff.tableRequired'));
    await captureJourneyStep('schema-diff-table-required');
  });
});
