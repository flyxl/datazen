import { expect, browser, $, $$ } from '@wdio/globals';
import * as path from 'node:path';
import fs from 'node:fs';
import { fileURLToPath } from 'node:url';
import { execSync } from 'node:child_process';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  captureJourneyStep,
  waitForNewConnectionDialog,
  findCardByName,
  expandAllGroups,
  waitForSchemaTreeLoaded,
} from '../helpers.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const ROOT = path.resolve(__dirname, '../..');
const CONN_NAME = 'E2E-SQLite-CTX';
const DB_PATH = path.resolve(__dirname, '../fixtures/test.db');

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
    await browser.pause(2000);
    return { mainWindow, connWindow };
  }

  const newConnBtn = await $(`button*=${t('action.newConnection')}`);
  await newConnBtn.click();
  await waitForNewConnectionDialog();

  const sqliteBtn = await $('button*=SQLite');
  await sqliteBtn.click();
  await browser.pause(300);

  const nameInput = await $(`input[placeholder="${t('newConn.namePlaceholder')}"]`);
  await nameInput.setValue(CONN_NAME);

  const dbInput = await $('input[placeholder="/path/to/db.sqlite"]');
  await dbInput.setValue(DB_PATH);

  const testBtn = await $(`button*=${t('newConn.testConnection')}`);
  await testBtn.click();
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return body.includes(t('newConn.testSuccess')) || body.includes('text-red-400');
    },
    { timeout: 15000 },
  );

  const saveBtn = await $(`button*=${t('common.save')}`);
  await saveBtn.click();
  await browser.waitUntil(
    async () => !(await $('[data-testid="new-connection-dialog"]').isExisting()),
    { timeout: 10000 },
  );
  await browser.switchToWindow(mainWindow);
  await browser.pause(1000);

  const card = await findCardByName(CONN_NAME);
  if (!card) throw new Error(`SQLite connection "${CONN_NAME}" not found`);

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
  await browser.pause(2000);
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

async function aiChatTextarea() {
  const textareas = await $$('aside textarea');
  if (textareas.length === 0) {
    throw new Error('AI chat textarea not found');
  }
  return textareas[textareas.length - 1];
}

async function waitForPicker() {
  await browser.waitUntil(async () => await $('[data-testid="context-picker"]').isExisting(), {
    timeout: 10000,
    timeoutMsg: 'context picker did not appear',
  });
}

async function resetAiInput() {
  const textarea = await aiChatTextarea();
  await textarea.click();
  await textarea.setValue('');
  while (await $('[data-testid="context-token"]').isExisting()) {
    await browser.keys(['Backspace']);
    await browser.pause(80);
  }
}

/** Tauri v2 freezes `__TAURI_INTERNALS__.invoke`; capture runs in `src/commands/ai.ts` when this array exists. */
async function installInvokeSpy() {
  await browser.execute(() => {
    (window as any).__invokeCalls = [];
  });
}

async function getInvokeCalls(): Promise<Array<{ cmd: string; args: Record<string, unknown> }>> {
  return browser.execute(() => (window as any).__invokeCalls ?? []);
}

async function setReactTextareaValue(text: string) {
  const ok = await browser.execute((value: string) => {
    const textarea = document.querySelector('aside textarea') as HTMLTextAreaElement | null;
    if (!textarea) return false;
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
    if (!setter) return false;
    setter.call(textarea, value);
    textarea.dispatchEvent(new Event('input', { bubbles: true }));
    textarea.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  }, text);
  expect(ok).toBe(true);
}

async function selectFirstTableItem() {
  await $('[data-testid="context-cat-tables"]').click();
  await browser.waitUntil(
    async () => (await $$('[data-testid="context-item"][data-kind="table"]')).length > 0,
    { timeout: 10000, timeoutMsg: 'no table items in picker' },
  );
  await $('[data-testid="context-item"][data-kind="table"]').click();
  await browser.pause(300);
}

