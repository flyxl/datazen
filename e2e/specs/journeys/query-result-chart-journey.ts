/**
 * Query → result → chart → export user journey.
 *
 * This composes the existing SQL, result workspace, chart, and export paths
 * into one user-visible flow. It intentionally uses a small deterministic
 * result set so the journey stays focused on transitions rather than data
 * volume.
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  closeDataExportDialogIfOpen,
  closeExtraWindows,
  connectBackend,
  disconnectBackend,
  executeSQL,
  executeQuery,
  openConnectionWindow,
  openQueryTab,
  waitForDataExportDialog,
} from '../../helpers.js';

describe('查询→结果→图表→导出完整用户旅程 (QUERY-RESULT-JOURNEY)', () => {
  let mainWindow: string;
  const tableName = `e2e_query_chart_journey_${Date.now().toString(36)}`;

  before(async () => {
    const dbSessionId = await connectBackend('conn_e2e_pg');
    try {
      await executeQuery(
        dbSessionId,
        `CREATE TABLE IF NOT EXISTS ${tableName} (category TEXT, amount INTEGER)`,
      );
      await executeQuery(dbSessionId, `DELETE FROM ${tableName} WHERE TRUE`);
      await executeQuery(
        dbSessionId,
        `INSERT INTO ${tableName} (category, amount) VALUES ('Alpha', 100), ('Beta', 200), ('Gamma', 150), ('Delta', 300)`,
      );
    } finally {
      await disconnectBackend(dbSessionId);
    }

    const opened = await openConnectionWindow();
    mainWindow = opened.mainWindow;
    await openQueryTab();
  });

  after(async () => {
    try {
      await closeDataExportDialogIfOpen();
      const dbSessionId = await connectBackend('conn_e2e_pg');
      try {
        await executeQuery(dbSessionId, `DROP TABLE IF EXISTS ${tableName}`);
      } finally {
        await disconnectBackend(dbSessionId);
      }
    } catch {
      /* best effort; the shared E2E teardown also removes journey tables */
    }
    await closeExtraWindows(mainWindow);
  });

  it('完整旅程：执行查询 → 查看表格结果 → 切换图表 → 返回并打开导出', async () => {
    await executeSQL(`SELECT category, amount FROM ${tableName} ORDER BY category`);

    const table = await $('[data-testid="result-workspace-table"]');
    await table.waitForDisplayed({ timeout: 10000 });
    const tableText = await table.getText();
    expect(tableText).toContain('category');
    expect(tableText).toContain('Alpha');
    expect(tableText).toContain('300');
    await captureJourneyStep('query-result-table-visible');

    const chartButton = await $('[data-testid="result-workspace-view-chart"]');
    await chartButton.waitForClickable({ timeout: 5000 });
    await chartButton.click();
    await $('[class*="recharts-wrapper"]').waitForExist({ timeout: 10000 });
    expect(await chartButton.getAttribute('aria-pressed')).toBe('true');
    await captureJourneyStep('query-result-chart-visible');

    const tableButton = await $('[data-testid="result-workspace-view-table"]');
    await tableButton.click();
    await browser.pause(400);
    expect(await tableButton.getAttribute('aria-pressed')).toBe('true');

    const exportButton = await $('[data-testid="data-table-export"]');
    await exportButton.waitForClickable({ timeout: 5000 });
    await exportButton.click();
    await waitForDataExportDialog();
    await expect(await $('[data-testid="data-export-dialog"]')).toBeDisplayed();
    await captureJourneyStep('query-result-export-dialog');
    await closeDataExportDialogIfOpen();
  });
});
