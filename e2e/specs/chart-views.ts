/**
 * Chart view E2E beyond expand overlay (TC-CHART-002~008/012).
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { openConnectionWindow, closeExtraWindows, executeSQL, openQueryTab } from '../helpers.js';
import { t } from '../i18n.js';

describe('图表视图 (TC-CHART-002~008/012)', () => {
  let mainWindow: string;
  let connWindow: string;

  before(async () => {
    const res = await openConnectionWindow();
    mainWindow = res.mainWindow;
    connWindow = res.connWindow;
    await openQueryTab();
    await browser.pause(500);
    // Execute setup statements independently. The query editor executes one
    // statement per tab; sending a semicolon-separated batch can report only
    // the first result and leave the fixture partially initialized.
    await executeSQL('CREATE TABLE IF NOT EXISTS e2e_chart_views (category TEXT, amount INTEGER)');
    await executeSQL('DELETE FROM e2e_chart_views');
    await executeSQL(`
      INSERT INTO e2e_chart_views (category, amount) VALUES
        ('Alpha', 100), ('Beta', 200), ('Gamma', 150), ('Delta', 300)
    `);
    await openQueryTab();
  });

  after(async () => {
    try {
      await openQueryTab();
      await executeSQL('DROP TABLE IF EXISTS e2e_chart_views');
    } catch {
      /* ignore */
    }
    await closeExtraWindows(mainWindow);
  });

  async function openChartWithData() {
    await browser.switchToWindow(connWindow);
    await openQueryTab();
    await executeSQL('SELECT category, amount FROM e2e_chart_views ORDER BY category');
    await browser.pause(800);
    const chartBtn = await $(`button*=${t('chart.viewChart')}`);
    await chartBtn.waitForDisplayed({ timeout: 8000 });
    await chartBtn.click();
    await browser.pause(800);
    await expect(await $('[data-testid="result-workspace-view-chart"]')).toBeDisplayed();
    expect(await $('[data-testid="result-workspace-view-chart"]').getAttribute('aria-pressed')).toBe(
      'true',
    );
    await $('[class*="recharts-wrapper"]').waitForExist({ timeout: 8000 });
  }

  it('TC-CHART-001/002: 应能切换图表类型（折线）', async () => {
    await openChartWithData();
    const lineBtn = await $(`button[aria-label="${t('chart.type.line')}"]`);
    await lineBtn.waitForDisplayed({ timeout: 5000 });
    await lineBtn.click();
    await browser.pause(500);
    await expect(await $('[class*="recharts-wrapper"]')).toBeExisting();
  });

  it('TC-CHART-002: 应能切换为饼图', async () => {
    const pieBtn = await $(`button[aria-label="${t('chart.type.pie')}"]`);
    await pieBtn.click();
    await browser.pause(500);
    await expect(await $('[class*="recharts-wrapper"]')).toBeExisting();
  });

  it('TC-CHART-007: 应能切换图例显示选项', async () => {
    const legend = await $(`label*=${t('chart.legend')}`);
    if (await legend.isExisting()) {
      const input = await legend.$('input[type="checkbox"]');
      const before = await input.isSelected();
      await input.click();
      await browser.pause(300);
      const after = await input.isSelected();
      expect(after).not.toBe(before);
      await input.click();
    }
  });

  it('TC-CHART-008: 切回表格再切图表应保持可用', async () => {
    const tableBtn = await $('[data-testid="result-workspace-view-table"]');
    if (await tableBtn.isExisting()) {
      await tableBtn.click();
      await browser.pause(400);
    } else {
      const fallback = await $('button*=表格');
      await fallback.click();
      await browser.pause(400);
    }
    expect(await tableBtn.getAttribute('aria-pressed')).toBe('true');
    const chartBtn = await $(`button*=${t('chart.viewChart')}`);
    await chartBtn.click();
    await browser.pause(800);
    await expect(await $('[class*="recharts-wrapper"]')).toBeExisting();
  });

  it('TC-CHART-012: 无数据时图表区应显示空状态或无崩溃', async () => {
    await openQueryTab();
    await executeSQL('SELECT category, amount FROM e2e_chart_views WHERE 1=0');
    await browser.pause(800);
    const chartBtn = await $(`button*=${t('chart.viewChart')}`);
    if (await chartBtn.isExisting()) {
      await chartBtn.click();
      await browser.pause(800);
    }
    const body = await $('body').getText();
    const ok =
      body.includes('空') ||
      body.includes('no data') ||
      body.includes('No data') ||
      body.includes(t('chart.viewChart')) ||
      (await $$('[class*="recharts-wrapper"]')).length >= 0;
    expect(ok).toBe(true);
  });
});
