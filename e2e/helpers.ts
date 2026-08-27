/**
 * Shared E2E test helpers for DataZen.
 *
 * Centralises common patterns so individual spec files stay focused on
 * feature-specific assertions.
 */
import { browser, $, $$ } from '@wdio/globals';
import { isScreenshotTraceEnabled, saveJourneyScreenshot } from './lib/screenshotTrace.js';

/** Manual journey step capture (`--screenshot`); also used by helpers below. */
export async function captureJourneyStep(label: string, settleMs = 400, force = false) {
  if (!isScreenshotTraceEnabled()) return;
  await saveJourneyScreenshot(browser, label, settleMs, force);
}

// ── window management ───────────────────────────────────────────────

/** Wait until the in-app new-connection dialog is visible. */
export async function waitForNewConnectionDialog(timeout = 15000) {
  const dialog = await $('[data-testid="new-connection-dialog"]');
  await dialog.waitForDisplayed({ timeout, timeoutMsg: '等待新建连接弹窗打开超时' });
  return dialog;
}

/** Click the main-window "New Connection" entry point and wait for the dialog. */
export async function openNewConnectionDialogFromUi() {
  const clicked = await browser.execute(() => {
    const textBtn = Array.from(document.querySelectorAll('button')).find(
      (b) => b.textContent?.includes('新建连接') || b.textContent?.includes('New Connection'),
    );
    if (textBtn) {
      textBtn.click();
      return true;
    }
    const titleBtn = document.querySelector(
      'button[title="新建连接"], button[title="New Connection"]',
    ) as HTMLButtonElement | null;
    if (titleBtn) {
      titleBtn.click();
      return true;
    }
    return false;
  });
  if (!clicked) throw new Error('Could not find "新建连接" button');
  await waitForNewConnectionDialog();
}

/** Close the new-connection dialog via Cancel (stays on main window). */
export async function closeNewConnectionDialogFromUi() {
  await browser.execute(() => {
    const dialog = document.querySelector('[data-testid="new-connection-dialog"]');
    if (!dialog) return;
    const buttons = Array.from(dialog.querySelectorAll('button'));
    const cancel = buttons.find(
      (b) => b.textContent?.includes('取消') || b.textContent?.includes('Cancel'),
    );
    cancel?.click();
  });
  await browser.waitUntil(
    async () => !(await $('[data-testid="new-connection-dialog"]').isExisting()),
    { timeout: 10000, timeoutMsg: '等待新建连接弹窗关闭超时' },
  );
}

export async function switchToNewWindow(originalHandle: string): Promise<string> {
  await browser.waitUntil(async () => (await browser.getWindowHandles()).length > 1, {
    timeout: 15000,
    timeoutMsg: '等待新窗口打开超时',
  });
  const handles = await browser.getWindowHandles();
  const newHandle = handles.find((h) => h !== originalHandle)!;
  await browser.switchToWindow(newHandle);
  return newHandle;
}

export async function closeExtraWindows(mainWindow: string) {
  let handles = await browser.getWindowHandles();
  for (const h of handles) {
    if (h !== mainWindow) {
      try {
        await browser.switchToWindow(h);
        await browser.closeWindow();
      } catch {
        /* window already gone */
      }
    }
  }
  handles = await browser.getWindowHandles();
  const target = handles.includes(mainWindow) ? mainWindow : handles[0];
  if (target) {
    await browser.switchToWindow(target);
  }
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

/** Seeded by wdio.conf.ts — locked to E2E_PG_DB so StandardSchemaTree is used. */
export const E2E_PG_CONN_NAME = '本地 PostgreSQL';

/**
 * Double-click a connection item in the new grouped list to open it.
 * Prefers the seeded Host PG connection so leftover MultiDb cards are not opened.
 * If already connected (window open), returns true without clicking.
 */
export async function clickCardConnectButton(nameFragment = E2E_PG_CONN_NAME) {
  const current = await browser.getWindowHandle();
  const handles = await browser.getWindowHandles();
  if (handles.length > 1) {
    await closeExtraWindows(current);
  }

  // Expand groups first so items are visible
  await expandAllGroups();

  // Use JS dblclick dispatch since WebDriver dblclick may not work in WebKit
  const found = await browser.execute((frag: string) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    const textOf = (item: Element) => item.textContent || '';
    const click = (item: Element) => {
      item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
    };
    const prefer = items.find((item) => textOf(item).includes(frag));
    if (prefer) {
      click(prefer);
      return true;
    }
    const pg = items.find((item) => {
      const text = textOf(item);
      if (text.includes('MultiDb')) return false;
      return text.includes('PostgreSQL') || text.includes('Postgres') || text.includes('Pg');
    });
    if (pg) {
      click(pg);
      return true;
    }
    if (items.length > 0) {
      click(items[0]);
      return true;
    }
    return false;
  }, nameFragment);
  return found;
}

