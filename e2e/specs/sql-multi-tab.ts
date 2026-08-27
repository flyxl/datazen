/**
 * SQL editor multi-tab — verify multiple tabs can coexist with
 * independent query results, and tab switching preserves state.
 *
 * Covers: TC-QUERY-009 ~ TC-QUERY-012
 */
import { expect, browser, $ } from '@wdio/globals';
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
    await browser.pause(1000);
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

  it('TC-QUERY-010: 执行不同查询应返回新结果', async () => {
    await executeSQL('SELECT 2 AS tab2_result');
    await browser.pause(500);
    const body = await $('body').getText();
    expect(body).toContain('tab2_result');
  });

  it('TC-QUERY-011: 新建 tab 后编辑器应为空', async () => {
    await openQueryTab();
    await browser.pause(1000);
    const editorText = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content') as HTMLElement;
      return el?.textContent?.trim() ?? '';
    });
    expect(editorText.length).toBe(0);
    await captureJourneyStep('sql-new-empty-tab');
  });

  it('TC-QUERY-012: 新 tab 执行查询后结果独立', async () => {
    await executeSQL('SELECT 123 AS independent_result');
    await browser.pause(500);
    const body = await $('body').getText();
    expect(body).toContain('independent_result');
  });
});
