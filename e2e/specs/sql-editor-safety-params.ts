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
  setSafeMode,
  setConfirmDangerousExecution,
  dismissAnyOpenDialog,
  dismissConfirmDialogIfOpen,
  dismissResultMessageIfOpen,
} from '../helpers.js';

/**
 * SQL Editor 安全模式（Safe Mode）与高危 SQL 确认 — Host capability.
 *
 * 三类用例：
 *  1. Safe Mode 开启 → 高危操作被硬拦截：只弹"已阻止"提示（ResultMessageDialog，仅 OK 关闭），
 *     不弹"确定/取消"确认框，操作绝不执行（表/行保持不变）。
 *  2. Safe Mode 关闭 + 设置 `confirmDangerousExecution`（默认开）→ 弹"确定/取消"确认框：
 *     · 取消 → 不执行（表保留）；· 确定 → 执行（表被删）。
 *  3. Safe Mode 关闭 + `confirmDangerousExecution=false` → 不弹任何框，直接放行执行。
 *
 * 依赖一个 PostgreSQL 连接（wdio.conf.ts 提供）。所有 DB 校验都在编辑器会话内完成（读结果网格
 * `data-table-cell`），避免跨会话 schema/库不一致造成的假阴性。
 *
 * 注：Safe Mode 拦截提示（ResultMessageDialog）与高危确认框（ConfirmDialog）都是模态，会挡住
 * 查询工具栏，因此每个用例在触碰下一个 openQueryTab 前都先可靠关闭它们。
 */