/** Find a connection item by exact name (avoids E2E-MySQL matching E2E-MySQL-MultiDb). */
export async function findCardByName(connName: string) {
  await expandAllGroups();
  const idx = await browser.execute((n: string) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    return items.findIndex((item) => {
      const attr = item.getAttribute('data-conn-name');
      if (attr) return attr === n;
      return item.querySelector('span.truncate')?.textContent?.trim() === n;
    });
  }, connName);
  if (idx < 0) return null;
  return (await $$('[data-conn-item]'))[idx];
}

export async function dblclickConnByExactName(connName: string) {
  const ok = await browser.execute((n: string) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    const item = items.find((el) => {
      const attr = el.getAttribute('data-conn-name');
      if (attr) return attr === n;
      return el.querySelector('span.truncate')?.textContent?.trim() === n;
    });
    if (!item) return false;
    item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
    return true;
  }, connName);
  return ok;
}

// ── unified workspace / connection helpers ──────────────────────────

/** Wait until the connection content toolbar is visible (connected state). */
export async function waitForConnectionToolbar(timeout = 20000) {
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return (
        body.includes('新建查询') ||
        body.includes('New Query') ||
        body.includes('新查詢') ||
        body.includes('Neue Abfrage')
      );
    },
    { timeout, timeoutMsg: 'Timed out waiting for connection toolbar' },
  );
  await browser.pause(800);
}

/** Single-click expand chevron on a connected card so schema children load. */
export async function expandConnectedConnectionInNavigator(nameFragment = E2E_PG_CONN_NAME) {
  await browser.waitUntil(
    async () =>
      browser.execute((frag: string) => {
        const items = Array.from(document.querySelectorAll('[data-conn-item]'));
        const item = items.find((el) => (el.textContent || '').includes(frag));
        return !!item?.querySelector('button[aria-expanded]');
      }, nameFragment),
    { timeout: 15000, timeoutMsg: '等待连接就绪以展开 schema 树' },
  );
  await browser.execute((frag: string) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    const item = items.find((el) => (el.textContent || '').includes(frag));
    const chevron = item?.querySelector('button[aria-expanded]') as HTMLElement | null;
    if (chevron && chevron.getAttribute('aria-expanded') !== 'true') {
      chevron.click();
    } else {
      item?.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    }
  }, nameFragment);
  await browser.pause(800);
}

/** Double-click seeded PG connection and wait for toolbar in the unified main window. */
export async function connectSeededPgInWorkspace() {
  await expandAllGroups();
  await browser.waitUntil(async () => (await $$('[data-conn-item]')).length > 0, {
    timeout: 15000,
    timeoutMsg: '等待连接项加载超时',
  });
  await clickCardConnectButton();
  await waitForConnectionToolbar();
  await expandConnectedConnectionInNavigator();
}

/**
 * Open a connection from the unified main workspace.
 * Returns `{ mainWindow, connWindow }` where both handles refer to the same OS window.
 */
