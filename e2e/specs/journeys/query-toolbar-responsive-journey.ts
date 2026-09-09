/**
 * Responsive query-toolbar user journey.
 *
 * The toolbar switches to icon-only actions below its measured threshold. The
 * test changes the real Tauri window size, verifies the compact state, and
 * proves the primary actions remain usable before restoring the original size.
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
  closeExtraWindows,
  executeSQL,
  invokeBackend,
  openConnectionWindow,
  openQueryTab,
} from '../../helpers.js';

async function setWindowSize(width: number, height: number) {
  await invokeBackend('plugin:window|set_size', { size: { width, height } });
  await browser.pause(600);
}

describe('查询工具栏响应式完整用户旅程 (QUERY-TOOLBAR-JOURNEY)', () => {
  let mainWindow: string;
  let originalSize: { width: number; height: number };

  before(async () => {
    const opened = await openConnectionWindow();
    mainWindow = opened.mainWindow;
    originalSize = await browser.getWindowSize();
    await openQueryTab();
  });

  after(async () => {
    try {
      await setWindowSize(originalSize.width, originalSize.height);
    } catch {
      /* best effort */
    }
    await closeExtraWindows(mainWindow);
  });

  it('完整旅程：缩窄窗口 → 工具栏进入 compact → 执行/历史仍可用 → 恢复窗口', async () => {
    const narrowWidth = Math.min(880, Math.max(760, originalSize.width - 240));
    await setWindowSize(narrowWidth, originalSize.height);

    const toolbar = await $('[data-testid="query-editor-toolbar"]');
    await toolbar.waitForDisplayed({ timeout: 10000 });
    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          const root = document.querySelector('[data-testid="query-editor-toolbar"]');
          const execute = root?.querySelector('[data-testid="editor-execute-button"]');
          const label = execute?.querySelector('span');
          return !!root && root.clientWidth < 920 && label?.classList.contains('sr-only') === true;
        }),
      { timeout: 10000, timeoutMsg: '窄窗口下查询工具栏未进入 compact 模式' },
    );
    await expect(await $('[data-testid="editor-execute-button"]')).toBeDisplayed();
    await expect(await $('[data-testid="editor-execution-strategy-button"]')).toBeDisplayed();
    await captureJourneyStep('query-toolbar-compact');

    await executeSQL('SELECT 1 AS compact_toolbar_value');
    const result = await $('[data-testid="result-workspace-table"]');
    await result.waitForDisplayed({ timeout: 15000 });
    expect(await result.getText()).toContain('compact_toolbar_value');
    await captureJourneyStep('query-toolbar-compact-query-success');

    const historyButton = await $('[data-testid="editor-history-toggle"]');
    await historyButton.click();
    await expect(await $('[data-testid="history-scope-current"]')).toBeDisplayed();
    await captureJourneyStep('query-toolbar-compact-history');

    await historyButton.click();
    await browser.waitUntil(
      async () => !(await $('[data-testid="history-scope-current"]').isExisting()),
      { timeout: 5000, timeoutMsg: '关闭历史面板超时' },
    );

    // Use a known-wide viewport for the positive expanded-state assertion. The
    // app may have been launched at its platform minimum size, where the
    // toolbar can legitimately remain compact even after "restoring" it.
    await setWindowSize(Math.max(originalSize.width, 1400), originalSize.height);
    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          const root = document.querySelector('[data-testid="query-editor-toolbar"]');
          const execute = root?.querySelector('[data-testid="editor-execute-button"]');
          const label = execute?.querySelector('span');
          return label?.classList.contains('sr-only') === false;
        }),
      { timeout: 10000, timeoutMsg: '恢复窗口宽度后查询工具栏未恢复标签' },
    );
    await captureJourneyStep('query-toolbar-expanded-wide');

    await setWindowSize(originalSize.width, originalSize.height);
    await expect(toolbar).toBeDisplayed();
    await captureJourneyStep('query-toolbar-restored');
  });
});
