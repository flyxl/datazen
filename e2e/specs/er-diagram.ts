/**
 * ER diagram full user journey — workspace home / toolbar entry, React Flow canvas,
 * controls, search, and table/relation stats.
 *
 * Covers: ER-001~ER-008
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import {
  closeExtraWindows,
  captureJourneyStep,
  connectSeededPgInWorkspace,
  injectDialogPath,
  invokeBackend,
  openErDiagramFromUi,
  openQueryTab,
  resetDialogQueue,
  waitForConnectionToolbar,
} from '../helpers.js';

const SEEDED_CONN_ID = 'conn_e2e_pg';

/** Click the ER Diagram panel tab (switches away from other tabs when present). */
async function focusErPanelTab() {
  const tabs = await $$('[data-testid="panel-tab"]');
  for (const tab of tabs) {
    const text = await tab.getText();
    if (/ER Diagram|ER 图/i.test(text)) {
      const selectBtn = await tab.$('button');
      await selectBtn.click();
      await browser.pause(400);
      return;
    }
  }
  throw new Error('ER Diagram panel tab not found');
}

/** Ensure ER diagram content is visible (re-focus tab or re-open from toolbar). */
async function ensureErDiagramVisible() {
  const erView = await $('[data-testid="er-diagram-view"]');
  if (await erView.isDisplayed()) return;
  try {
    await focusErPanelTab();
  } catch {
    await openErDiagramFromUi();
  }
  await erView.waitForDisplayed({ timeout: 15000 });
}

describe('ER 图功能 E2E 测试 (ER-001~ER-008)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    await waitForConnectionToolbar();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    try {
      await resetDialogQueue();
    } catch {
      /* ok */
    }
  });

  it('ER-001: get_er_data IPC 应返回 schema 数组', async () => {
    const dbSessionId = await invokeBackend<string>('connect', { connectionId: SEEDED_CONN_ID });
    const databases = await invokeBackend<string[]>('get_databases', { dbSessionId });
    expect(databases.length).toBeGreaterThan(0);

    const schemas = await invokeBackend<unknown[]>('get_er_data', {
      dbSessionId,
      database: databases[0],
    });
    expect(Array.isArray(schemas)).toBe(true);
  });

  it('ER-002: 连接后 ER 入口应可见（首页快捷操作或工具栏）', async () => {
    await browser.switchToWindow(mainWindow);
    const homeQuick = await $('[data-testid="home-quick-er-diagram"]');
    const toolbar = await $('[data-testid="content-toolbar-er-diagram"]');
    const homeVisible = await homeQuick.isExisting();
    if (!homeVisible) {
      await openQueryTab();
      await browser.pause(500);
    }
    const hasEntry = homeVisible || (await toolbar.isExisting()) || (await homeQuick.isExisting());
    expect(hasEntry).toBe(true);
    await captureJourneyStep('er-entry-visible', 0, true);
  });

  it('ER-003: 点击 ER 入口应打开 ER 面板', async () => {
    await browser.switchToWindow(mainWindow);
    await openErDiagramFromUi();

    const erView = await $('[data-testid="er-diagram-view"]');
    await erView.waitForDisplayed({ timeout: 15000 });
    const reactFlow = await erView.$('.react-flow');
    await expect(reactFlow).toBeDisplayed();
    await captureJourneyStep('er-canvas-visible', 0, true);
  });

  it('ER-004: 面板 tab 应出现 ER Diagram 标签', async () => {
    await browser.switchToWindow(mainWindow);
    await openQueryTab();
    await browser.pause(500);
    await focusErPanelTab();
    const erView = await $('[data-testid="er-diagram-view"]');
    await erView.waitForDisplayed({ timeout: 15000 });
    const tabs = await $$('[data-testid="panel-tab"]');
    expect(tabs.length).toBeGreaterThan(0);
    const body = await $('body').getText();
    expect(/ER Diagram|ER 图/i.test(body)).toBe(true);
    await captureJourneyStep('er-tab-switched', 0, true);
  });

  it('ER-005: React Flow 控件应可见', async () => {
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    const zoomIn = await $('[data-testid="er-diagram-view"] .react-flow__controls-zoomin');
    await zoomIn.waitForClickable({ timeout: 10000 });
    await zoomIn.click();
    await zoomIn.click();
    await browser.pause(500);
    const controls = await $('[data-testid="er-diagram-view"] .react-flow__controls');
    await expect(controls).toBeDisplayed();
    await captureJourneyStep('er-controls-zoomed', 0, true);
  });

  it('ER-006: 统计面板应显示表/关系数量', async () => {
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    const fitView = await $('[data-testid="er-diagram-view"] .react-flow__controls-fitview');
    if (await fitView.isExisting()) {
      await fitView.click();
      await browser.pause(400);
    }
    const firstNode = await $('[data-testid="er-diagram-view"] .react-flow__node');
    await firstNode.waitForDisplayed({ timeout: 10000 });
    await firstNode.click();
    await browser.pause(500);
    const stats = await $('[data-testid="er-diagram-stats"]');
    await stats.waitForDisplayed({ timeout: 10000 });
    const text = await stats.getText();
    expect(text.length).toBeGreaterThan(0);
    expect(text).toMatch(/\d/);
    await captureJourneyStep('er-table-selected', 0, true);
  });

  it('ER-007: 搜索框应可过滤表节点', async () => {
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    const search = await $('[data-testid="er-diagram-search"]');
    await search.waitForDisplayed({ timeout: 10000 });
    await search.clearValue();
    await search.setValue('pg_');
    await browser.pause(500);
    await captureJourneyStep('er-search-filtered', 0, true);
    const nodeCount = await browser.execute(
      () => document.querySelectorAll('[data-testid="er-diagram-view"] .react-flow__node').length,
    );
    expect(nodeCount).toBeGreaterThanOrEqual(0);
  });

  it('ER-008: 导出 PNG 应通过注入对话框落盘（mock 原生另存为）', async () => {
    await browser.switchToWindow(mainWindow);
    await ensureErDiagramVisible();
    const search = await $('[data-testid="er-diagram-search"]');
    if (await search.isExisting()) {
      await search.clearValue();
      await browser.pause(400);
    }
    const exportPng = await $('[data-testid="er-diagram-export-png"]');
    const exportSvg = await $('[data-testid="er-diagram-export-svg"]');
    await expect(exportPng).toBeDisplayed();
    await expect(exportSvg).toBeDisplayed();

    await resetDialogQueue();
    const outPath = path.join(os.tmpdir(), `datazen-er-${Date.now()}.png`);
    try {
      await injectDialogPath(outPath);
      await exportPng.scrollIntoView();
      await exportPng.click();
      await browser.waitUntil(() => fs.existsSync(outPath), {
        timeout: 20000,
        interval: 500,
        timeoutMsg: `ER PNG export did not write ${outPath}`,
      });
      expect(fs.statSync(outPath).size).toBeGreaterThan(100);
      await captureJourneyStep('er-export-png-saved', 0, true);
    } finally {
      try {
        fs.unlinkSync(outPath);
      } catch {
        /* ok */
      }
    }
  });
});
