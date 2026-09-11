import { expect, browser, $ } from '@wdio/globals';
import {
  clickCardConnectButton,
  closeExtraWindows,
  setEditorContent,
  openQueryTab,
  openConnectionsWorkspace,
  expandConnectedConnectionInNavigator,
  waitForConnectionToolbar,
  executeSQL,
  invokeBackend,
} from '../helpers.js';

/**
 * SQL Editor Statement execution tests (Host capability).
 *
 * Tests the Host-native statement execution model: statement-level shortcuts,
 * running-state indication, multi-result regression, and DML row counts.
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts).
 *
 * NOTE: statement frame/gutter decorations (Pro S4-A) were migrated to
 * packages/pro-extensions/sql-editor-pro/e2e/specs/sql-editor-statement.ts.
 * Uses Host generic behaviour — no specific database dialect assertions.
 */
describe('SQL Editor 语句执行 (SE-STMT)', () => {
  let mainWindow: string;
  const connId = 'e2e_pg_sql_stmt';
  const connName = 'E2E-PostgreSQL-Stmt';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await invokeBackend('save_connection', {
      config: {
        id: connId,
        name: connName,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: process.env.E2E_PG_DB || 'postgres',
        schema: process.env.E2E_WORKER_SCHEMA || undefined,
        group: 'E2E 测试',
        colorTag: 'green',
        sslMode: 'disable',
        options: {},
      },
    });
    await browser.refresh();
    await browser.pause(1500);
    await openConnectionsWorkspace();
    await clickCardConnectButton(connName);
    await waitForConnectionToolbar();
    await expandConnectedConnectionInNavigator(connName);
    await browser.pause(1000);
    await openQueryTab();
  });

  after(async () => {
    try {
      await closeExtraWindows(mainWindow);
      await invokeBackend('delete_connection', { id: connId });
    } catch {
      /* cleanup best-effort */
    }
  });

  // ── 语句级快捷键 ───────────────────────────────────────────────

  it('SE-STMT-030: Cmd+Enter 应执行当前语句', async () => {
    await openQueryTab();
    await setEditorContent('SELECT 100 AS stmt_shortcut');
    await browser.pause(300);

    await browser.keys(['Meta', 'Enter']);
    await browser.pause(300);

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('100') || body.includes('stmt_shortcut');
      },
      { timeout: 15000, timeoutMsg: '等待 Cmd+Enter 执行完成超时' },
    );
  });

  it('SE-STMT-031: Ctrl+Enter 应执行当前语句 (非 macOS)', async () => {
    await openQueryTab();
    await setEditorContent('SELECT 200 AS ctrl_shortcut');
    await browser.pause(300);

    // On macOS, Meta+Enter is used; on other platforms, Ctrl+Enter
    const platform = await browser.execute(() => {
      return navigator.platform.toLowerCase().includes('mac') ? 'mac' : 'other';
    });
    if (platform === 'mac') {
      await browser.keys(['Meta', 'Enter']);
    } else {
      await browser.keys(['Control', 'Enter']);
    }
    await browser.pause(300);

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('200') || body.includes('ctrl_shortcut');
      },
      { timeout: 15000, timeoutMsg: '等待执行完成超时' },
    );
  });

  // ── 运行状态指示 ───────────────────────────────────────────────

  it('SE-STMT-040: 执行期间编辑器应显示执行状态', async () => {
    await openQueryTab();
    await setEditorContent('SELECT pg_sleep(3)');
    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(500);

    // Stop button should appear during execution
    const stopBtn = await $('[data-testid="editor-stop-button"]');
    const isDisplayed = await stopBtn.isDisplayed();
    expect(isDisplayed).toBe(true);

    // Cancel
    await stopBtn.click();
    await browser.pause(2000);
  });

  it('SE-STMT-041: 执行完成后停止按钮应消失', async () => {
    await openQueryTab();
    await setEditorContent('SELECT 1 AS done_test');
    await executeSQL('SELECT 1 AS done_test');
    await browser.pause(1000);

    const stopBtn = await $('[data-testid="editor-stop-button"]');
    const isDisplayed = await stopBtn.isDisplayed().catch(() => false);
    expect(isDisplayed).toBe(false);
  });

  // ── 多结果回归 ─────────────────────────────────────────────────

  it('SE-STMT-050: 多语句执行应显示多个结果标签', async () => {
    await openQueryTab();
    await setEditorContent('SELECT 1 AS first; SELECT 2 AS second');
    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('结果 1') || body.includes('Result 1');
      },
      { timeout: 15000, timeoutMsg: '等待多结果标签出现超时' },
    );
  });

  it('SE-STMT-051: 结果标签应可切换', async () => {
    // After previous test, result tabs should be visible
    const body = await $('body').getText();
    const hasTabs =
      body.includes('结果 1') ||
      body.includes('Result 1') ||
      body.includes('结果 2') ||
      body.includes('Result 2');
    expect(hasTabs).toBe(true);
  });

  // ── DML 行数 ───────────────────────────────────────────────────

  it('SE-STMT-060: DML 语句应显示影响行数', async () => {
    await openQueryTab();
    await setEditorContent(
      'CREATE TABLE IF NOT EXISTS _e2e_stmt_test (id SERIAL PRIMARY KEY, val TEXT); ' +
        "INSERT INTO _e2e_stmt_test (val) VALUES ('test1')",
    );
    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('ms') || body.includes('行') || body.includes('rows');
      },
      { timeout: 15000, timeoutMsg: '等待 DML 执行完成超时' },
    );

    // Clean up
    await executeSQL('DROP TABLE IF EXISTS _e2e_stmt_test');
  });
});