describe('SQL Editor 安全模式与高危确认 (SE-SAFETY)', () => {
  let mainWindow: string;
  const connId = 'e2e_pg_sql_safety';
  const connName = 'E2E-PostgreSQL-Safety';

  async function clickExec(): Promise<void> {
    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
  }

  /** True iff the dangerous-SQL ConfirmDialog (确定/取消) becomes visible within `scanMs`. */
  async function confirmDialogSeen(scanMs: number, pollMs = 100): Promise<boolean> {
    const ok = await $('[data-testid="confirm-dialog-ok"]');
    const start = Date.now();
    while (Date.now() - start < scanMs) {
      if (await ok.isDisplayed().catch(() => false)) return true;
      await browser.pause(pollMs);
    }
    return false;
  }

  /** Run `sql` in the editor and return the text of the first result-grid data cell (null if it errored). */
  async function editorFirstCell(sql: string): Promise<string | null> {
    await dismissAnyOpenDialog();
    await openQueryTab();
    await setEditorContent(sql);
    await browser.pause(300);
    await clickExec();
    const MISSING = /does not exist|doesn't exist|could not find|不存在|未找到/i;
    let bodyText = '';
    await browser.waitUntil(
      async () => {
        const cell = await $('[data-testid="data-table-cell"]');
        if (await cell.isExisting()) return true;
        bodyText = await $('body').getText();
        return MISSING.test(bodyText) || (await isErrorDialogOpen());
      },
      { timeout: 15000, timeoutMsg: '等待查询结果/错误超时' },
    );
    const cell = await $('[data-testid="data-table-cell"]');
    if (await cell.isExisting()) {
      const text = (await cell.getText()).trim();
      await dismissAnyOpenDialog();
      return text;
    }
    // No result cell → the query failed (e.g. relation does not exist) → table not queryable.
    await dismissAnyOpenDialog();
    return null;
  }

  async function isErrorDialogOpen(): Promise<boolean> {
    const msgOk = await $('[data-testid="result-message-ok"]');
    return (await msgOk.isExisting()) && (await msgOk.isDisplayed().catch(() => false));
  }

  /** Poll for a ResultMessageDialog (Safe Mode "blocked" notice) to appear within `scanMs`. */
  async function noticeDialogSeen(scanMs: number, pollMs = 100): Promise<boolean> {
    const msgOk = await $('[data-testid="result-message-ok"]');
    const start = Date.now();
    while (Date.now() - start < scanMs) {
      if ((await msgOk.isExisting()) && (await msgOk.isDisplayed().catch(() => false))) return true;
      await browser.pause(pollMs);
    }
    return false;
  }

  /** Run a column count in the editor; returns the number if the table exists, null otherwise. */
  async function tableLive(name: string): Promise<boolean> {
    const cell = await editorFirstCell(`SELECT COUNT(*) FROM ${name}`);
    if (cell == null) return false;
    const n = Number(cell.replace(/[^\d-]/g, ''));
    return Number.isFinite(n);
  }

  async function setSafeModeAndConfirm(safe: boolean, confirm: boolean): Promise<void> {
    await setSafeMode(safe);
    await setConfirmDangerousExecution(confirm);
  }

  /** Drop a leftover table: Safe Mode blocks DROP through sql_guard, so disable it first. */
  async function dropTable(name: string): Promise<void> {
    await setSafeMode(false);
    await setConfirmDangerousExecution(true);
    await executeSQL(`DROP TABLE IF EXISTS ${name}`);
    await dismissAnyOpenDialog();
  }

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
        colorTag: 'red',
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
    await setSafeModeAndConfirm(true, true);
    await openQueryTab();
    await dismissAnyOpenDialog();
  });

  after(async () => {
    try {
      await setSafeModeAndConfirm(true, true);
      await dismissAnyOpenDialog();
      await closeExtraWindows(mainWindow);
      await invokeBackend('delete_connection', { id: connId });
    } catch {
      /* cleanup best-effort */
    }
  });

  // ──────────────────────────────────────────────────────────────
  // 第 1 类：Safe Mode 开启 → 硬拦截，仅提示，绝不执行
  // ──────────────────────────────────────────────────────────────

  it('SE-SAFETY-030: Safe Mode 开启拦截 DROP TABLE（仅提示、绝不执行）', async () => {
    await setSafeModeAndConfirm(true, true);
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_drop (id SERIAL PRIMARY KEY)');
    await dismissAnyOpenDialog();

    await openQueryTab();
    await setEditorContent('DROP TABLE _e2e_safety_drop');
    await browser.pause(300);
    await clickExec();

    // 硬拦截：不出现"确定/取消"确认框
    expect(await confirmDialogSeen(1500)).toBe(false);
    // 但出现"已阻止"提示（ResultMessageDialog）→ 关掉它
    expect(await noticeDialogSeen(4000)).toBe(true);
    await dismissResultMessageIfOpen(3000);
    // 操作绝不执行：表仍在
    expect(await tableLive('_e2e_safety_drop')).toBe(true);

    await dropTable('_e2e_safety_drop');
  });

  it('SE-SAFETY-031: Safe Mode 开启拦截 DELETE 无 WHERE（仅提示、绝不执行）', async () => {
    await setSafeModeAndConfirm(true, true);
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_delete (id SERIAL PRIMARY KEY)');
    await executeSQL('INSERT INTO _e2e_safety_delete VALUES (1), (2), (3)');
    await dismissAnyOpenDialog();

    await openQueryTab();
    await setEditorContent('DELETE FROM _e2e_safety_delete');
    await browser.pause(300);
    await clickExec();

    expect(await confirmDialogSeen(1500)).toBe(false);
    expect(await noticeDialogSeen(4000)).toBe(true);
    await dismissResultMessageIfOpen(3000);
    // 行应原封不动（仍 3 行）
    const cell = await editorFirstCell('SELECT COUNT(*) FROM _e2e_safety_delete');
    expect(Number((cell ?? '').replace(/[^\d-]/g, ''))).toBe(3);

    await dropTable('_e2e_safety_delete');
  });

  it('SE-SAFETY-032: Safe Mode 开启拦截 UPDATE 无 WHERE（仅提示、绝不执行）', async () => {
    await setSafeModeAndConfirm(true, true);
    await executeSQL(
      'CREATE TABLE IF NOT EXISTS _e2e_safety_update (id SERIAL PRIMARY KEY, val TEXT)',
    );
    await executeSQL("INSERT INTO _e2e_safety_update (val) VALUES ('a'), ('b')");
    await dismissAnyOpenDialog();

    await openQueryTab();
    await setEditorContent("UPDATE _e2e_safety_update SET val = 'changed'");
    await browser.pause(300);
    await clickExec();

    expect(await confirmDialogSeen(1500)).toBe(false);
    expect(await noticeDialogSeen(4000)).toBe(true);
    await dismissResultMessageIfOpen(3000);
    // 值未被改写
    const cell = await editorFirstCell('SELECT val FROM _e2e_safety_update ORDER BY id LIMIT 1');
    expect(cell).toBe('a');

    await dropTable('_e2e_safety_update');
  });

  it('SE-SAFETY-033: Safe Mode 开启拦截 TRUNCATE（仅提示、绝不执行）', async () => {
    await setSafeModeAndConfirm(true, true);
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_truncate (id SERIAL PRIMARY KEY)');
    await executeSQL('INSERT INTO _e2e_safety_truncate VALUES (1)');
    await dismissAnyOpenDialog();

    await openQueryTab();
    await setEditorContent('TRUNCATE TABLE _e2e_safety_truncate');
    await browser.pause(300);
    await clickExec();

    expect(await confirmDialogSeen(1500)).toBe(false);
    expect(await noticeDialogSeen(4000)).toBe(true);
    await dismissResultMessageIfOpen(3000);
    // 行仍是 1，未被清空
    const cell = await editorFirstCell('SELECT COUNT(*) FROM _e2e_safety_truncate');
    expect(Number((cell ?? '').replace(/[^\d-]/g, ''))).toBe(1);

    await dropTable('_e2e_safety_truncate');
  });

  // ──────────────────────────────────────────────────────────────
  // 第 2 类：Safe Mode 关闭 → 是否弹"确定/取消"由 confirmDangerousExecution 控制
  // ──────────────────────────────────────────────────────────────

  it('SE-SAFETY-040: Safe Mode 关闭 + 确认开启 → 弹框后点取消则不执行', async () => {
    await setSafeModeAndConfirm(false, true);
    await executeSQL(
      'CREATE TABLE IF NOT EXISTS _e2e_safety_confirm_cancel (id SERIAL PRIMARY KEY)',
    );
    await dismissAnyOpenDialog();

    await openQueryTab();
    await setEditorContent('DROP TABLE _e2e_safety_confirm_cancel');
    await browser.pause(300);
    await clickExec();

    // 应弹"确定/取消"确认框
    expect(await confirmDialogSeen(4000)).toBe(true);
    // 点取消 → 不执行
    const cancelBtn = await $('[data-testid="confirm-dialog-cancel"]');
    await cancelBtn.waitForDisplayed({ timeout: 2000 });
    await cancelBtn.click();
    await browser.pause(400);
    await dismissAnyOpenDialog();

    expect(await tableLive('_e2e_safety_confirm_cancel')).toBe(true);

    await dropTable('_e2e_safety_confirm_cancel');
  });

  it('SE-SAFETY-041: Safe Mode 关闭 + 确认开启 → 弹框后点确定则执行', async () => {
    await setSafeModeAndConfirm(false, true);
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_confirm_ok (id SERIAL PRIMARY KEY)');
    await dismissAnyOpenDialog();

    await openQueryTab();
    await setEditorContent('DROP TABLE _e2e_safety_confirm_ok');
    await browser.pause(300);
    await clickExec();

    expect(await confirmDialogSeen(4000)).toBe(true);
    const okBtn = await $('[data-testid="confirm-dialog-ok"]');
    await okBtn.waitForDisplayed({ timeout: 2000 });
    await okBtn.click();
    await browser.pause(600);
    await dismissAnyOpenDialog();

    // 执行成功：表已被删除
    expect(await tableLive('_e2e_safety_confirm_ok')).toBe(false);
  });

  it('SE-SAFETY-050: Safe Mode 关闭 + 确认关闭 → 不弹框、直接放行执行', async () => {
    await setSafeModeAndConfirm(false, false);
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_direct (id SERIAL PRIMARY KEY)');
    await dismissAnyOpenDialog();

    await openQueryTab();
    await setEditorContent('DROP TABLE _e2e_safety_direct');
    await browser.pause(300);
    await clickExec();

    // 直接放行：不弹"确定/取消"确认框
    expect(await confirmDialogSeen(1500)).toBe(false);
    // 立即执行：表已不存在
    expect(await tableLive('_e2e_safety_direct')).toBe(false);
  });
});