describe('AI context tables (CTX-T01~T06)', () => {
  let mainWindow: string;
  let contextDir: string;

  before(async () => {
    execSync('node e2e/create-sqlite-test-db.mjs', { cwd: ROOT, stdio: 'pipe' });

    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);

    contextDir = await invokeBackend<string>('context_get_dir');
    fs.writeFileSync(
      `${contextDir}/schema.sql`,
      'CREATE TABLE users (id INT PRIMARY KEY, name VARCHAR(100));',
    );

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
    await waitForSchemaTreeLoaded(15000);
    await openAiChatPanel();
    await installInvokeSpy();
  });

  beforeEach(async () => {
    await resetAiInput();
    await browser.execute(() => {
      (window as any).__invokeCalls = [];
    });
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

  it('CTX-T01: @ shows Tables and Files categories', async () => {
    const textarea = await aiChatTextarea();
    await textarea.click();
    await textarea.setValue('@');
    await waitForPicker();
    await expect($('[data-testid="context-cat-tables"]')).toBeExisting();
    await expect($('[data-testid="context-cat-files"]')).toBeExisting();
    await captureJourneyStep('ai-context-categories');
  });

  it('CTX-T02: drill Tables then back', async () => {
    const textarea = await aiChatTextarea();
    await textarea.click();
    await textarea.setValue('@');
    await waitForPicker();

    await $('[data-testid="context-cat-tables"]').click();
    await expect($('[data-testid="context-picker-back"]')).toBeExisting();
    await expect($('[data-testid="context-item"][data-kind="table"]')).toBeExisting();

    await $('[data-testid="context-picker-back"]').click();
    await expect($('[data-testid="context-cat-tables"]')).toBeExisting();
  });

  it('CTX-T03: nested Tables filter narrows by keyboard', async () => {
    const textarea = await aiChatTextarea();
    await textarea.click();
    await textarea.setValue('@');
    await waitForPicker();

    // @ only opens categories (no cross-list dump at root).
    await expect($('[data-testid="context-cat-tables"]')).toBeExisting();
    await $('[data-testid="context-cat-tables"]').click();
    await expect($('[data-testid="context-picker-back"]')).toBeExisting();

    const before = await $$('[data-testid="context-item"][data-kind="table"]');
    expect(before.length).toBeGreaterThan(1);

    // Same single @: query text filters after drill-in (no second @).
    // Re-open with @users then enter Tables — nested list is filtered.
    await textarea.setValue('@users');
    await waitForPicker();
    await expect($('[data-testid="context-cat-tables"]')).toBeExisting();
    await $('[data-testid="context-cat-tables"]').click();
    await expect($('[data-testid="context-picker-back"]')).toBeExisting();

    const items = await $$('[data-testid="context-item"][data-kind="table"]');
    expect(items.length).toBeGreaterThan(0);
    expect(items.length).toBeLessThan(before.length);

    let hasUsersTable = false;
    for (const item of items) {
      const id = await item.getAttribute('data-id');
      if (id?.includes('users')) {
        hasUsersTable = true;
      }
    }
    expect(hasUsersTable).toBe(true);
  });

  it('CTX-T04: select table → token; Backspace removes', async () => {
    const textarea = await aiChatTextarea();
    await textarea.click();
    await textarea.setValue('@');
    await waitForPicker();
    await selectFirstTableItem();
    await expect($('[data-testid="context-token"][data-kind="table"]')).toBeExisting();
    await captureJourneyStep('ai-context-table-selected');

    await textarea.click();
    await textarea.setValue('');
    await browser.keys(['Backspace']);
    await browser.pause(200);
    await expect($('[data-testid="context-token"][data-kind="table"]')).not.toBeExisting();
  });

  it('CTX-T05: file select still works', async () => {
    const textarea = await aiChatTextarea();
    await textarea.click();
    await textarea.setValue('@');
    await waitForPicker();

    await $('[data-testid="context-cat-files"]').click();
    await browser.waitUntil(
      async () => (await $$('[data-testid="context-item"][data-kind="file"]')).length > 0,
      { timeout: 10000, timeoutMsg: 'no file items in picker' },
    );

    const fileItems = await $$('[data-testid="context-item"][data-kind="file"]');
    let schemaItem = fileItems[0];
    for (const item of fileItems) {
      const id = await item.getAttribute('data-id');
      if (id?.includes('schema.sql')) {
        schemaItem = item;
        break;
      }
    }
    await schemaItem.click();
    await browser.pause(300);

    await expect($('[data-testid="context-token"][data-kind="file"]')).toBeExisting();
  });

  it('CTX-T06: send includes context_tables', async () => {
    const textarea = await aiChatTextarea();
    await textarea.click();
    await textarea.setValue('@');
    await waitForPicker();
    await selectFirstTableItem();

    await setReactTextareaValue('list rows from pinned table');
    await browser.pause(200);

    const sendClicked = await browser.execute(() => {
      for (const aside of document.querySelectorAll('aside')) {
        const btn = aside.querySelector('button .lucide-arrow-up')?.closest('button');
        if (btn && !(btn as HTMLButtonElement).disabled) {
          (btn as HTMLButtonElement).click();
          return true;
        }
      }
      return false;
    });
    expect(sendClicked).toBe(true);

    await browser.waitUntil(
      async () => {
        const calls = await getInvokeCalls();
        return calls.some((c) => c.cmd === 'ai_chat' || c.cmd === 'ai_generate_sql');
      },
      { timeout: 10000, timeoutMsg: 'ai_chat invoke not captured' },
    );

    const calls = await getInvokeCalls();
    const aiCall = calls.find((c) => c.cmd === 'ai_chat' || c.cmd === 'ai_generate_sql');
    expect(aiCall).toBeDefined();

    const tables =
      (aiCall!.args.contextTables as string[] | undefined) ??
      (aiCall!.args.context_tables as string[] | undefined);
    expect(Array.isArray(tables)).toBe(true);
    expect(tables!.length).toBeGreaterThan(0);
  });
});
