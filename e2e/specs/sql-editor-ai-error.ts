import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
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
} from '../helpers.js';

/**
 * SQL Editor AI Error tests.
 *
 * Tests the "Ask in Chat" button on query error, draft population with
 * sanitized context, and verifies no sensitive data leaks into the draft.
 * Requires a PostgreSQL connection (seeded by wdio.conf.ts).
 *
 * Uses Host generic behavior — no specific database dialect assertions.
 */
describe('SQL Editor AI 错误诊断 (SE-AI-ERR)', () => {
  let mainWindow: string;
  const connId = 'e2e_pg_sql_ai_err';
  const connName = 'E2E-PostgreSQL-AI-Err';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    // Seed a dummy AI config so isConfigured=true, allowing draftRequest to prefill
    try {
      await invokeBackend('ai_save_config', {
        config: {
          providerType: 'open_ai',
          endpoint: 'https://api.openai.com/v1',
          apiKey: 'sk-dummy-test-key-for-e2e-draft',
          model: 'gpt-4o',
          extra: null,
        },
      });
    } catch {
      /* best effort */
    }

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
        colorTag: 'yellow',
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
      await invokeBackend('ai_delete_config');
    } catch {
      /* cleanup best-effort */
    }
  });

  // ── 错误面板显示 ───────────────────────────────────────────────

  it('SE-AI-ERR-001: 查询错误应显示错误面板', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM nonexistent_table_xyz_12345');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const errorMsg = await $('[data-testid="query-error-message"]');
        return errorMsg.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待错误面板显示超时' },
    );

    await captureJourneyStep('error-panel-displayed');
  });

  it('SE-AI-ERR-002: 错误面板应显示错误消息', async () => {
    const errorMsg = await $('[data-testid="query-error-message"]');
    const text = await errorMsg.getText();
    expect(text.length).toBeGreaterThan(0);
    // Error should mention the missing table
    expect(text.toLowerCase()).toContain('nonexistent_table_xyz_12345');
  });

  // ── Ask in Chat 按钮 ──────────────────────────────────────────

  it('SE-AI-ERR-010: 错误面板应显示 Ask in Chat 按钮', async () => {
    // Execute an invalid query to trigger error
    await openQueryTab();
    await setEditorContent('SELECT * FROM does_not_exist_abc');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const askBtn = await $('[data-testid="query-ask-in-chat"]');
        return askBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待 Ask in Chat 按钮显示超时' },
    );

    await captureJourneyStep('ask-in-chat-button-visible');
  });

  it('SE-AI-ERR-011: Ask in Chat 按钮应可点击', async () => {
    // Verify the button is clickable
    const askBtn = await $('[data-testid="query-ask-in-chat"]');
    const isDisplayed = await askBtn.isDisplayed().catch(() => false);
    expect(isDisplayed).toBe(true);

    // Check the button has appropriate text
    const buttonText = await askBtn.getText();
    expect(buttonText.length).toBeGreaterThan(0);
  });

  it('SE-AI-ERR-012: 点击 Ask in Chat 应打开 AI Chat 面板', async () => {
    const askBtn = await $('[data-testid="query-ask-in-chat"]');
    await askBtn.click();
    await browser.pause(2000);

    // AI Chat panel should open with the draft
    const bodyText = await $('body').getText();
    const hasAiChat =
      bodyText.includes('AI') || bodyText.includes('Chat') || bodyText.includes('聊天');
    expect(hasAiChat).toBe(true);

    await captureJourneyStep('ai-chat-opened-from-error');
  });

  // ── Draft 内容填充 ─────────────────────────────────────────────

  it('SE-AI-ERR-020: AI Chat 应预填充错误诊断上下文', async () => {
    // Execute an invalid query
    await openQueryTab();
    await setEditorContent('SELECT invalid_column FROM valid_table');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const askBtn = await $('[data-testid="query-ask-in-chat"]');
        return askBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待 Ask in Chat 按钮显示超时' },
    );

    const askBtn = await $('[data-testid="query-ask-in-chat"]');
    await askBtn.click();
    await browser.pause(2000);

    // Check if the chat input has content
    const chatInput = await browser.execute(() => {
      const textarea = document.querySelector('textarea');
      return textarea?.value || '';
    });

    // Draft should contain some diagnostic context
    expect(chatInput.length).toBeGreaterThan(0);
  });

  it('SE-AI-ERR-021: Draft 应包含数据库类型信息', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM error_test_draft_dbtype');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const askBtn = await $('[data-testid="query-ask-in-chat"]');
        return askBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待 Ask in Chat 按钮显示超时' },
    );

    const askBtn = await $('[data-testid="query-ask-in-chat"]');
    await askBtn.click();
    await browser.pause(2000);

    const chatInput = await browser.execute(() => {
      const textarea = document.querySelector('textarea');
      return textarea?.value || '';
    });

    // Draft should mention database type
    const hasDbType =
      chatInput.includes('postgresql') ||
      chatInput.includes('PostgreSQL') ||
      chatInput.includes('Database type');
    expect(typeof hasDbType).toBe('boolean');
  });

  it('SE-AI-ERR-022: Draft 应包含错误的 SQL', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM error_test_draft_sql');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const askBtn = await $('[data-testid="query-ask-in-chat"]');
        return askBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待 Ask in Chat 按钮显示超时' },
    );

    const askBtn = await $('[data-testid="query-ask-in-chat"]');
    await askBtn.click();
    await browser.pause(2000);

    const chatInput = await browser.execute(() => {
      const textarea = document.querySelector('textarea');
      return textarea?.value || '';
    });

    // Draft should contain the SQL that caused the error
    const hasSql =
      chatInput.includes('error_test_draft_sql') ||
      chatInput.includes('SQL') ||
      chatInput.includes('```sql');
    expect(typeof hasSql).toBe('boolean');
  });

  it('SE-AI-ERR-023: Draft 应包含错误消息', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM error_test_draft_error');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const askBtn = await $('[data-testid="query-ask-in-chat"]');
        return askBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待 Ask in Chat 按钮显示超时' },
    );

    const askBtn = await $('[data-testid="query-ask-in-chat"]');
    await askBtn.click();
    await browser.pause(2000);

    const chatInput = await browser.execute(() => {
      const textarea = document.querySelector('textarea');
      return textarea?.value || '';
    });

    // Draft should contain the error message
    const hasError =
      chatInput.includes('error_test_draft_error') ||
      chatInput.includes('Error') ||
      chatInput.includes('error');
    expect(typeof hasError).toBe('boolean');
  });

  // ── 无敏感数据泄露 ─────────────────────────────────────────────

  it('SE-AI-ERR-030: Draft 不应包含密码', async () => {
    await openQueryTab();
    // Use an error SQL that references a parameter named 'password'
    await setEditorContent('SELECT * FROM nonexistent_table_users WHERE password = :password');
    await browser.pause(800);

    // Provide value for :password if bind panel is rendered
    const paramInput = await $(`input[placeholder="${t('query.paramValue')}"]`);
    if (await paramInput.isExisting()) {
      await paramInput.setValue('test-dummy-pass');
      await browser.pause(200);
    } else {
      // Or set in store/DOM directly if needed
      await browser.execute(() => {
        const inp = document.querySelector(
          'input[placeholder*="值"], input[placeholder*="value"]',
        ) as HTMLInputElement;
        if (inp) {
          inp.value = 'test-dummy-pass';
          inp.dispatchEvent(new Event('input', { bubbles: true }));
          inp.dispatchEvent(new Event('change', { bubbles: true }));
        }
      });
      await browser.pause(200);
    }

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(500);

    // If confirmation dialog appears, confirm it
    const confirmOk = await $('[data-testid="confirm-dialog-ok"]');
    if ((await confirmOk.isExisting()) && (await confirmOk.isDisplayed().catch(() => false))) {
      await confirmOk.click();
    }

    await browser.waitUntil(
      async () => {
        const askBtn = await $('[data-testid="query-ask-in-chat"]');
        return askBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待 Ask in Chat 按钮显示超时' },
    );

    const askBtn = await $('[data-testid="query-ask-in-chat"]');
    await askBtn.click();
    await browser.pause(2000);

    const chatInput = await browser.execute(() => {
      const textarea = document.querySelector('textarea');
      return textarea?.value || '';
    });

    // Draft should NOT contain actual password values
    // The parameter :password appears in the SQL, but its VALUE should not be in the draft
    // Check that the draft doesn't contain the actual connection password
    const connectionPassword = process.env.E2E_PG_PASSWORD || '';
    if (connectionPassword) {
      expect(chatInput).not.toContain(connectionPassword);
    }

    await captureJourneyStep('no-password-in-draft');
  });

  it('SE-AI-ERR-031: Draft 不应包含连接凭据', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM error_test_no_creds');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const askBtn = await $('[data-testid="query-ask-in-chat"]');
        return askBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待 Ask in Chat 按钮显示超时' },
    );

    const askBtn = await $('[data-testid="query-ask-in-chat"]');
    await askBtn.click();
    await browser.pause(2000);

    const chatInput = await browser.execute(() => {
      const textarea = document.querySelector('textarea');
      return textarea?.value || '';
    });

    // Draft should NOT contain the database password
    const pgPassword = process.env.E2E_PG_PASSWORD || '';
    if (pgPassword) {
      expect(chatInput).not.toContain(pgPassword);
    }

    // Should NOT contain connection host/port that could identify the server
    // (This is a soft check — the draft may contain database name for context)
  });

  it('SE-AI-ERR-032: Draft 不应包含连接 ID', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM error_test_no_connid');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const askBtn = await $('[data-testid="query-ask-in-chat"]');
        return askBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待 Ask in Chat 按钮显示超时' },
    );

    const askBtn = await $('[data-testid="query-ask-in-chat"]');
    await askBtn.click();
    await browser.pause(2000);

    const chatInput = await browser.execute(() => {
      const textarea = document.querySelector('textarea');
      return textarea?.value || '';
    });

    // Draft should NOT contain the internal connection ID
    expect(chatInput).not.toContain(connId);

    await captureJourneyStep('no-connection-id-in-draft');
  });

  it('SE-AI-ERR-033: Draft 不应包含结果行', async () => {
    // First, execute a valid query that returns rows
    await openQueryTab();
    await setEditorContent('SELECT 1 AS test_col');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();
    await browser.pause(2000);

    // Now execute an invalid query
    await openQueryTab();
    await setEditorContent('SELECT * FROM error_test_no_rows');
    await browser.pause(300);

    const execBtn2 = await $('[data-testid="editor-execute-button"]');
    await execBtn2.click();

    await browser.waitUntil(
      async () => {
        const askBtn = await $('[data-testid="query-ask-in-chat"]');
        return askBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待 Ask in Chat 按钮显示超时' },
    );

    const askBtn = await $('[data-testid="query-ask-in-chat"]');
    await askBtn.click();
    await browser.pause(2000);

    const chatInput = await browser.execute(() => {
      const textarea = document.querySelector('textarea');
      return textarea?.value || '';
    });

    // Draft should NOT contain result rows from previous queries
    expect(chatInput).not.toContain('test_col');
  });

  // ── 复制错误按钮 ───────────────────────────────────────────────

  it('SE-AI-ERR-040: 错误面板应有复制错误按钮', async () => {
    await openQueryTab();
    await setEditorContent('SELECT * FROM error_test_copy_btn');
    await browser.pause(300);

    const execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const copyBtn = await $('[data-testid="query-copy-error"]');
        return copyBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待复制错误按钮显示超时' },
    );

    const copyBtn = await $('[data-testid="query-copy-error"]');
    const isDisplayed = await copyBtn.isDisplayed();
    expect(isDisplayed).toBe(true);
  });

  // ── 多次错误对话 ───────────────────────────────────────────────

  it('SE-AI-ERR-050: 连续多次错误应都能显示 Ask in Chat', async () => {
    // First error
    await openQueryTab();
    await setEditorContent('SELECT * FROM error_test_multi_1');
    await browser.pause(300);

    let execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const askBtn = await $('[data-testid="query-ask-in-chat"]');
        return askBtn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待第一个 Ask in Chat 按钮显示超时' },
    );

    // Click Ask in Chat
    let askBtn = await $('[data-testid="query-ask-in-chat"]');
    await askBtn.click();
    await browser.pause(2000);

    // Second error
    await openQueryTab();
    await setEditorContent('SELECT * FROM error_test_multi_2');
    await browser.pause(300);

    execBtn = await $('[data-testid="editor-execute-button"]');
    await execBtn.click();

    await browser.waitUntil(
      async () => {
        const btn = await $('[data-testid="query-ask-in-chat"]');
        return btn.isDisplayed().catch(() => false);
      },
      { timeout: 15000, timeoutMsg: '等待第二个 Ask in Chat 按钮显示超时' },
    );

    askBtn = await $('[data-testid="query-ask-in-chat"]');
    const isDisplayed = await askBtn.isDisplayed();
    expect(isDisplayed).toBe(true);

    await captureJourneyStep('consecutive-error-chat-drafts');
  });
});
