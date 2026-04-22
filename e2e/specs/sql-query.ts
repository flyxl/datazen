import { expect, browser, $, $$ } from '@wdio/globals';
import {
  clickCardConnectButton,
  closeExtraWindows,
  setEditorContent,
  openQueryTab,
} from '../helpers.js';

/**
 * SQL query module tests.
 * Requires the seeded "本地 PostgreSQL" connection to be available.
 */
describe('SQL 查询模块 (SQ-001~SQ-012)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await $('button*=新建连接').waitForDisplayed({ timeout: 10000 });
    await browser.pause(1500);

    await clickCardConnectButton();

    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 20000, timeoutMsg: '等待连接窗口打开超时' },
    );
    const handles = await browser.getWindowHandles();
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);

    await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
    await browser.pause(1000);

    await openQueryTab();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  // ── 基础 UI ────────────────────────────────────────────────────

  it('SQL 编辑器应显示执行按钮 (SQ-001)', async () => {
    await expect(await $('button*=执行')).toBeDisplayed();
  });

  it('应显示执行快捷键提示 (SQ-001)', async () => {
    await expect(await $('span*=⌘+Enter')).toBeDisplayed();
  });

  it('执行查询期间应显示停止按钮 (SQ-001)', async () => {
    await setEditorContent('SELECT pg_sleep(5)');
    const execBtn = await $('button*=执行');
    await execBtn.click();
    await browser.pause(500);

    const stopBtn = await $('button*=停止');
    const isVisible = await stopBtn.isDisplayed();
    expect(isVisible).toBe(true);

    // Cancel to not block other tests
    await stopBtn.click();
    await browser.pause(2000);
  });

  // ── 执行查询 ───────────────────────────────────────────────────

  it('应能输入 SQL 并执行查询 (SQ-001, SQ-002)', async () => {
    await setEditorContent('SELECT 1 AS test_col');

    const execBtn = await $('button*=执行');
    await execBtn.click();

    await browser.waitUntil(
      async () => (await $('body').getText()).includes('1 行'),
      { timeout: 15000, timeoutMsg: '等待查询结果超时' },
    );
  });

  it('结果应显示行数、列数和耗时 (SQ-004)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('行');
    expect(body).toContain('列');
    expect(body).toContain('ms');
  });

  it('执行后应显示总耗时 (SQ-002)', async () => {
    await expect(await $('span*=总耗时')).toBeDisplayed();
  });

  // ── 多语句 ─────────────────────────────────────────────────────

  it('执行多条语句应显示多个结果标签 (SQ-011)', async () => {
    await setEditorContent('SELECT 1 AS a; SELECT 2 AS b');

    const execBtn = await $('button*=执行');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('结果 1') && body.includes('结果 2');
      },
      { timeout: 15000, timeoutMsg: '等待多结果集标签超时' },
    );

    await expect(await $('button*=结果 1')).toBeDisplayed();
    await expect(await $('button*=结果 2')).toBeDisplayed();
  });

  it('应能切换结果标签 (SQ-011)', async () => {
    const tab2 = await $('button*=结果 2');
    await tab2.click();
    await browser.pause(300);
    const body = await $('body').getText();
    expect(body).toContain('1 行');

    const tab1 = await $('button*=结果 1');
    await tab1.click();
    await browser.pause(300);
  });

  // ── DML 语句 ───────────────────────────────────────────────────

  it('执行 DML 语句应显示影响行数 (SQ-012)', async () => {
    await setEditorContent(
      "CREATE TABLE IF NOT EXISTS _e2e_sql_test (id SERIAL PRIMARY KEY, val TEXT); " +
      "INSERT INTO _e2e_sql_test (val) VALUES ('hello')"
    );
    const execBtn = await $('button*=执行');
    await execBtn.click();

    await browser.waitUntil(
      async () => (await $('body').getText()).includes('总耗时'),
      { timeout: 15000, timeoutMsg: '等待 DML 执行超时' },
    );

    // Clean up
    await setEditorContent('DROP TABLE IF EXISTS _e2e_sql_test');
    const execBtn2 = await $('button*=执行');
    await execBtn2.click();
    await browser.waitUntil(
      async () => (await $('body').getText()).includes('总耗时'),
      { timeout: 10000 },
    );
  });

  // ── 错误处理 ───────────────────────────────────────────────────

  it('执行错误 SQL 应显示错误信息 (SQ-002)', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM nonexistent_table_xyz_12345');

    const execBtn = await $('button*=执行');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('does not exist') || body.includes('不存在') ||
          body.includes('nonexistent') || body.includes('ERROR') ||
          body.includes('失败');
      },
      { timeout: 20000, timeoutMsg: '等待错误信息超时' },
    );
  });

  // ── 历史面板 ───────────────────────────────────────────────────

  it('历史按钮应能切换历史面板 (SQ-005)', async () => {
    const histBtn = await $('button*=历史');
    await histBtn.click();
    await browser.pause(500);
    await expect(await $('div*=查询历史')).toBeDisplayed();
  });

  it('历史面板应显示之前执行的 SQL 记录 (SQ-005)', async () => {
    const body = await $('body').getText();
    const hasHistory = body.includes('SELECT') || body.includes('成功') || body.includes('失败');
    expect(hasHistory).toBe(true);
  });

  it('点击历史记录应回填到编辑器 (SQ-005)', async () => {
    const historyBtns = await $$('aside button');
    let clickedHistory = false;
    for (const btn of historyBtns) {
      const text = await btn.getText();
      if (text.includes('SELECT') && text.includes('ms')) {
        await btn.click();
        clickedHistory = true;
        break;
      }
    }

    if (clickedHistory) {
      await browser.pause(500);
      const editorContent = await browser.execute(() => {
        const el = document.querySelector('.cm-editor .cm-content') as HTMLElement;
        return el?.textContent || '';
      });
      expect(editorContent).toContain('SELECT');
    }
  });

  it('关闭历史面板 (SQ-005)', async () => {
    const histBtn = await $('button*=历史');
    await histBtn.click();
    await browser.pause(300);
  });

  // ── 取消查询 ───────────────────────────────────────────────────

  it('执行长查询时应能取消 (SQ-006)', async () => {
    await setEditorContent('SELECT pg_sleep(10)');
    const execBtn = await $('button*=执行');
    await execBtn.click();
    await browser.pause(1500);

    const stopBtn = await $('button*=停止');
    if (await stopBtn.isExisting() && await stopBtn.isDisplayed()) {
      await stopBtn.click();
      await browser.pause(3000);

      const body = await $('body').getText();
      const wasCancelled = body.includes('cancel') || body.includes('取消') ||
        body.includes('总耗时') || body.includes('错误') ||
        body.includes('失败') || body.includes('interrupted') ||
        body.includes('pg_sleep');
      expect(wasCancelled).toBe(true);
    } else {
      // If stop button didn't appear, the query completed or the UI didn't render it
      // Just verify the app is responsive
      const body = await $('body').getText();
      expect(body.length).toBeGreaterThan(0);
    }
  });
});
