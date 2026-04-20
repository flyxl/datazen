/**
 * Shared E2E test helpers for DataZen.
 *
 * Centralises common patterns so individual spec files stay focused on
 * feature-specific assertions.
 */
import { browser, $, $$ } from '@wdio/globals';

// ── window management ───────────────────────────────────────────────

export async function switchToNewWindow(originalHandle: string): Promise<string> {
  await browser.waitUntil(
    async () => (await browser.getWindowHandles()).length > 1,
    { timeout: 15000, timeoutMsg: '等待新窗口打开超时' },
  );
  const handles = await browser.getWindowHandles();
  const newHandle = handles.find((h) => h !== originalHandle)!;
  await browser.switchToWindow(newHandle);
  return newHandle;
}

export async function closeExtraWindows(mainWindow: string) {
  const handles = await browser.getWindowHandles();
  for (const h of handles) {
    if (h !== mainWindow) {
      await browser.switchToWindow(h);
      await browser.closeWindow();
    }
  }
  await browser.switchToWindow(mainWindow);
}

// ── main window ─────────────────────────────────────────────────────

/**
 * Click the "连接" button on a card with PostgreSQL badge (the seeded connection).
 * If already connected (window open), returns true without clicking.
 */
export async function clickCardConnectButton() {
  // Check if already connected (connection window already open)
  const handles = await browser.getWindowHandles();
  if (handles.length > 1) {
    return true;
  }

  // Check if there's a "断开" button (already connected but window closed)
  // Disconnect first and then reconnect
  const allButtons = await $$('button');
  for (const btn of allButtons) {
    const text = (await btn.getText()).trim();
    if (text === '断开') {
      await btn.click();
      await browser.pause(1000);
      break;
    }
  }

  const cards = await $$('.group.relative');
  for (const card of cards) {
    const cardText = await card.getText();
    if (cardText.includes('PostgreSQL')) {
      const btns = await card.$$('button');
      for (const btn of btns) {
        const text = (await btn.getText()).trim();
        if (text === '连接') {
          await btn.click();
          return true;
        }
      }
    }
  }
  // Fallback: click any "连接" button
  const buttons = await $$('button');
  for (const btn of buttons) {
    const text = (await btn.getText()).trim();
    if (text === '连接') {
      await btn.click();
      return true;
    }
  }
  return false;
}

/** Find a connection card by name in the main window. */
export async function findCardByName(connName: string) {
  const cards = await $$('.group.relative');
  for (const card of cards) {
    const text = await card.getText();
    if (text.includes(connName)) return card;
  }
  return null;
}

// ── connection window helpers ───────────────────────────────────────

/**
 * Open a connection from main window and switch to the connection window.
 * Returns { mainWindow, connWindow }.
 */
export async function openConnectionWindow() {
  const mainWindow = await browser.getWindowHandle();
  await $('button*=新建连接').waitForDisplayed({ timeout: 10000 });
  await browser.pause(1500);

  await clickCardConnectButton();
  await browser.waitUntil(
    async () => (await browser.getWindowHandles()).length > 1,
    { timeout: 30000, timeoutMsg: '等待连接窗口打开超时' },
  );
  const handles = await browser.getWindowHandles();
  const connWindow = handles.find((h) => h !== mainWindow)!;
  await browser.switchToWindow(connWindow);
  await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
  await browser.pause(2000);

  return { mainWindow, connWindow };
}

// ── SQL / CodeMirror ────────────────────────────────────────────────

/** Replace CodeMirror editor content using execCommand. */
export async function setEditorContent(sql: string) {
  await browser.execute((text: string) => {
    const el = document.querySelector('.cm-editor .cm-content') as HTMLElement;
    if (!el) return;
    el.focus();
    const sel = window.getSelection();
    if (sel) {
      sel.selectAllChildren(el);
      sel.deleteFromDocument();
    }
    document.execCommand('insertText', false, text);
  }, sql);
  await browser.pause(300);
}

/** Execute SQL in the currently active query tab and wait for completion. */
export async function executeSQL(sql: string) {
  await setEditorContent(sql);
  const execBtn = await $('button*=执行');
  await execBtn.click();
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return body.includes('总耗时') || body.includes('text-red-400');
    },
    { timeout: 15000, timeoutMsg: `等待 SQL 执行完成超时: ${sql.slice(0, 60)}` },
  );
  await browser.pause(500);
}

/** Open a new query tab and wait for the execute button. */
export async function openQueryTab() {
  const newQueryBtn = await $('button*=新建查询');
  await newQueryBtn.click();
  await browser.pause(500);
  const execBtn = await $('button*=执行');
  await execBtn.waitForDisplayed({ timeout: 5000 });
}

// ── schema sidebar ──────────────────────────────────────────────────

/** Click a table by exact name in the sidebar. */
export async function clickTableInSidebar(tableName: string) {
  await browser.waitUntil(
    async () => (await $('aside').getText()).includes('Tables'),
    { timeout: 10000, timeoutMsg: '等待 Tables 列表加载超时' },
  );
  const asideButtons = await $$('aside button');
  for (const btn of asideButtons) {
    const text = (await btn.getText()).trim();
    if (text === tableName) {
      await btn.click();
      return;
    }
  }
  throw new Error(`未找到表 "${tableName}"`);
}

/** Click the first table/view entry in the sidebar and return its name. */
export async function clickFirstTable() {
  await browser.waitUntil(
    async () => (await $('aside').getText()).includes('Tables'),
    { timeout: 10000, timeoutMsg: '等待 Tables 列表加载超时' },
  );
  const asideButtons = await $$('aside button');
  for (const btn of asideButtons) {
    const text = (await btn.getText()).trim();
    const cls = (await btn.getAttribute('class')) || '';
    if (cls.includes('text-left') && cls.includes('13px') && text.length > 0) {
      await btn.click();
      return text;
    }
  }
  return null;
}

/** Switch to a sub-tab inside a table panel (数据/结构/索引/外键/DDL). */
export async function switchSubTab(label: string) {
  const tab = await $(`button*=${label}`);
  await tab.click();
  await browser.pause(500);
}

// ── DataTable cell interaction ──────────────────────────────────────

/** Double-click a cell by its displayed text using synthetic dblclick. */
export async function doubleClickCellByText(text: string) {
  await browser.waitUntil(
    async () => {
      const el = await $(`span[title="${text}"]`);
      return el.isDisplayed();
    },
    { timeout: 8000, timeoutMsg: `等待 "${text}" 单元格显示超时` },
  );
  await browser.execute((t: string) => {
    const el = document.querySelector(`span[title="${t}"]`);
    if (!el) return;
    el.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
  }, text);
  await browser.pause(500);
}

/** Wait for the inline editing input to appear and return it. */
export async function waitForEditInput() {
  await browser.waitUntil(
    async () => {
      const exists = await browser.execute(() => !!document.querySelector('input.font-mono'));
      return exists;
    },
    { timeout: 8000, timeoutMsg: '等待编辑 input 出现超时' },
  );
  return $('input.font-mono');
}

/** Use the Zustand store to directly update a cell value and commit. */
export async function storeUpdateCell(rowIndex: number, columnName: string, value: unknown) {
  await browser.execute(
    (row: number, col: string, val: unknown) => {
      const store = (window as any).__tableDataStore;
      if (!store) throw new Error('__tableDataStore not found');
      store.getState().updateCell(row, col, val);
    },
    rowIndex,
    columnName,
    value,
  );
  await browser.pause(2000);
}