export async function openConnectionWindow() {
  const mainWindow = await browser.getWindowHandle();
  await connectSeededPgInWorkspace();
  return { mainWindow, connWindow: mainWindow };
}

/** @deprecated Use {@link openConnectionWindow}; kept for contract matrix imports. */
export async function openSeededPgConnectionWindow(mainWindow: string) {
  await browser.switchToWindow(mainWindow);
  const body = await $('body').getText();
  const connected =
    body.includes('新建查询') || body.includes('New Query') || body.includes('新查詢');
  if (!connected) {
    await connectSeededPgInWorkspace();
  } else {
    await waitForConnectionToolbar();
  }
  return mainWindow;
}

async function finishConnectInWorkspace(mainWindow: string) {
  await browser.switchToWindow(mainWindow);
  await waitForConnectionToolbar();
  return { mainWindow, connWindow: mainWindow };
}

// ── MySQL connection helpers ─────────────────────────────────────────

/**
 * Create a MySQL connection via the new-connection UI and connect to it.
 * Returns { mainWindow, connWindow }.
 * Assumes we start on the main window.
 */
export async function createAndConnectMySQL(
  opts: {
    name?: string;
    host?: string;
    port?: string;
    user?: string;
    password?: string;
    /** Pass empty string to leave the database field blank (multi-db session). */
    database?: string;
  } = {},
) {
  const {
    name = 'E2E-MySQL',
    host = process.env.E2E_MYSQL_HOST || '127.0.0.1',
    port = process.env.E2E_MYSQL_PORT || '3306',
    user = process.env.E2E_MYSQL_USER || 'root',
    password = process.env.E2E_MYSQL_PASSWORD || '',
    database = process.env.E2E_MYSQL_DB || 'datazen_test',
  } = opts;

  const mainWindow = await browser.getWindowHandle();

  // Expand groups so items are visible
  await expandAllGroups();

  // Check if the MySQL connection item already exists and just double-click to connect
  const existingItem = await findCardByName(name);
  if (existingItem) {
    await dblclickConnByExactName(name);
    return finishConnectInWorkspace(mainWindow);
  }

  // Create a new MySQL connection (icon-only toolbar button OR text button in empty state)
  const clickedNew = await browser.execute(() => {
    // Try text button first (empty state)
    const textBtn = Array.from(document.querySelectorAll('button')).find(
      (b) => b.textContent?.includes('新建连接') || b.textContent?.includes('New Connection'),
    );
    if (textBtn) {
      textBtn.click();
      return true;
    }
    // Fallback: icon-only toolbar button by title
    const titleBtn = document.querySelector(
      'button[title="新建连接"], button[title="New Connection"]',
    ) as HTMLButtonElement | null;
    if (titleBtn) {
      titleBtn.click();
      return true;
    }
    return false;
  });
  if (!clickedNew) throw new Error('Could not find "新建连接" button');
  await waitForNewConnectionDialog();

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

  // Database (optional — empty string leaves field blank for multi-db session)
  const dbInput = await $('input[placeholder="myapp_production"]');
  await dbInput.clearValue();
  if (database) {
    await dbInput.setValue(database);
  }

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
    async () => !(await $('[data-testid="new-connection-dialog"]').isExisting()),
    {
      timeout: 10000,
      timeoutMsg: '保存连接后弹窗未关闭',
    },
  );
  await browser.pause(1000);

  // Now connect by double-clicking the item
  const card = await findCardByName(name);
  if (!card) throw new Error(`未找到 MySQL 连接 "${name}"`);
  await dblclickConnByExactName(name);

  return finishConnectInWorkspace(mainWindow);
}

/**
 * Create a PostgreSQL connection via the new-connection UI and connect to it.
 * Returns { mainWindow, connWindow }.
 * Assumes we start on the main window.
 */
