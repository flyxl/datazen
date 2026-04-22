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

/** Expand all collapsed groups so connection items become visible. */
export async function expandAllGroups() {
  // Lucide renders <svg class="lucide lucide-chevron-right ..."> when collapsed
  // and <svg class="lucide lucide-chevron-down ..."> when expanded.
  // Click only headers whose SVG indicates collapsed state.
  await browser.execute(() => {
    document.querySelectorAll('[data-group-header]').forEach((header) => {
      const svg = header.querySelector('svg');
      if (!svg) return;
      const classes = svg.getAttribute('class') || '';
      if (classes.includes('chevron-right')) {
        (header as HTMLElement).click();
      }
    });
  });
  await browser.pause(500);
}

/**
 * Double-click a connection item in the new grouped list to open it.
 * Searches for a connection containing `nameFragment` in its text.
 * If already connected (window open), returns true without clicking.
 */
export async function clickCardConnectButton(nameFragment = 'Pg') {
  const handles = await browser.getWindowHandles();
  if (handles.length > 1) return true;

  // Expand groups first so items are visible
  await expandAllGroups();

  // Use JS dblclick dispatch since WebDriver dblclick may not work in WebKit
  const found = await browser.execute((frag: string) => {
    const items = document.querySelectorAll('[data-conn-item]');
    for (const item of items) {
      const text = item.textContent || '';
      if (text.includes(frag) || text.includes('Postgres') || text.includes('PostgreSQL') || text.includes('localhost')) {
        item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        return true;
      }
    }
    if (items.length > 0) {
      items[0].dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
      return true;
    }
    return false;
  }, nameFragment);
  return found;
}

/** Find a connection item by name in the main window. */
export async function findCardByName(connName: string) {
  await expandAllGroups();
  const items = await $$('[data-conn-item]');
  for (const item of items) {
    const text = await item.getText();
    if (text.includes(connName)) return item;
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
  // Expand groups and wait for the main window to render connection items
  await expandAllGroups();
  await browser.waitUntil(
    async () => (await $$('[data-conn-item]')).length > 0,
    { timeout: 15000, timeoutMsg: '等待连接项加载超时' },
  );
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

// ── MySQL connection helpers ─────────────────────────────────────────

/**
 * Create a MySQL connection via the new-connection UI and connect to it.
 * Returns { mainWindow, connWindow }.
 * Assumes we start on the main window.
 */
export async function createAndConnectMySQL(opts: {
  name?: string;
  host?: string;
  port?: string;
  user?: string;
  password?: string;
  database?: string;
} = {}) {
  const {
    name = 'E2E-MySQL',
    host = '127.0.0.1',
    port = '3306',
    user = 'root',
    password = '',
    database = 'datazen_test',
  } = opts;

  const mainWindow = await browser.getWindowHandle();

  // Expand groups so items are visible
  await expandAllGroups();

  // Check if the MySQL connection item already exists and just double-click to connect
  const existingItem = await findCardByName(name);
  if (existingItem) {
    await browser.execute((n: string) => {
      const items = document.querySelectorAll('[data-conn-item]');
      for (const item of items) {
        if (item.textContent?.includes(n)) {
          item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
          return;
        }
      }
    }, name);
    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 30000, timeoutMsg: '等待 MySQL 连接窗口打开超时' },
    );
    const handles = await browser.getWindowHandles();
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);
    await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
    await browser.pause(2000);
    return { mainWindow, connWindow };
  }

  // Create a new MySQL connection
  const newConnBtn = await $('button*=新建连接');
  await newConnBtn.click();
  const newConnWindow = await switchToNewWindow(mainWindow);

  // Select MySQL type
  const mysqlBtn = await $('button*=MySQL');
  await mysqlBtn.click();
  await browser.pause(300);

  // Fill form fields
  const nameInput = await $('input[placeholder="例如：主数据库"]');
  await nameInput.setValue(name);

  const hostInput = await $('input[placeholder="prod-db.example.com"]');
  await hostInput.clearValue();
  await hostInput.setValue(host);

  // Port input
  const allInputs = await $$('input');
  for (const inp of allInputs) {
    if ((await inp.getValue()) === '3306') {
      await inp.clearValue();
      await inp.setValue(port);
      break;
    }
  }

  // Database
  const dbInput = await $('input[placeholder="myapp_production"]');
  await dbInput.setValue(database);

  // Username
  const userInput = await $('input[placeholder="postgres"]');
  await userInput.clearValue();
  await userInput.setValue(user);

  // Password
  if (password) {
    const pwInput = await $('input[type="password"]');
    await pwInput.setValue(password);
  }

  // Save
  const saveBtn = await $('button*=保存');
  await saveBtn.click();
  await browser.waitUntil(
    async () => (await browser.getWindowHandles()).length === 1,
    { timeout: 10000, timeoutMsg: '保存连接后窗口未关闭' },
  );
  await browser.switchToWindow(mainWindow);
  await browser.pause(1000);

  // Now connect by double-clicking the item
  const card = await findCardByName(name);
  if (!card) throw new Error(`未找到 MySQL 连接 "${name}"`);
  await browser.execute((n: string) => {
    const items = document.querySelectorAll('[data-conn-item]');
    for (const item of items) {
      if (item.textContent?.includes(n)) {
        item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        return;
      }
    }
  }, name);

  await browser.waitUntil(
    async () => (await browser.getWindowHandles()).length > 1,
    { timeout: 30000, timeoutMsg: '等待 MySQL 连接窗口打开超时' },
  );
  const handles = await browser.getWindowHandles();
  const connWindow = handles.find((h) => h !== mainWindow)!;
  await browser.switchToWindow(connWindow);
  await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
  await browser.pause(2000);

  return { mainWindow, connWindow };
}

/**
 * Connect to a named connection card (any DB type) from the main window.
 */
export async function connectToCard(cardName: string) {
  const mainWindow = await browser.getWindowHandle();
  await expandAllGroups();
  const card = await findCardByName(cardName);
  if (!card) throw new Error(`未找到连接 "${cardName}"`);

  await browser.execute((n: string) => {
    const items = document.querySelectorAll('[data-conn-item]');
    for (const item of items) {
      if (item.textContent?.includes(n)) {
        item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        return;
      }
    }
  }, cardName);

  await browser.waitUntil(
    async () => (await browser.getWindowHandles()).length > 1,
    { timeout: 30000, timeoutMsg: `等待 "${cardName}" 连接窗口打开超时` },
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
