import { expect, browser, $, $$ } from '@wdio/globals';
import { openConnectionWindow, closeExtraWindows, executeSQL, openQueryTab } from '../helpers.js';
import { t } from '../i18n.js';

describe('图表放大功能 (CHART-EXPAND)', () => {
  let mainWindow: string;
  let connWindow: string;

  before(async () => {
    const res = await openConnectionWindow();
    mainWindow = res.mainWindow;
    connWindow = res.connWindow;
    await openQueryTab();
    await browser.pause(1000);
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('应能切换到图表视图', async () => {
    await executeSQL('SELECT status, COUNT(*) as count FROM product GROUP BY status ORDER BY count DESC LIMIT 10');
    await browser.pause(1000);

    const chartBtn = await $(`button*=${t('chart.viewChart')}`);
    await chartBtn.waitForDisplayed({ timeout: 5000 });
    await chartBtn.click();
    await browser.pause(1000);

    const chartCanvas = await $('[class*="recharts-wrapper"]');
    await chartCanvas.waitForExist({ timeout: 5000 });
  });

  it('应显示放大按钮', async () => {
    const expandBtn = await $(`button[title="${t('chart.expand')}"]`);
    await expandBtn.waitForDisplayed({ timeout: 5000 });
  });

  it('点击放大按钮应打开全屏图表覆盖层', async () => {
    const expandBtn = await $(`button[title="${t('chart.expand')}"]`);
    await expandBtn.click();
    await browser.pause(500);

    const title = await $(`span*=${t('chart.expandTitle')}`);
    await title.waitForDisplayed({ timeout: 5000 });
  });

  it('放大视图应包含图表画布', async () => {
    const canvas = await $('[class*="recharts-wrapper"]');
    await canvas.waitForExist({ timeout: 5000 });
    const count = await $$('[class*="recharts-wrapper"]');
    expect(count.length).toBeGreaterThanOrEqual(1);
  });

  it('放大视图应有导出按钮', async () => {
    const pngBtn = await $('button*=PNG');
    await expect(pngBtn).toBeDisplayed();
    const svgBtn = await $('button*=SVG');
    await expect(svgBtn).toBeDisplayed();
  });

  it('按 ESC 应关闭放大视图', async () => {
    await browser.keys('Escape');
    await browser.pause(500);

    const title = await $(`span*=${t('chart.expandTitle')}`);
    const exists = await title.isExisting();
    expect(exists).toBe(false);
  });

  it('点击关闭按钮也应关闭放大视图', async () => {
    const expandBtn = await $(`button[title="${t('chart.expand')}"]`);
    await expandBtn.click();
    await browser.pause(500);

    const title = await $(`span*=${t('chart.expandTitle')}`);
    await title.waitForDisplayed({ timeout: 5000 });

    const closeBtn = await $(`button[title="${t('chart.collapse')}"]`);
    await closeBtn.click();
    await browser.pause(500);

    const titleAfter = await $(`span*=${t('chart.expandTitle')}`);
    const exists = await titleAfter.isExisting();
    expect(exists).toBe(false);
  });

  it('切回表格视图后放大按钮不可见', async () => {
    const tableBtn = await $(`button*=${t('chart.viewTable')}`);
    await tableBtn.click();
    await browser.pause(500);

    const expandBtn = await $(`button[title="${t('chart.expand')}"]`);
    const exists = await expandBtn.isExisting();
    expect(exists).toBe(false);
  });
});
