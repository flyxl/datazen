/** Query cancellation and multi-tab state-isolation journey. */
import { expect, browser, $, $$ } from '@wdio/globals';
import {
  captureJourneyStep,
  closeExtraWindows,
  connectSeededPgInWorkspace,
  executeSQL,
  openQueryTab,
  setEditorContent,
} from '../../helpers.js';

describe('SQL 查询边界完整用户旅程 (QUERY-EDGE-JOURNEY, TC-QUERY-006, TC-QUERY-009~012)', () => {
  let mainWindow: string;
  let firstJourneyTabIndex: number;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await openQueryTab();
    const tabCount = await browser.execute(
      () => document.querySelectorAll('[data-testid="panel-tab"]').length,
    );
    firstJourneyTabIndex = tabCount - 1;
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.refresh();
  });

  it('完整 edge：取消长查询 → 恢复执行 → 新 Tab 隔离 → 切回结果仍保留', async () => {
    await setEditorContent('SELECT pg_sleep(10)');
    await $('[data-testid="editor-execute-button"]').click();
    const stopButton = await $('[data-testid="editor-stop-button"]');
    await stopButton.waitForDisplayed({ timeout: 5000 });
    await captureJourneyStep('query-edge-running');
    await stopButton.click();

    await browser.waitUntil(
      async () =>
        await $('[data-testid="editor-execute-button"]')
          .isDisplayed()
          .catch(() => false),
      { timeout: 10000, timeoutMsg: '取消查询后执行按钮未恢复' },
    );
    await executeSQL('SELECT 99 AS cancel_recovered_value');
    expect(await $('[data-testid="result-workspace-table"]').getText()).toContain(
      'cancel_recovered_value',
    );
    await captureJourneyStep('query-edge-cancel-recovered');

    await openQueryTab();
    const emptyEditorText = await browser.execute(
      () =>
        document.querySelector<HTMLElement>('.cm-editor .cm-content')?.textContent?.trim() ?? '',
    );
    expect(emptyEditorText).not.toContain('cancel_recovered_value');
    await executeSQL('SELECT 2 AS isolated_second_tab');
    expect(await $('[data-testid="result-workspace-table"]').getText()).toContain(
      'isolated_second_tab',
    );
    await captureJourneyStep('query-edge-second-tab');

    const tabs = await $$('[data-testid="panel-tab"]');
    const tabCount = await browser.execute(
      () => document.querySelectorAll('[data-testid="panel-tab"]').length,
    );
    expect(tabCount).toBeGreaterThan(firstJourneyTabIndex + 1);
    await (await tabs[firstJourneyTabIndex].$('button')).click();
    await browser.waitUntil(
      async () => (await $('body').getText()).includes('cancel_recovered_value'),
      { timeout: 5000, timeoutMsg: '切回首个查询 Tab 后结果未保留' },
    );
    expect(await $('body').getText()).not.toContain('isolated_second_tab');
    await captureJourneyStep('query-edge-first-tab-restored');
  });
});
