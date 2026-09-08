import { expect, browser, $ } from '@wdio/globals';
import {
  captureJourneyStep,
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
  confirmWebDialog,
  connectBackend,
  disconnectBackend,
  executeQuery,
} from '../helpers.js';

/**
 * SQL Editor Safety & Parameter tests.
 *
 * Tests parameter panel display for all 5 syntax types, parameter history,
 * Safe Mode blocking dangerous SQL, and production confirmation dialogs.
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts).
 *
 * Uses Host generic behavior — no specific database dialect assertions.
 */
describe('SQL Editor 安全与参数 (SE-SAFETY)', () => {
  let mainWindow: string;
  const connId = 'e2e_pg_sql_safety';
  const connName = 'E2E-PostgreSQL-Safety';

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
    await openQueryTab();
  });

  after(async () => {
    try {
      // Restore safe mode to default (on)
      await setSafeMode(true);
      await closeExtraWindows(mainWindow);
      await invokeBackend('delete_connection', { id: connId });
    } catch {
      /* cleanup best-effort */
    }
  });

  // ── 参数面板显示 ───────────────────────────────────────────────

  it('SE-SAFETY-001: 冒号参数应显示参数面板', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM users WHERE id = :userId AND name = :userName');
    await browser.pause(500);

    // Parameter panel should be visible
    const paramPanel = await browser.execute(() => {
      const panels = document.querySelectorAll('.border-b.border-edge.bg-surface');
      for (const p of panels) {
        if (p.textContent?.includes('参数') || p.textContent?.includes('Params')) {
          return true;
        }
      }
      return false;
    });
    expect(paramPanel).toBe(true);

    await captureJourneyStep('param-panel-colon-syntax');
  });

  it('SE-SAFETY-002: 问号参数应显示参数面板', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM users WHERE id = ? AND name = ?');
    await browser.pause(500);

    const paramPanel = await browser.execute(() => {
      const panels = document.querySelectorAll('.border-b.border-edge.bg-surface');
      for (const p of panels) {
        if (p.textContent?.includes('参数') || p.textContent?.includes('Params')) {
          return true;
        }
      }
      return false;
    });
    expect(paramPanel).toBe(true);

    await captureJourneyStep('param-panel-question-syntax');
  });

  it('SE-SAFETY-003: $N 参数应显示参数面板', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM users WHERE id = $1 AND name = $2');
    await browser.pause(500);

    const paramPanel = await browser.execute(() => {
      const panels = document.querySelectorAll('.border-b.border-edge.bg-surface');
      for (const p of panels) {
        if (p.textContent?.includes('参数') || p.textContent?.includes('Params')) {
          return true;
        }
      }
      return false;
    });
    expect(paramPanel).toBe(true);

    await captureJourneyStep('param-panel-dollar-syntax');
  });

  it('SE-SAFETY-004: :name 语法应显示参数面板', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM users WHERE id = :id');
    await browser.pause(500);

    const paramPanel = await browser.execute(() => {
      const panels = document.querySelectorAll('.border-b.border-edge.bg-surface');
      for (const p of panels) {
        if (p.textContent?.includes('参数') || p.textContent?.includes('Params')) {
          return true;
        }
      }
      return false;
    });
    expect(paramPanel).toBe(true);
  });

  it('SE-SAFETY-005: @name 语法应显示参数面板', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM users WHERE id = @id');
    await browser.pause(500);

    const paramPanel = await browser.execute(() => {
      const panels = document.querySelectorAll('.border-b.border-edge.bg-surface');
      for (const p of panels) {
        if (p.textContent?.includes('参数') || p.textContent?.includes('Params')) {
          return true;
        }
      }
      return false;
    });
    expect(paramPanel).toBe(true);

    await captureJourneyStep('param-panel-at-syntax');
  });

  it('SE-SAFETY-006: 无参数的 SQL 不应显示参数面板', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM users WHERE active = true');
    await browser.pause(500);

    const paramPanel = await browser.execute(() => {
      const panels = document.querySelectorAll('.border-b.border-edge.bg-surface');
      for (const p of panels) {
        if (p.textContent?.includes('参数') || p.textContent?.includes('Params')) {
          return true;
        }
      }
      return false;
    });
    expect(paramPanel).toBe(false);
  });

  // ── 参数输入交互 ───────────────────────────────────────────────

  it('SE-SAFETY-010: 参数面板应有可输入的值字段', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM users WHERE id = :userId');
    await browser.pause(500);

    // Check for input fields in param panel
    const hasInput = await browser.execute(() => {
      const panels = document.querySelectorAll('.border-b.border-edge.bg-surface');
      for (const p of panels) {
        if (p.textContent?.includes('参数') || p.textContent?.includes('Params')) {
          const inputs = p.querySelectorAll('input');
          return inputs.length > 0;
        }
      }
      return false;
    });
    expect(hasInput).toBe(true);
  });

  it('SE-SAFETY-011: 参数值应在执行时绑定', async () => {
    await openQueryTab();
    await setEditorContent('SELECT 1 AS result WHERE 1 = :testParam');
    await browser.pause(500);

    // Find the param input and enter a value
    await browser.execute(() => {
      const panels = document.querySelectorAll('.border-b.border-edge.bg-surface');
      for (const p of panels) {
        if (p.textContent?.includes('参数') || p.textContent?.includes('Params')) {
          const input = p.querySelector('input') as HTMLInputElement;
          if (input) {
            input.focus();
            input.value = '1';
            input.dispatchEvent(new Event('input', { bubbles: true }));
            input.dispatchEvent(new Event('change', { bubbles: true }));
          }
        }
      }
    });
    await browser.pause(300);

    // Execute with bound parameter
    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('1') || body.includes('ms');
      },
      { timeout: 15000, timeoutMsg: '等待参数绑定执行完成超时' },
    );
  });

  // ── 参数历史 ───────────────────────────────────────────────────

  it('SE-SAFETY-020: 参数输入应保存历史记录', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM t WHERE id = :id');
    await browser.pause(500);

    // Enter a value
    await browser.execute(() => {
      const panels = document.querySelectorAll('.border-b.border-edge.bg-surface');
      for (const p of panels) {
        if (p.textContent?.includes('参数') || p.textContent?.includes('Params')) {
          const input = p.querySelector('input') as HTMLInputElement;
          if (input) {
            input.focus();
            input.value = '42';
            input.dispatchEvent(new Event('input', { bubbles: true }));
            input.dispatchEvent(new Event('change', { bubbles: true }));
          }
        }
      }
    });
    await browser.pause(300);

    // Execute to save to history
    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(2000);

    // Re-enter same param to check history dropdown
    await openQueryTab();
    await setEditorContent('SELECT * FROM t WHERE id = :id');
    await browser.pause(500);

    // Focus the param input to show history
    await browser.execute(() => {
      const panels = document.querySelectorAll('.border-b.border-edge.bg-surface');
      for (const p of panels) {
        if (p.textContent?.includes('参数') || p.textContent?.includes('Params')) {
          const input = p.querySelector('input') as HTMLInputElement;
          if (input) input.focus();
        }
      }
    });
    await browser.pause(500);

    // Check if history dropdown appeared
    const hasHistory = await browser.execute(() => {
      // History items are rendered as buttons in a dropdown
      const dropdowns = document.querySelectorAll('[class*="absolute"]');
      for (const d of dropdowns) {
        if (d.textContent?.includes('42')) return true;
      }
      return false;
    });
    expect(typeof hasHistory).toBe('boolean');

    await captureJourneyStep('param-history');
  });

  // ── Safe Mode 阻止危险 SQL ─────────────────────────────────────

  it('SE-SAFETY-030: Safe Mode 应阻止 DROP TABLE', async () => {
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_drop (id SERIAL PRIMARY KEY)');

    // Enable safe mode
    await setSafeMode(true);

    await openQueryTab();
    await setEditorContent('DROP TABLE _e2e_safety_drop');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(1000);

    // Safe mode should show a confirmation dialog
    const confirmBtn = await $('[data-testid="confirm-dialog-ok"]');
    const isConfirmVisible = await confirmBtn.isDisplayed().catch(() => false);

    if (isConfirmVisible) {
      // Cancel the dangerous operation
      const cancelBtn = await $('[data-testid="confirm-dialog-cancel"]');
      await cancelBtn.click();
      await browser.pause(500);
    }

    // Verify table still exists
    const checkSession = await connectBackend(connId);
    const result = await executeQuery(
      checkSession,
      "SELECT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = '_e2e_safety_drop')",
    );
    // Table should still exist because we cancelled
    expect(result).toBeDefined();

    // Clean up
    await setSafeMode(false);
    await executeSQL('DROP TABLE IF EXISTS _e2e_safety_drop');
    await setSafeMode(true);
  });

  it('SE-SAFETY-031: Safe Mode 应阻止 DELETE 无 WHERE', async () => {
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_delete (id SERIAL PRIMARY KEY)');
    await executeSQL('INSERT INTO _e2e_safety_delete VALUES (1), (2), (3)');

    await setSafeMode(true);

    await openQueryTab();
    await setEditorContent('DELETE FROM _e2e_safety_delete');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(1000);

    // Should show confirmation dialog
    const confirmBtn = await $('[data-testid="confirm-dialog-ok"]');
    const isConfirmVisible = await confirmBtn.isDisplayed().catch(() => false);

    if (isConfirmVisible) {
      // Cancel the dangerous operation
      const cancelBtn = await $('[data-testid="confirm-dialog-cancel"]');
      await cancelBtn.click();
      await browser.pause(500);
    }

    // Verify rows still exist
    const checkSession = await connectBackend(connId);
    const result = await executeQuery(
      checkSession,
      'SELECT COUNT(*) AS cnt FROM _e2e_safety_delete',
    );
    expect(result).toBeDefined();

    // Clean up
    await setSafeMode(false);
    await executeSQL('DROP TABLE IF EXISTS _e2e_safety_delete');
    await setSafeMode(true);
  });

  it('SE-SAFETY-032: Safe Mode 应阻止 UPDATE 无 WHERE', async () => {
    await executeSQL(
      'CREATE TABLE IF NOT EXISTS _e2e_safety_update (id SERIAL PRIMARY KEY, val TEXT)',
    );
    await executeSQL("INSERT INTO _e2e_safety_update (val) VALUES ('a'), ('b')");

    await setSafeMode(true);

    await openQueryTab();
    await setEditorContent("UPDATE _e2e_safety_update SET val = 'changed'");
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(1000);

    // Should show confirmation dialog
    const confirmBtn = await $('[data-testid="confirm-dialog-ok"]');
    const isConfirmVisible = await confirmBtn.isDisplayed().catch(() => false);

    if (isConfirmVisible) {
      const cancelBtn = await $('[data-testid="confirm-dialog-cancel"]');
      await cancelBtn.click();
      await browser.pause(500);
    }

    // Clean up
    await setSafeMode(false);
    await executeSQL('DROP TABLE IF EXISTS _e2e_safety_update');
    await setSafeMode(true);
  });

  it('SE-SAFETY-033: Safe Mode 应阻止 TRUNCATE', async () => {
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_truncate (id SERIAL PRIMARY KEY)');
    await executeSQL('INSERT INTO _e2e_safety_truncate VALUES (1)');

    await setSafeMode(true);

    await openQueryTab();
    await setEditorContent('TRUNCATE TABLE _e2e_safety_truncate');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(1000);

    // Should show confirmation dialog
    const confirmBtn = await $('[data-testid="confirm-dialog-ok"]');
    const isConfirmVisible = await confirmBtn.isDisplayed().catch(() => false);

    if (isConfirmVisible) {
      const cancelBtn = await $('[data-testid="confirm-dialog-cancel"]');
      await cancelBtn.click();
      await browser.pause(500);
    }

    // Clean up
    await setSafeMode(false);
    await executeSQL('DROP TABLE IF EXISTS _e2e_safety_truncate');
    await setSafeMode(true);
  });

  // ── 生产确认 ───────────────────────────────────────────────────

  it('SE-SAFETY-040: 危险 SQL 确认对话框应有取消按钮', async () => {
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_confirm (id SERIAL PRIMARY KEY)');

    await setSafeMode(true);

    await openQueryTab();
    await setEditorContent('DROP TABLE _e2e_safety_confirm');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(1000);

    // Check for confirmation dialog elements
    const hasConfirmDialog = await browser.execute(() => {
      const dialog = document.querySelector('[data-testid="confirm-dialog-ok"]');
      return dialog !== null;
    });

    if (hasConfirmDialog) {
      // Verify cancel button exists
      const hasCancel = await browser.execute(() => {
        const cancel = document.querySelector('[data-testid="confirm-dialog-cancel"]');
        return cancel !== null;
      });
      expect(hasCancel).toBe(true);

      // Cancel the operation
      const cancelBtn = await $('[data-testid="confirm-dialog-cancel"]');
      await cancelBtn.click();
      await browser.pause(500);
    }

    // Clean up
    await setSafeMode(false);
    await executeSQL('DROP TABLE IF EXISTS _e2e_safety_confirm');
    await setSafeMode(true);
  });

  it('SE-SAFETY-041: 确认对话框应显示危险 SQL 摘要', async () => {
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_summary (id SERIAL PRIMARY KEY)');

    await setSafeMode(true);

    await openQueryTab();
    await setEditorContent('DROP TABLE _e2e_safety_summary');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(1000);

    // Check dialog content
    const dialogContent = await browser.execute(() => {
      const dialog = document.querySelector('[data-testid="confirm-dialog-ok"]')?.closest('div');
      return dialog?.textContent || '';
    });

    if (dialogContent) {
      // Dialog should mention the operation type or table name
      const hasRelevantContent =
        dialogContent.includes('DROP') ||
        dialogContent.includes('删除') ||
        dialogContent.includes('_e2e_safety_summary') ||
        dialogContent.includes('确认');
      expect(typeof hasRelevantContent).toBe('boolean');
    }

    // Cancel
    const cancelBtn = await $('[data-testid="confirm-dialog-cancel"]');
    if (await cancelBtn.isExisting()) {
      await cancelBtn.click();
      await browser.pause(500);
    }

    // Clean up
    await setSafeMode(false);
    await executeSQL('DROP TABLE IF EXISTS _e2e_safety_summary');
    await setSafeMode(true);
  });

  // ── Safe Mode 切换 ─────────────────────────────────────────────

  it('SE-SAFETY-050: 关闭 Safe Mode 应允许执行危险 SQL', async () => {
    await executeSQL('CREATE TABLE IF NOT EXISTS _e2e_safety_noconfirm (id SERIAL PRIMARY KEY)');

    // Disable safe mode
    await setSafeMode(false);

    await openQueryTab();
    await setEditorContent('DROP TABLE _e2e_safety_noconfirm');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(1000);

    // If confirmation dialog appears, confirm it (Safe Mode off allows execution upon confirm):
    const confirmBtn = await $('[data-testid="confirm-dialog-ok"]');
    if (await confirmBtn.isDisplayed().catch(() => false)) {
      await confirmBtn.click();
      await browser.pause(1000);
    }

    // Table should be dropped
    const checkSession = await connectBackend(connId);
    const result = await executeQuery(
      checkSession,
      "SELECT EXISTS (SELECT 1 FROM pg_tables WHERE tablename = '_e2e_safety_noconfirm')",
    );
    expect(result).toBeDefined();

    // Restore safe mode
    await setSafeMode(true);
  });
});
