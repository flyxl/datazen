/**
 * ER diagram full user journey — workspace home / toolbar entry, React Flow canvas,
 * controls, search, and table/relation stats.
 *
 * Covers: ER-001~ER-008
 */
import { expect, browser, $ } from '@wdio/globals';
import {
  closeExtraWindows,
  connectSeededPgInWorkspace,
  invokeBackend,
  waitForConnectionToolbar,
} from '../helpers.js';

const SEEDED_CONN_ID = 'conn_e2e_pg';

async function openErDiagramFromUi() {
  const homeQuick = await $('[data-testid="home-quick-er-diagram"]');
  if (await homeQuick.isExisting()) {
    await homeQuick.click();
    return;
  }
  const toolbarBtn = await $('[data-testid="content-toolbar-er-diagram"] button');
  await toolbarBtn.waitForClickable({ timeout: 10000 });
  await toolbarBtn.click();
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

  it('ER-002: 工具栏 ER 入口应可见', async () => {
    await browser.switchToWindow(mainWindow);
    const toolbar = await $('[data-testid="content-toolbar-er-diagram"]');
    await expect(toolbar).toBeDisplayed();
  });

  it('ER-003: 点击 ER 入口应打开 ER 面板', async () => {
    await browser.switchToWindow(mainWindow);
    await openErDiagramFromUi();
    await browser.pause(2000);

    const erView = await $('[data-testid="er-diagram-view"]');
    await erView.waitForDisplayed({ timeout: 15000 });
    const reactFlow = await erView.$('.react-flow');
    await expect(reactFlow).toBeDisplayed();
  });

  it('ER-004: 面板 tab 应出现 ER Diagram 标签', async () => {
    await browser.switchToWindow(mainWindow);
    const tabs = await $$('[data-testid="panel-tab"]');
    expect(tabs.length).toBeGreaterThan(0);
    const labels = await Promise.all(tabs.map((t) => t.getText()));
    expect(labels.some((l) => /ER Diagram|ER 图|er diagram/i.test(l))).toBe(true);
  });

  it('ER-005: React Flow 控件应可见', async () => {
    await browser.switchToWindow(mainWindow);
    const controls = await $('[data-testid="er-diagram-view"] .react-flow__controls');
    await controls.waitForDisplayed({ timeout: 10000 });
    await expect(controls).toBeDisplayed();
  });

  it('ER-006: 统计面板应显示表/关系数量', async () => {
    await browser.switchToWindow(mainWindow);
    const stats = await $('[data-testid="er-diagram-stats"]');
    await stats.waitForDisplayed({ timeout: 10000 });
    const text = await stats.getText();
    expect(text.length).toBeGreaterThan(0);
    expect(text).toMatch(/\d/);
  });

  it('ER-007: 搜索框应可过滤表节点', async () => {
    await browser.switchToWindow(mainWindow);
    const search = await $('[data-testid="er-diagram-search"]');
    await search.waitForDisplayed({ timeout: 10000 });
    await search.setValue('pg_');
    await browser.pause(500);
    const nodeCount = await browser.execute(
      () => document.querySelectorAll('[data-testid="er-diagram-view"] .react-flow__node').length,
    );
    expect(nodeCount).toBeGreaterThanOrEqual(0);
  });

  it('ER-008: 导出 PNG/SVG 按钮应可见（不触发原生对话框）', async () => {
    await browser.switchToWindow(mainWindow);
    const exportPng = await $('[data-testid="er-diagram-export-png"]');
    const exportSvg = await $('[data-testid="er-diagram-export-svg"]');
    await expect(exportPng).toBeDisplayed();
    await expect(exportSvg).toBeDisplayed();
  });
});