export async function createAndConnectPostgreSQL(
  opts: {
    name?: string;
    host?: string;
    port?: string;
    user?: string;
    password?: string;
    /** Pass empty string to leave the database field blank (multi-db session). */
    database?: string;
  } = {},
) {
  const {
    name = 'E2E-PostgreSQL',
    host = process.env.E2E_PG_HOST || '127.0.0.1',
    port = process.env.E2E_PG_PORT || '5432',
    user = process.env.E2E_PG_USER || 'postgres',
    password = process.env.E2E_PG_PASSWORD || '',
    database = process.env.E2E_PG_DB || 'postgres',
  } = opts;

  const mainWindow = await browser.getWindowHandle();

  await expandAllGroups();

  const existingItem = await findCardByName(name);
  if (existingItem) {
    await dblclickConnByExactName(name);
    return finishConnectInWorkspace(mainWindow);
  }

  await openNewConnectionDialogFromUi();

  // PostgreSQL is the default type; ensure it is selected
  const pgBtn = await $('button*=PostgreSQL');
  await pgBtn.click();
  await browser.pause(300);

  const nameInput = await $('input[placeholder="例如：主数据库"]');
  await nameInput.setValue(name);

  const hostInput = await $('input[placeholder="prod-db.example.com"]');
  await hostInput.clearValue();
  await hostInput.setValue(host);

  const allInputs = await $$('input');
  for (const inp of allInputs) {
    if ((await inp.getValue()) === '5432') {
      await inp.clearValue();
      await inp.setValue(port);
      break;
    }
  }

  const dbInput = await $('input[placeholder="myapp_production"]');
  await dbInput.clearValue();
  if (database) {
    await dbInput.setValue(database);
  }

  const userInput = await $('input[placeholder="postgres"]');
  await userInput.clearValue();
  await userInput.setValue(user);

  if (password) {
    const pwInput = await $('input[type="password"]');
    await pwInput.setValue(password);
  }

  const saveBtn = await $('button*=保存');
  await saveBtn.click();
  await browser.waitUntil(
    async () => !(await $('[data-testid="new-connection-dialog"]').isExisting()),
    {
      timeout: 10000,
      timeoutMsg: '保存连接后弹窗未关闭',
    },
  );
  await browser.pause(1000);

  const card = await findCardByName(name);
  if (!card) throw new Error(`未找到 PostgreSQL 连接 "${name}"`);
  await dblclickConnByExactName(name);

  return finishConnectInWorkspace(mainWindow);
}

/**
 * Connect to a named connection card (any DB type) from the main window.
 */
export async function connectToCard(cardName: string) {
  const mainWindow = await browser.getWindowHandle();
  await expandAllGroups();
  const card = await findCardByName(cardName);
  if (!card) throw new Error(`未找到连接 "${cardName}"`);

  await dblclickConnByExactName(cardName);

  return finishConnectInWorkspace(mainWindow);
}

// ── SQL / CodeMirror ────────────────────────────────────────────────

type SettingsLike = { safeMode?: boolean } & Record<string, unknown>;

async function invokeSettings<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
  const result = await browser.executeAsync(
    (c: string, a: string, done: (r: unknown) => void) => {
      (
        window as unknown as {
          __TAURI_INTERNALS__?: { invoke: (cmd: string, args: unknown) => Promise<unknown> };
        }
      ).__TAURI_INTERNALS__
        ?.invoke(c, JSON.parse(a))
        .then((r) => done(r))
        .catch((e: unknown) => done({ __error: String(e) }));
    },
    cmd,
    JSON.stringify(args),
  );
  if (result && typeof result === 'object' && result !== null && '__error' in result) {
    throw new Error(String((result as { __error: string }).__error));
  }
  return result as T;
}

// ── Tauri IPC / query results ───────────────────────────────────────

export type QueryResultPayload = {
  rows?: unknown[][];
  results?: Array<{ rows?: unknown[][]; columns?: Array<{ name: string } | string> }>;
  data?: unknown;
};

