/**
 * Query error recovery user journey.
 *
 * The AI provider is deliberately not part of this deterministic journey:
 * provider-backed calls are covered by the conditional AI specs. This test
 * verifies the implemented local recovery path a user can always complete.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  executeSQL,
  openConnectionWindow,
  openQueryTab,
  setEditorContent,
} from '../../helpers.js';

describe('查询错误恢复完整用户旅程 (QUERY-RECOVERY-JOURNEY)', () => {
  let mainWindow: string;

  before(async () => {
    const opened = await openConnectionWindow();
    mainWindow = opened.mainWindow;
    await openQueryTab();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('完整旅程：执行失败 → 复制错误 → 修正 SQL → 重新执行成功', async () => {
    await setEditorContent('SELECT * FROM e2e_query_recovery_missing_table');
    const executeButton = await $('[data-testid="editor-execute-button"]');
    await executeButton.click();

    const errorPanel = await $('[data-testid="query-error-message"]');
    await errorPanel.waitForDisplayed({ timeout: 15000 });
    expect((await errorPanel.getText()).toLowerCase()).toContain(
      'e2e_query_recovery_missing_table',
    );
    await expect(await $('[data-testid="query-retry"]')).toBeDisplayed();
    await expect(await $('[data-testid="query-copy-error"]')).toBeDisplayed();
    await captureJourneyStep('query-recovery-error-visible');

    await $('[data-testid="query-copy-error"]').click();
    await expect(await $(`button*=${t('common.copied')}`)).toBeDisplayed();
    await captureJourneyStep('query-recovery-error-copied');

    await setEditorContent('SELECT 42 AS recovered_value');
    await $('[data-testid="editor-execute-button"]').click();
    const result = await $('[data-testid="result-workspace-table"]');
    await result.waitForDisplayed({ timeout: 15000 });
    await browser.waitUntil(
      async () => !(await $('[data-testid="query-error-message"]').isExisting()),
      { timeout: 5000, timeoutMsg: '修正 SQL 后错误面板仍然存在' },
    );
    const resultText = await result.getText();
    expect(resultText).toContain('recovered_value');
    expect(resultText).toContain('42');
    await captureJourneyStep('query-recovery-success');
  });
});
