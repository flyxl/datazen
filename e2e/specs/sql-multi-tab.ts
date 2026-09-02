/**
 * SQL editor multi-tab — verify multiple tabs can coexist with
 * independent query results, and tab switching preserves state.
 *
 * Covers: TC-QUERY-009 ~ TC-QUERY-012
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import {
  captureJourneyStep,
  connectSeededPgInWorkspace,
  closeExtraWindows,
  executeSQL,
  openQueryTab,
} from '../helpers.js';

describe('SQL 多 Tab 并发 (TC-QUERY-009~012)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await browser.pause(300);
    await openQueryTab();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('TC-QUERY-009: 执行 SQL 应返回结果', async () => {
    await executeSQL('SELECT 1 AS tab1_result');
    await browser.pause(500);
    const body = await $('body').getText();
    expect(body).toContain('tab1_result');
  });

  it('TC-QUERY-010: 第二个 tab 执行不同查询', async () => {
    await openQueryTab();
    await browser.pause(500);
    await executeSQL('SELECT 2 AS tab2_result');
    await browser.pause(500);
    const body = await $('body').getText();
    expect(body).toContain('tab2_result');
  });

  it('TC-QUERY-011: 新建 tab 后编辑器应为空', async () => {
    await openQueryTab();
    await browser.pause(300);
    const editorText = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content') as HTMLElement;
      return el?.textContent?.trim() ?? '';
    });
    expect(editorText.includes('tab2_result')).toBe(false);
    expect(editorText.includes('tab1_result')).toBe(false);
    await captureJourneyStep('sql-new-empty-tab');
  });

  it('TC-QUERY-012: 切回首个 tab 结果仍保留', async () => {
    const tabs = await $$('[data-testid="panel-tab"]');
    expect(tabs.length).toBeGreaterThanOrEqual(2);
    await (await tabs[0].$('button')).click();
    await browser.pause(800);
    const preserved = await $('body').getText();
    expect(preserved).toContain('tab1_result');
    await captureJourneyStep('sql-tab-switch-preserved');

    await openQueryTab();
    await executeSQL('SELECT 123 AS independent_result');
    await browser.pause(500);
    const body = await $('body').getText();
    expect(body).toContain('independent_result');
  });
});