/** Invoke a Tauri command from the WebDriver session. */
export async function invokeBackend<T>(
  cmd: string,
  args: Record<string, unknown> = {},
): Promise<T> {
  return invokeSettings<T>(cmd, args);
}

/** Normalize `execute_query` payloads (rows array or legacy `{ data: [...] }`). */
export function parseQueryRows(payload: QueryResultPayload): unknown[][] {
  if (Array.isArray(payload.rows)) return payload.rows;
  const fromResults = payload.results?.[0]?.rows;
  if (Array.isArray(fromResults)) return fromResults;
  return [];
}

/** Read the first scalar cell from an `execute_query` response. */
export function queryScalar(payload: QueryResultPayload, field: string | number = 0): number {
  const legacy = payload.data;
  if (Array.isArray(legacy) && legacy[0] && typeof legacy[0] === 'object' && legacy[0] !== null) {
    const row = legacy[0] as Record<string, unknown>;
    if (typeof field === 'string' && field in row) return Number(row[field]);
    const values = Object.values(row);
    const idx = typeof field === 'number' ? field : 0;
    return Number(values[idx]);
  }
  const rows = parseQueryRows(payload);
  const idx = typeof field === 'number' ? field : 0;
  return Number(rows[0]?.[idx]);
}

/** DROP / TRUNCATE are blocked when Safe Mode is on (product default). */
export function sqlBlockedBySafeMode(sql: string): boolean {
  return /^\s*(DROP|TRUNCATE)\b/im.test(sql);
}

/** Temporarily disable Safe Mode for fixture DDL (DROP/TRUNCATE). Restores previous flag. */
export async function withSafeModeOff<T>(fn: () => Promise<T>): Promise<T> {
  const settings = await invokeSettings<SettingsLike>('get_settings');
  if (!settings.safeMode) return fn();
  await invokeSettings('save_settings', { settings: { ...settings, safeMode: false } });
  try {
    return await fn();
  } finally {
    const current = await invokeSettings<SettingsLike>('get_settings');
    await invokeSettings('save_settings', { settings: { ...current, safeMode: true } });
  }
}

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
  if (sqlBlockedBySafeMode(sql)) {
    return withSafeModeOff(() => executeSqlInEditor(sql));
  }
  return executeSqlInEditor(sql);
}

async function executeSqlInEditor(sql: string) {
  await setEditorContent(sql);
  // Stable E2E locator (vite-gated data-testid, see src/lib/tid.ts) — survives i18n switching.
  const execBtn = await $('[data-testid="editor-execute-button"]');
  const prevTotal = await browser.execute(() => {
    const spans = Array.from(document.querySelectorAll('span'));
    return spans.find((s) => s.textContent?.includes('总耗时'))?.textContent ?? '';
  });
  await execBtn.click();
  const started = Date.now();
  await browser.waitUntil(
    async () => {
      const elapsed = Date.now() - started;
      const body = await $('body').getText();
      // Confirm unclosed-BEGIN dialog if shown (product guard).
      if (body.includes('检测到未结束的事务') || body.includes('Unclosed transaction detected')) {
        const cont = await $('button*=继续执行');
        const contEn = await $('button*=Continue');
        if (await cont.isExisting()) await cont.click();
        else if (await contEn.isExisting()) await contEn.click();
        await browser.pause(200);
        return false;
      }
      if (
        /current transaction is aborted/i.test(body) ||
        body.includes('事务已中止') ||
        body.includes('Transaction aborted')
      ) {
        const rb = await $('button*=全部回滚');
        const rbEn = await $('button*=Roll back all');
        const rbLegacy = await $('button*=回滚');
        if ((await rb.isExisting()) && (await rb.isDisplayed().catch(() => false)))
          await rb.click();
        else if ((await rbEn.isExisting()) && (await rbEn.isDisplayed().catch(() => false)))
          await rbEn.click();
        else if ((await rbLegacy.isExisting()) && !(await rbLegacy.getAttribute('disabled')))
          await rbLegacy.click();
        await browser.pause(300);
        return true;
      }
      if (
        elapsed > 200 &&
        /Query failed|No database selected|Access denied|error returned from database/i.test(body)
      ) {
        return true;
      }
      const stop = await $('[data-testid="editor-stop-button"]');
      if ((await stop.isExisting()) && (await stop.isDisplayed().catch(() => false))) {
        return false;
      }
      const curTotal = await browser.execute(() => {
        const spans = Array.from(document.querySelectorAll('span'));
        return spans.find((s) => s.textContent?.includes('总耗时'))?.textContent ?? '';
      });
      if (curTotal && curTotal !== prevTotal) return true;
      // Fast queries can keep the same "总耗时 0 ms" label; settle after click.
      if (elapsed > 900 && curTotal && !(await execBtn.getAttribute('disabled'))) return true;
      return false;
    },
    { timeout: 15000, timeoutMsg: `等待 SQL 执行完成超时: ${sql.slice(0, 60)}` },
  );
  // Auto-chart can hide grid column headers; switch back to table when present.
  const tableView = await $('button*=表格');
  if (await tableView.isExisting()) {
    try {
      if (await tableView.isDisplayed()) await tableView.click();
    } catch {
      /* ignore */
    }
  }
  await browser.pause(500);
}

