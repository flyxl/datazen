import { expect, browser, $ } from '@wdio/globals';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  waitForNewConnectionDialog,
  findCardByName,
  expandAllGroups,
} from '../helpers.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CONN_NAME = 'E2E-SQLite-CODEBLOCK';
const DB_PATH = path.resolve(__dirname, '../fixtures/test.db');

const SQL_MSG =
  'Try this query:\n```sql\nSELECT id FROM users LIMIT 10;\n```\nLet me know if you need changes.';

async function createAndConnectSQLite() {
  const mainWindow = await browser.getWindowHandle();
  await expandAllGroups();

  const existingItem = await findCardByName(CONN_NAME);
  if (existingItem) {
    await browser.execute((n: string) => {
      const items = document.querySelectorAll('[data-conn-item]');
      for (const item of items) {
        if (item.textContent?.includes(n)) {
          item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
          return;
        }
      }
    }, CONN_NAME);
    await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
      timeout: 30000,
      timeoutMsg: '等待 SQLite 连接窗口打开超时',
    });
    const handles = await browser.getWindowHandles();
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);
    await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
    await browser.pause(1500);
    return { mainWindow, connWindow };
  }

  const newConnBtn = await $(`button*=${t('action.newConnection')}`);
  await newConnBtn.click();
  await waitForNewConnectionDialog();

  await (await $('button*=SQLite')).click();
  await browser.pause(300);

  await (await $(`input[placeholder="${t('newConn.namePlaceholder')}"]`)).setValue(CONN_NAME);
  await (await $('input[placeholder="/path/to/db.sqlite"]')).setValue(DB_PATH);

  await (await $(`button*=${t('newConn.testConnection')}`)).click();
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return body.includes(t('newConn.testSuccess')) || body.includes('text-red-400');
    },
    { timeout: 15000 },
  );

  await (await $(`button*=${t('common.save')}`)).click();
  await browser.waitUntil(
    async () => !(await $('[data-testid="new-connection-dialog"]').isExisting()),
    { timeout: 10000 },
  );
  await browser.switchToWindow(mainWindow);
  await browser.pause(1000);

  await browser.execute((n: string) => {
    const items = document.querySelectorAll('[data-conn-item]');
    for (const item of items) {
      if (item.textContent?.includes(n)) {
        item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        return;
      }
    }
  }, CONN_NAME);

  await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
    timeout: 30000,
  });
  const handles = await browser.getWindowHandles();
  const connWindow = handles.find((h) => h !== mainWindow)!;
  await browser.switchToWindow(connWindow);
  await $(`button*=${t('connWin.newQuery')}`).waitForDisplayed({ timeout: 20000 });
  await browser.pause(1500);
  return { mainWindow, connWindow };
}

async function openAiChatPanel() {
  const clicked = await browser.execute(() => {
    for (const btn of document.querySelectorAll('button')) {
      if (btn.querySelector('.lucide-message-square')) {
        btn.click();
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
  let connWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await browser.pause(1500);
    const ctx = await createAndConnectSQLite();
    connWindow = ctx.connWindow;
    await openAiChatPanel();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
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
