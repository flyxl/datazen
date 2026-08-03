import { expect, browser, $, $$ } from '@wdio/globals';
import { openConnectionWindow, closeExtraWindows, executeSQL, openQueryTab } from '../helpers.js';

describe('图表放大功能 (CHART-EXPAND)', () => {
  let mainWindow: string;
  let connWindow: string;

  before(async () => {
    const res = await openConnectionWindow();
    mainWindow = res.mainWindow;
    connWindow = res.connWindow;
    await openQueryTab();
    await browser.pause(1000);

    // Create test table and insert data in a single SQL statement
    await executeSQL(`
      CREATE TABLE IF NOT EXISTS e2e_chart_test (category TEXT, amount INTEGER);
      DELETE FROM e2e_chart_test;
      INSERT INTO e2e_chart_test (category, amount) VALUES
        ('Alpha', 100), ('Beta', 200), ('Gamma', 150), ('Delta', 300), ('Epsilon', 250);
    `);
    await browser.pause(500);
    await openQueryTab();
    await browser.pause(500);
  });

  after(async () => {
    // Cleanup the test table
    try {
      await openQueryTab();
      await executeSQL('DROP TABLE IF EXISTS e2e_chart_test');
    } catch { /* ignore */ }
    await closeExtraWindows(mainWindow);
  });

  it('应能切换到图表视图', async () => {
    await executeSQL('SELECT category, amount FROM e2e_chart_test ORDER BY category');
    await browser.pause(1000);

    const chartBtn = await $('button*=图表');
    await chartBtn.waitForDisplayed({ timeout: 5000 });
    await chartBtn.click();
    await browser.pause(1000);

    const chartCanvas = await $('[class*="recharts-wrapper"]');
    await chartCanvas.waitForExist({ timeout: 5000 });
  });

  it('应显示放大按钮', async () => {
    const expandBtn = await $('button[title="放大显示"]');
    await expandBtn.waitForDisplayed({ timeout: 5000 });
  });

  it('点击放大按钮应打开全屏图表覆盖层', async () => {
    const expandBtn = await $('button[title="放大显示"]');
    await expandBtn.click();
    await browser.pause(500);

    const title = await $('span*=图表预览');
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

    const title = await $('span*=图表预览');
    const exists = await title.isExisting();
    expect(exists).toBe(false);
  });

  it('点击关闭按钮也应关闭放大视图', async () => {
    const expandBtn = await $('button[title="放大显示"]');
    await expandBtn.click();
    await browser.pause(500);

    const title = await $('span*=图表预览');
    await title.waitForDisplayed({ timeout: 5000 });

    const closeBtn = await $('button[title="退出放大"]');
    await closeBtn.click();
    await browser.pause(500);

    const titleAfter = await $('span*=图表预览');
    const exists = await titleAfter.isExisting();
    expect(exists).toBe(false);
  });

  it('切回表格视图后放大按钮不可见', async () => {
    const tableBtn = await $('button*=表格');
    await tableBtn.click();
    await browser.pause(500);

    const expandBtn = await $('button[title="放大显示"]');
    const exists = await expandBtn.isExisting();
    expect(exists).toBe(false);
  });
});