/** Open a new query tab and wait for the execute button. */
export async function openQueryTab() {
  // Stable E2E locators (vite-gated data-testid, see src/lib/tid.ts).
  let newQueryBtn = await $('[data-testid="conn-toolbar-new-query"]');
  if (!(await newQueryBtn.isExisting())) {
    // First tab: workspace-home quick action opens the first panel.
    const quickAction = await $('[data-testid="home-quick-new-query"]');
    if (await quickAction.isExisting()) {
      newQueryBtn = quickAction;
    } else {
      // fallback: find by text
      const labels = ['新建查询', 'New Query', '新查詢', 'Neue Abfrage'];
      for (const label of labels) {
        const candidate = await $(`button*=${label}`);
        if (await candidate.isExisting()) {
          newQueryBtn = candidate;
          break;
        }
      }
    }
  }
  await newQueryBtn.click();
  await browser.pause(500);
  // Wait for execute button — try testid first, then aria-label fallback.
  let execBtn = await $('[data-testid="editor-execute-button"]');
  if (!(await execBtn.isExisting())) {
    execBtn = await $('button[aria-label="执行"]');
  }
  await execBtn.waitForDisplayed({ timeout: 10000 });
}

// ── schema sidebar ──────────────────────────────────────────────────

/** Sidebar section headers indicating schema tree loaded (en + zh-CN). */
export const SCHEMA_TREE_SECTION_MARKERS = ['Tables', '表', 'Keys', '键', 'Views', '视图'] as const;

export function asideHasSchemaSections(text: string): boolean {
  return SCHEMA_TREE_SECTION_MARKERS.some((marker) => text.includes(marker));
}

/** True if text looks like a schema tree section header (e.g. "Tables (5)" or "表 (3)"). */
export function isSchemaSectionLabel(text: string): boolean {
  return SCHEMA_TREE_SECTION_MARKERS.some((m) => text.startsWith(m));
}

/** Connection navigator sidebar (not the 40px workspace mode rail). */
export async function connectionNavigatorAside() {
  const byTestId = await $('[data-testid="connection-navigator-aside"]');
  if (await byTestId.isExisting()) {
    return byTestId;
  }
  const asides = await $$('aside');
  for (const aside of asides) {
    if (await aside.$('[data-conn-item]').isExisting()) {
      return aside;
    }
  }
  for (const aside of asides) {
    const text = await aside.getText();
    if (asideHasSchemaSections(text)) {
      return aside;
    }
  }
  for (const aside of asides) {
    const { width } = await aside.getSize();
    if (width > 200) {
      return aside;
    }
  }
  throw new Error('connection navigator aside not found');
}

