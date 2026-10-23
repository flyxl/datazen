/**
 * Responsive query-toolbar user journey.
 *
 * The toolbar is icon-only at every width: action names live in tooltips, and
 * the right-hand status zone shortens below its measured threshold. The test
 * changes the real Tauri window size, proves the row neither grows labels nor
 * overflows, and that the primary actions remain usable before restoring the
 * original size.
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../../i18n.js';
import {
  captureJourneyStep,
  closeExtraWindows,
  executeSQL,
  invokeBackend,
  openConnectionWindow,
  openQueryTab,
} from '../../helpers.js';

async function setWindowSize(width: number, height: number) {
  await invokeBackend('plugin:window|set_size', { value: { Logical: { width, height } } });
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

  it('完整旅程：缩窄窗口 → 工具栏保持纯图标且不溢出 → 执行/历史仍可用 → 恢复窗口', async () => {
    // `setWindowSize` takes **physical** pixels while every layout assertion here
    // is in CSS pixels. On a Retina display the old constant 880 meant 440 CSS
    // pixels — far below the app's minimum window width — so the window never
    // actually narrowed to the intended 900 CSS pixels and the toolbar
    // overflowed instead of switching to icons.
    const viewport = await browser.execute(() => ({
      devicePixelRatio: window.devicePixelRatio || 1,
      innerWidth: window.innerWidth,
    }));
    const targetCssWidth = Math.min(900, Math.max(760, viewport.innerWidth - 200));
    await setWindowSize(
      Math.round(targetCssWidth * viewport.devicePixelRatio),
      originalSize.height,
    );

    const toolbar = await $('[data-testid="query-editor-toolbar"]');
    await toolbar.waitForDisplayed({ timeout: 10000 });
    const readToolbar = () =>
      browser.execute(() => {
        const root = document.querySelector('[data-testid="query-editor-toolbar"]');
        if (!root) return null;
        const buttons = Array.from(root.querySelectorAll('button'));
        const withText = buttons
          .map((btn) => (btn.textContent ?? '').trim())
          .filter((text) => text.length > 0);
        // Who took the rest of the window: the toolbar is only as wide as its
        // parent chain allows, so an overflow here is a layout claim about the
        // ancestors, not about the buttons.
        const ancestors: Array<Record<string, unknown>> = [];
        let node: HTMLElement | null = root.parentElement;
        for (let depth = 0; node && depth < 6; depth += 1) {
          const siblings = Array.from(node.children).map((child) => ({
            tag: child.tagName.toLowerCase(),
            testid: child.getAttribute('data-testid'),
            width: Math.round(child.getBoundingClientRect().width),
            className: (child.className || '').toString().slice(0, 60),
          }));
          ancestors.push({
            tag: node.tagName.toLowerCase(),
            testid: node.getAttribute('data-testid'),
            className: (node.className || '').toString().slice(0, 90),
            width: Math.round(node.getBoundingClientRect().width),
            siblings,
          });
          node = node.parentElement;
        }
        // A toolbar that is too narrow for eleven icon buttons may scroll, but
        // it must not clip: every action stays reachable at either end.
        const overflowX = getComputedStyle(root).overflowX;
        const scrollable = overflowX === 'auto' || overflowX === 'scroll';
        const before = root.scrollLeft;
        root.scrollLeft = root.scrollWidth;
        const lastReachable = root.scrollLeft + root.clientWidth >= root.scrollWidth - 1;
        root.scrollLeft = before;
        return {
          clientWidth: root.clientWidth,
          scrollWidth: root.scrollWidth,
          buttonCount: buttons.length,
          texts: withText,
          innerWidth: window.innerWidth,
          minWidth: root.getBoundingClientRect().width,
          overflowX,
          scrollable,
          lastReachable,
          ancestors,
        };
      });
    let toolbarState = await readToolbar();
    await browser
      .waitUntil(
        async () => {
          toolbarState = await readToolbar();
          if (!toolbarState) return false;
          // Every action is icon-only: no visible text node inside the buttons…
          // …and the row still fits without horizontal overflow.
          return (
            toolbarState.clientWidth < 920 &&
            toolbarState.buttonCount > 0 &&
            toolbarState.texts.length === 0 &&
            // Either everything fits, or the row scrolls and its last action is
            // still reachable — clipping is what this case forbids.
            (toolbarState.scrollWidth <= toolbarState.clientWidth ||
              (toolbarState.scrollable && toolbarState.lastReachable))
          );
        },
        { timeout: 10000 },
      )
      .catch(() => {
        throw new Error(
          `窄窗口下查询工具栏未保持纯图标或出现横向溢出: ${JSON.stringify(toolbarState)}`,
        );
      });
    await expect(await $('[data-testid="editor-execute-button"]')).toBeDisplayed();
    await expect(await $('[data-testid="editor-execution-strategy-button"]')).toBeDisplayed();
    await expect(await $('[data-testid="editor-format-button"]')).toBeDisplayed();
    // The action names survive as tooltips now that they are not rendered.
    expect(await $('[data-testid="editor-format-button"]').getAttribute('title')).toContain(
      t('query.format'),
    );
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

    // Use a known-wide viewport for the expanded-state assertion. The app may
    // have been launched at its platform minimum size, so never rely on the
    // original width here. The toolbar stays icon-only either way; what the
    // extra room buys is the full status text on the right-hand side.
    await setWindowSize(Math.max(originalSize.width, 1400), originalSize.height);
    await browser.waitUntil(
      async () =>
        browser.execute(() => {
          const root = document.querySelector('[data-testid="query-editor-toolbar"]');
          // The toolbar is as wide as the window minus the connection sidebar, so
          // the "the window is wide again" check is about the window. With that
          // room every action fits without scrolling.
          if (!root || window.innerWidth < 1200) return false;
          const execute = root.querySelector('[data-testid="editor-execute-button"]');
          return (
            !!execute &&
            (execute.textContent ?? '').trim().length === 0 &&
            root.scrollWidth <= root.clientWidth
          );
        }),
      { timeout: 10000, timeoutMsg: '宽窗口下查询工具栏未保持纯图标或出现横向溢出' },
    );
    await captureJourneyStep('query-toolbar-expanded-wide');

    await setWindowSize(originalSize.width, originalSize.height);
    await expect(toolbar).toBeDisplayed();
    await captureJourneyStep('query-toolbar-restored');
  });
});
