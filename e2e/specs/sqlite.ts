import { expect, browser, $, $$ } from '@wdio/globals';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  closeExtraWindows,
  switchToNewWindow,
  findCardByName,
  expandAllGroups,
  openQueryTab,
  executeSQL,
  clickTableInSidebar,
  switchSubTab,
  asideHasSchemaSections,
} from '../helpers.js';

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const CONN_NAME = 'E2E-SQLite';
const DB_PATH = path.resolve(__dirname, '../fixtures/test.db');

// Helper to create and connect to SQLite
async function createAndConnectSQLite() {
  const mainWindow = await browser.getWindowHandle();
  await expandAllGroups();

  // Check if already exists
  const existingItem = await findCardByName(CONN_NAME);
  if (existingItem) {
    // Double-click to connect
    await browser.execute((n: string) => {
      const items = document.querySelectorAll('[data-conn-item]');
      for (const item of items) {
        if (item.textContent?.includes(n)) {
          item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
          return;
        }
      }
    }, CONN_NAME);
    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 30000 },
    );
    const handles = await browser.getWindowHandles();
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);
    await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
    await browser.pause(2000);
    return { mainWindow, connWindow };
  }

  // Create new connection
  const newConnBtn = await $('button*=新建连接');
  await newConnBtn.click();
  await switchToNewWindow(mainWindow);

  // Select SQLite type
  const sqliteBtn = await $('button*=SQLite');
  await sqliteBtn.click();
  await browser.pause(300);

  // Fill name
  const nameInput = await $('input[placeholder="例如：主数据库"]');
  await nameInput.setValue(CONN_NAME);

  // Fill database file path
  const dbInput = await $('input[placeholder="/path/to/db.sqlite"]');
  await dbInput.setValue(DB_PATH);

  // Test connection
  const testBtn = await $('button*=测试连接');
  await testBtn.click();
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return body.includes('连接成功') || body.includes('text-red-400');
    },
    { timeout: 15000 },
  );
  await browser.pause(500);

  // Save
  const saveBtn = await $('button*=保存');
  await saveBtn.click();
  await browser.waitUntil(
    async () => (await browser.getWindowHandles()).length === 1,
    { timeout: 10000 },
  );
  await browser.switchToWindow(mainWindow);
  await browser.pause(1000);

  // Connect
  const card = await findCardByName(CONN_NAME);
  if (!card) throw new Error(`未找到 SQLite 连接 "${CONN_NAME}"`);
  await browser.execute((n: string) => {
    const items = document.querySelectorAll('[data-conn-item]');
    for (const item of items) {
      if (item.textContent?.includes(n)) {
        item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true, cancelable: true }));
        return;
      }
    }
  }, CONN_NAME);

  await browser.waitUntil(
    async () => (await browser.getWindowHandles()).length > 1,
    { timeout: 30000 },
  );
  const handles = await browser.getWindowHandles();
  const connWindow = handles.find((h) => h !== mainWindow)!;
  await browser.switchToWindow(connWindow);
  await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
  await browser.pause(2000);
  return { mainWindow, connWindow };
}

describe('SQLite', () => {
  let mainWindow: string;
  let connWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);
    const result = await createAndConnectSQLite();
    mainWindow = result.mainWindow;
    connWindow = result.connWindow;
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('should show tables in sidebar', async () => {
    const aside = await $('aside');
    const text = await aside.getText();
    expect(asideHasSchemaSections(text)).toBe(true);
    expect(text).toContain('users');
    expect(text).toContain('posts');
    expect(text).toContain('tags');
  });

  it('should show views in sidebar', async () => {
    const aside = await $('aside');
    const text = await aside.getText();
    expect(text).toContain('Views');
    expect(text).toContain('published_posts');
  });

  it('should display table data', async () => {
    await clickTableInSidebar('users');
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(body).toContain('name');
    expect(body).toContain('email');
  });

  it('should show table structure', async () => {
    await clickTableInSidebar('users');
    await browser.pause(500);
    await switchSubTab('结构');
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(body).toContain('id');
    expect(body).toContain('INTEGER');
  });

  it('should execute SQL query', async () => {
    await openQueryTab();
    await executeSQL('SELECT * FROM users WHERE age > 20');
    const body = await $('body').getText();
    expect(body).toContain('name');
  });

  it('should execute multiple statements', async () => {
    await openQueryTab();
    await executeSQL('SELECT COUNT(*) FROM users;\nSELECT COUNT(*) FROM posts');
    const body = await $('body').getText();
    // Should show "结果 1" and "结果 2" tabs
    expect(body).toContain('结果 1');
  });

  it('should show indexes', async () => {
    await clickTableInSidebar('users');
    await browser.pause(500);
    await switchSubTab('索引');
    await browser.pause(1000);
    // SQLite creates autoindex for UNIQUE constraints
    const body = await $('body').getText();
    expect(body).toContain('email');
  });

  it('should display view data', async () => {
    // Click the published_posts view
    const asideButtons = await $$('aside button');
    for (const btn of asideButtons) {
      const text = (await btn.getText()).trim();
      if (text === 'published_posts') {
        await btn.click();
        break;
      }
    }
    await browser.pause(1000);
    const body = await $('body').getText();
    expect(body).toContain('author');
  });
});