async function navigatorHasTableButtons(): Promise<boolean> {
  return browser.execute(() => {
    const nav =
      document.querySelector('[data-testid="connection-navigator-aside"]') ??
      Array.from(document.querySelectorAll('aside')).find((a) =>
        a.querySelector('[data-conn-item]'),
      );
    if (!nav) return false;
    return Array.from(nav.querySelectorAll('button')).some((b) => {
      const label = (b.textContent ?? '').trim();
      if (!label || label.startsWith('表') || label.startsWith('Tables')) return false;
      if (label.includes('PostgreSQL') || label.includes('本地')) return false;
      return /^[\w.-]+$/.test(label);
    });
  });
}

/** Wait until the connection navigator lists at least one table entry. */
export async function waitForSchemaTreeLoaded(timeout = 20000) {
  await browser.waitUntil(
    async () => {
      if (await navigatorHasTableButtons()) return true;
      await expandConnectedConnectionInNavigator();
      await browser.execute(() => {
        const nav =
          document.querySelector('[data-testid="connection-navigator-aside"]') ??
          Array.from(document.querySelectorAll('aside')).find((a) =>
            a.querySelector('[data-conn-item]'),
          );
        if (!nav) return;
        const dbBtn = Array.from(nav.querySelectorAll('button')).find((b) => {
          const label = (b.textContent ?? '').trim();
          return label.length > 0 && !label.startsWith('表') && !label.includes('PostgreSQL');
        });
        dbBtn?.click();
        const tablesBtn = Array.from(nav.querySelectorAll('button')).find((b) => {
          const label = (b.textContent ?? '').trim();
          return label.startsWith('表') || label.startsWith('Tables');
        });
        tablesBtn?.click();
      });
      await browser.pause(500);
      return false;
    },
    {
      timeout,
      timeoutMsg: '等待 schema 树加载超时',
    },
  );
}

/** Click a table by exact name in the sidebar. */
export async function clickTableInSidebar(tableName: string) {
  await waitForSchemaTreeLoaded();
  const aside = await connectionNavigatorAside();
  const asideButtons = await aside.$$('button');
  for (const btn of asideButtons) {
    const text = (await btn.getText()).trim();
    if (text === tableName || text.endsWith(`.${tableName}`) || text.endsWith(`/${tableName}`)) {
      await btn.click();
      return;
    }
  }
  const clicked = await browser.execute((name: string) => {
    const nav =
      document.querySelector('[data-testid="connection-navigator-aside"]') ??
      Array.from(document.querySelectorAll('aside')).find((a) =>
        a.querySelector('[data-conn-item]'),
      );
    if (!nav) return false;
    const btn = Array.from(nav.querySelectorAll('button')).find((b) => {
      const label = (b.textContent ?? '').trim();
      return label === name || label.endsWith(`.${name}`) || label.endsWith(`/${name}`);
    });
    if (!btn) return false;
    btn.click();
    return true;
  }, tableName);
  if (!clicked) {
    throw new Error(`未找到表 "${tableName}"`);
  }
}

