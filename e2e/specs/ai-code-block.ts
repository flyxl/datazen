import { expect, browser, $ } from '@wdio/globals';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { execSync } from 'node:child_process';
import {
  closeExtraWindows,
  findCardByName,
  expandAllGroups,
  dblclickConnByExactName,
  waitForConnectionToolbar,
} from '../helpers.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '../..');
const CONN_NAME = 'E2E-SQLite-CODEBLOCK';
const DB_PATH = path.resolve(__dirname, '../fixtures/test.db');

const SQL_MSG =
  'Try this query:\n```sql\nSELECT id FROM users LIMIT 10;\n```\nLet me know if you need changes.';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke(c, JSON.parse(a))
        .then((r: unknown) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && '__error' in (result as Record<string, unknown>)) {
    throw new Error((result as Record<string, unknown>).__error as string);
  }
  return result as T;
}

const CONN_ID = 'conn_e2e_sqlite_codeblock';

async function ensureSQLiteConnection() {
  const conns = await invokeBackend<Array<{ id: string; name: string }>>('get_connections');
  if (!conns.some((c) => c.id === CONN_ID || c.name === CONN_NAME)) {
    await invokeBackend('save_connection', {
      config: {
        id: CONN_ID,
        name: CONN_NAME,
        databaseType: 'sqlite',
        host: '',
        port: 0,
        username: '',
        password: '',
        database: DB_PATH,
        group: 'E2E 测试',
        sslMode: 'disable',
      },
    });
    await browser.refresh();
    await browser.pause(1500);
  }
}

async function createAndConnectSQLite() {
  const mainWindow = await browser.getWindowHandle();
  await expandAllGroups();
  await ensureSQLiteConnection();

  const existingItem = await findCardByName(CONN_NAME);
  if (!existingItem) {
    throw new Error(`SQLite connection "${CONN_NAME}" not found after save`);
  }

  await dblclickConnByExactName(CONN_NAME);
  await browser.switchToWindow(mainWindow);
  await waitForConnectionToolbar();
  return { mainWindow, connWindow: mainWindow };
}

async function openQueryTabIfNeeded() {
  const hasEditor = await browser.execute(() => !!document.querySelector('.cm-editor'));
  if (hasEditor) return;

  const clicked = await browser.execute(() => {
    const btn = Array.from(document.querySelectorAll('button')).find((b) => {
      const text = b.textContent || '';
      return text.includes('新建查询') || text.includes('New Query') || text.includes('New query');
    });
    if (btn) {
      btn.click();
      return true;
    }
    return false;
  });
  expect(clicked).toBe(true);
  await browser.pause(500);
}

async function openAiChatPanel() {
  const clicked = await browser.execute(() => {
    for (const btn of document.querySelectorAll('button')) {
      if (btn.querySelector('[class*="message-square"]')) {
        btn.click();
        return true;
      }
    }
    const toolbar = document.querySelector('[class*="bg-surface-alt"][class*="border-b"]');
    if (toolbar) {
      const btns = Array.from(toolbar.querySelectorAll('button'));
      const aiBtn = btns.length >= 2 ? btns[btns.length - 2] : btns[btns.length - 1];
      if (aiBtn) {
        aiBtn.click();
        return true;
      }
    }
    return false;
  });
  expect(clicked).toBe(true);
  await browser.pause(800);
}

async function injectAssistantMessage(content: string) {
  return browser.execute((c: string) => {
    const store = (window as any).__datazenAiStore;
    if (!store?.getState) return false;
    const state = store.getState();
    if (!state.chatSession) {
      if (typeof state.initChatSession === 'function') state.initChatSession();
    }
    const next = store.getState();
    if (!next.chatSession) return false;
    store.setState({
      isConfigured: true,
      chatSession: {
        ...next.chatSession,
        messages: [...(next.chatSession.messages || []), { role: 'assistant', content: c }],
        isStreaming: false,
      },
    });
    return true;
  }, content);
}

describe('AI Chat inline code blocks (E2E)', () => {
  let mainWindow: string;

  before(async () => {
    execSync('node e2e/create-sqlite-test-db.mjs', { cwd: ROOT, stdio: 'pipe' });

    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);

    await invokeBackend('ai_save_config', {
      config: {
        providerType: 'open_ai',
        endpoint: 'https://example.com/v1',
        apiKey: 'e2e-stub-key',
        model: 'stub',
        extra: null,
      },
    });

    const windows = await createAndConnectSQLite();
    mainWindow = windows.mainWindow;
    await openQueryTabIfNeeded();
    await openAiChatPanel();
  });

  after(async () => {
    try {
      await invokeBackend('ai_delete_config');
    } catch {
      // ignore
    }
    if (mainWindow) {
      await closeExtraWindows(mainWindow);
    }
  });

  it('E2E-CB-01: renders inline code block for fenced SQL', async () => {
    expect(await injectAssistantMessage(SQL_MSG)).toBe(true);
    await browser.pause(500);

    const codeBlock = await $('[data-testid="ai-code-block"]');
    await codeBlock.waitForDisplayed({ timeout: 10000 });

    const hasRawFence = await browser.execute(
      () => document.body.textContent?.includes('```sql') ?? false,
    );
    expect(hasRawFence).toBe(false);

    const blockText = await codeBlock.getText();
    expect(blockText).toContain('SELECT id FROM users LIMIT 10');
  });

  it('E2E-CB-02: no legacy bottom action chips outside code block header', async () => {
    const chipCount = await browser.execute(() => {
      const blocks = document.querySelectorAll('[data-testid="ai-code-block"]');
      let outside = 0;
      for (const btn of document.querySelectorAll('button')) {
        const label = btn.textContent || '';
        if (!label.includes('Insert') && !label.includes('插入')) continue;
        const insideBlock = Array.from(blocks).some((b) => b.contains(btn));
        if (!insideBlock) outside += 1;
      }
      return outside;
    });
    expect(chipCount).toBe(0);
  });

  it('E2E-CB-04: insert SQL into query editor', async () => {
    await openQueryTabIfNeeded();
    const insertBtn = await $('[data-testid="ai-code-insert"]');
    await insertBtn.waitForDisplayed({ timeout: 5000 });
    await insertBtn.click();
    await browser.pause(500);

    const editorContent = await browser.execute(() => {
      const el = document.querySelector('.cm-editor .cm-content') as HTMLElement | null;
      return el?.textContent || '';
    });
    expect(editorContent).toContain('SELECT id FROM users LIMIT 10');
  });

  it('E2E-CB-05: JSON block has no insert button', async () => {
    const jsonMsg = 'Config:\n```json\n{"a":1}\n```';
    expect(await injectAssistantMessage(jsonMsg)).toBe(true);
    await browser.pause(500);

    const jsonBlocks = await browser.execute(() => {
      const blocks = Array.from(document.querySelectorAll('[data-testid="ai-code-block"]'));
      return blocks.filter((b) => b.textContent?.includes('{"a":1}')).length;
    });
    expect(jsonBlocks).toBeGreaterThan(0);

    const hasJsonInsert = await browser.execute(() => {
      const blocks = Array.from(document.querySelectorAll('[data-testid="ai-code-block"]'));
      for (const block of blocks) {
        if (!block.textContent?.includes('{"a":1}')) continue;
        return !!block.querySelector('[data-testid="ai-code-insert"]');
      }
      return false;
    });
    expect(hasJsonInsert).toBe(false);
  });
});