/** Click the first table/view entry in the sidebar and return its name. */
export async function clickFirstTable() {
  await waitForSchemaTreeLoaded();
  const name = await browser.execute(() => {
    const nav =
      document.querySelector('[data-testid="connection-navigator-aside"]') ??
      Array.from(document.querySelectorAll('aside')).find((a) =>
        a.querySelector('[data-conn-item]'),
      );
    if (!nav) return null;
    for (const btn of Array.from(nav.querySelectorAll('button'))) {
      const label = (btn.textContent ?? '').trim();
      if (!label || label.startsWith('表') || label.startsWith('Tables')) continue;
      if (label.includes('PostgreSQL') || label.includes('本地')) continue;
      if (!/^[\w.-]+$/.test(label)) continue;
      btn.click();
      return label;
    }
    return null;
  });
  if (!name) {
    throw new Error('未找到可点击的表节点');
  }
  return name;
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

// ── Host Select (dz-select-listbox) ──────────────────────────────────

/** Open a Host Select by matching the trigger's visible label, then pick an option. */
export async function selectDzOption(triggerLabel: string, optionLabel: string) {
  await browser.execute(
    (trigger: string, option: string) => {
      const buttons = Array.from(document.querySelectorAll('button[aria-haspopup="listbox"]'));
      const btn = buttons.find((b) => (b.textContent || '').includes(trigger));
      if (!btn) throw new Error(`Select trigger not found: ${trigger}`);
      (btn as HTMLElement).click();
      const list = document.getElementById('dz-select-listbox');
      if (!list) throw new Error('dz-select-listbox not open');
      const item = Array.from(list.children).find((el) => (el.textContent || '').includes(option));
      if (!item) throw new Error(`Select option not found: ${option}`);
      item.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, cancelable: true }));
    },
    triggerLabel,
    optionLabel,
  );
  await browser.pause(200);
}

// ── workspace navigation ────────────────────────────────────────────

/** Click workspace left-nav and wait for target panel. */
export async function switchWorkspaceNav(
  navTestId: string,
  waitTestId: string,
  _stepLabel?: string,
) {
  const nav = await $(`[data-testid="${navTestId}"]`);
  await nav.waitForDisplayed({ timeout: 15000 });
  await nav.click();
  const target = await $(`[data-testid="${waitTestId}"]`);
  await target.waitForDisplayed({ timeout: 15000 });
}

/** Open ER diagram from home quick action or connection toolbar. */
export async function openErDiagramFromUi() {
  const homeQuick = await $('[data-testid="home-quick-er-diagram"]');
  if (await homeQuick.isExisting()) {
    await homeQuick.click();
    return;
  }
  const toolbarWrap = await $('[data-testid="content-toolbar-er-diagram"]');
  const toolbarBtn = await toolbarWrap.$('button');
  await toolbarBtn.waitForClickable({ timeout: 10000 });
  await toolbarBtn.click();
}

/** Emit a cross-window menu event on the main Tauri window. */
export async function emitCrossWindowEvent(event: string, payload?: Record<string, unknown>) {
  await browser.execute(
    (evt: string, pl: Record<string, unknown> | null) => {
      (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
        ?.invoke?.('plugin:event|emit', {
          event: evt,
          payload: pl ?? {},
        })
        .catch(() => {});
    },
    event,
    payload ?? null,
  );
  await browser.pause(300);
}

/** Open SettingsPage inside the main window (F1; replaces legacy settings sub-window URL). */
export async function openSettingsInMainWindow(section?: string) {
  await browser.url('tauri://localhost');
  await browser.waitUntil(
    async () => {
      const nav = await $('[data-testid="workspace-nav-connections"]');
      return nav.isDisplayed().catch(() => false);
    },
    { timeout: 20000, timeoutMsg: 'Main window workspace nav not ready' },
  );
  await emitCrossWindowEvent('menu:open-settings', section ? { section } : undefined);
  const settingsPage = await $('[data-testid="settings-page"]');
  await settingsPage.waitForDisplayed({ timeout: 15000 });
}

/** Click SettingsPage back control and wait for workspace shell. */
export async function backFromSettingsInMainWindow() {
  const backBtn = await $('[data-testid="settings-back"]');
  await backBtn.waitForDisplayed({ timeout: 8000 });
  await backBtn.click();
  await browser.pause(400);
  const settingsPage = await $('[data-testid="settings-page"]');
  await browser.waitUntil(async () => !(await settingsPage.isDisplayed().catch(() => false)), {
    timeout: 10000,
    timeoutMsg: 'SettingsPage did not close after back',
  });
  await $('[data-testid="workspace-nav-connections"]').waitForDisplayed({ timeout: 10000 });
}
