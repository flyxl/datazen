import { expect, browser, $, $$ } from '@wdio/globals';
import {
  closeExtraWindows,
  switchToNewWindow,
  findCardByName,
  expandAllGroups,
  createAndConnectKiwi,
  setEditorContent,
  executeSQL,
  openQueryTab,
  clickFirstTable,
  switchSubTab,
  asideHasSchemaSections,
  waitForSchemaTreeLoaded,
  isSchemaSectionLabel,
} from '../helpers.js';

const CONN_NAME = 'E2E-Kiwi';
const KIWI_URL = process.env.E2E_KIWI_URL || 'https://kiwi.akusre.com';
const KIWI_TOKEN = process.env.E2E_KIWI_TOKEN || '';
const KIWI_DOMAIN = process.env.E2E_KIWI_DOMAIN || '';

function skipIfNoCredentials() {
  if (!KIWI_TOKEN || !KIWI_DOMAIN) {
    console.warn('⏩ Skipping Kiwi E2E tests: E2E_KIWI_TOKEN / E2E_KIWI_DOMAIN not set');
    return true;
  }
  return false;
}

// ═════════════════════════════════════════════════════════════════════
// Part 1: Connection Form UI (no credentials needed)
// ═════════════════════════════════════════════════════════════════════

describe('Kiwi 连接表单 (KW-001~KW-006)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('切换到 Kiwi 类型应显示专属表单字段 (KW-001)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const kiwiBtn = await $('button*=Kiwi');
    await expect(kiwiBtn).toBeExisting();
    await kiwiBtn.click();
    await browser.pause(300);

    // Should show Kiwi URL field
    const urlInput = await $('input[placeholder="https://kiwi.akusre.com"]');
    await expect(urlInput).toBeDisplayed();

    // Should show Token field
    const tokenInput = await $('input[type="password"]');
    await expect(tokenInput).toBeDisplayed();

    // Should show SSO login button
    const ssoBtn = await $('button*=SSO 登录');
    await expect(ssoBtn).toBeDisplayed();

    // Standard host/port fields should NOT be shown
    const hostInput = await $('input[placeholder="prod-db.example.com"]');
    await expect(hostInput).not.toBeExisting();
  });

  it('Kiwi URL 默认值应为 https://kiwi.akusre.com (KW-002)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const kiwiBtn = await $('button*=Kiwi');
    await kiwiBtn.click();
    await browser.pause(300);

    const urlInput = await $('input[placeholder="https://kiwi.akusre.com"]');
    const value = await urlInput.getValue();
    expect(value).toBe('https://kiwi.akusre.com');
  });

  it('填入 Token 后应显示 Instance Domain 和 Username 字段 (KW-003)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const kiwiBtn = await $('button*=Kiwi');
    await kiwiBtn.click();
    await browser.pause(300);

    // Before token: domain/username fields should not be visible
    let domainInput = await $('input[placeholder*="rwlb"]');
    await expect(domainInput).not.toBeExisting();

    // Enter a token
    const tokenInput = await $('input[type="password"]');
    await tokenInput.setValue('fake-token-for-ui-test-12345678901234567890');
    await browser.pause(600);

    // After token: domain and username fields should appear
    domainInput = await $('input[placeholder*="rwlb"]');
    await expect(domainInput).toBeDisplayed();

    const usernameInput = await $('input[placeholder="wuxl"]');
    await expect(usernameInput).toBeDisplayed();
  });

  it('"加载实例" 按钮应在填入 Token 后显示 (KW-004)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const kiwiBtn = await $('button*=Kiwi');
    await kiwiBtn.click();
    await browser.pause(300);

    const tokenInput = await $('input[type="password"]');
    await tokenInput.setValue('fake-token-for-ui-test-12345678901234567890');
    await browser.pause(600);

    const loadBtn = await $('button*=加载实例');
    await expect(loadBtn).toBeDisplayed();
  });

  it('Source Type 字段默认值应为 4 (KW-005)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const kiwiBtn = await $('button*=Kiwi');
    await kiwiBtn.click();
    await browser.pause(300);

    const tokenInput = await $('input[type="password"]');
    await tokenInput.setValue('fake-token-for-ui-test-12345678901234567890');
    await browser.pause(600);

    const allInputs = await $$('input');
    let sourceTypeFound = false;
    for (const inp of allInputs) {
      const val = await inp.getValue();
      const ph = await inp.getAttribute('placeholder');
      if (val === '4' && ph === '4') {
        sourceTypeFound = true;
        break;
      }
    }
    expect(sourceTypeFound).toBe(true);
  });

  it('从 Kiwi 切换回 PostgreSQL 应恢复标准表单 (KW-006)', async () => {
    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const kiwiBtn = await $('button*=Kiwi');
    await kiwiBtn.click();
    await browser.pause(300);

    // Verify Kiwi form is shown
    const urlInput = await $('input[placeholder="https://kiwi.akusre.com"]');
    await expect(urlInput).toBeDisplayed();

    // Switch back to PostgreSQL
    const pgBtn = await $('button*=PostgreSQL');
    await pgBtn.click();
    await browser.pause(300);

    // Standard host/port should reappear
    const hostInput = await $('input[placeholder="prod-db.example.com"]');
    await expect(hostInput).toBeDisplayed();

    // Kiwi-specific fields should disappear
    const urlInputAfter = await $('input[placeholder="https://kiwi.akusre.com"]');
    await expect(urlInputAfter).not.toBeExisting();
  });
});

// ═════════════════════════════════════════════════════════════════════
// Part 2: Connection + Query (requires real credentials)
// ═════════════════════════════════════════════════════════════════════

describe('Kiwi 连接与查询 (KW-010~KW-025)', () => {
  let mainWindow: string;
  const shouldSkip = skipIfNoCredentials();

  before(async function () {
    if (shouldSkip) return this.skip();

    const handles = await browser.getWindowHandles();
    mainWindow = handles.find((h) => h === 'main') ?? handles[0];
    await browser.switchToWindow(mainWindow);
    await closeExtraWindows(mainWindow);
    await browser.pause(1000);

    const result = await createAndConnectKiwi({
      name: CONN_NAME,
      baseUrl: KIWI_URL,
      token: KIWI_TOKEN,
      domain: KIWI_DOMAIN,
    });
    mainWindow = result.mainWindow;
  });

  after(async () => {
    if (shouldSkip) return;
    try {
      await closeExtraWindows(mainWindow);
    } catch { /* ignore */ }
  });

  afterEach(async () => {
    if (shouldSkip) return;
    // Ensure we are still on the connection window (not main)
    const handles = await browser.getWindowHandles();
    const connHandle = handles.find((h) => h !== mainWindow);
    if (connHandle) {
      await browser.switchToWindow(connHandle);
    }
  });

  // ── Connection Window Layout ──

  it('Kiwi 连接窗口应显示 SQL 查询界面 (KW-010)', async function () {
    if (shouldSkip) return this.skip();
    const body = await $('body').getText();
    expect(body).toContain('新建查询');
  });

  it('标题栏应包含连接名称和 Kiwi 标识 (KW-011)', async function () {
    if (shouldSkip) return this.skip();
    const body = await $('body').getText();
    expect(body).toContain(CONN_NAME);
  });

  // ── Database Sidebar ──

  it('左侧边栏应显示数据库列表 (KW-012)', async function () {
    if (shouldSkip) return this.skip();
    const aside = await $('aside');
    await aside.waitForDisplayed({ timeout: 10000 });
    const asideText = await aside.getText();
    // Kiwi should list at least one database
    expect(asideText.length).toBeGreaterThan(0);
  });

  it('点击数据库应展开表列表 (KW-013)', async function () {
    if (shouldSkip) return this.skip();
    // Click the first database in sidebar
    const dbButtons = await $$('aside button');
    let clicked = false;
    for (const btn of dbButtons) {
      const text = (await btn.getText()).trim();
      if (text.length > 0 && !isSchemaSectionLabel(text)) {
        await btn.click();
        clicked = true;
        break;
      }
    }
    expect(clicked).toBe(true);

    await waitForSchemaTreeLoaded(15000);
    const asideText = await $('aside').getText();
    expect(asideHasSchemaSections(asideText)).toBe(true);
  });

  it('表列表中应包含至少一张表 (KW-014)', async function () {
    if (shouldSkip) return this.skip();
    await waitForSchemaTreeLoaded();

    const tableName = await clickFirstTable();
    expect(tableName).not.toBeNull();
    expect(tableName!.length).toBeGreaterThan(0);
  });

  // ── Table Structure ──

  it('点击表后应显示数据/结构等子标签 (KW-015)', async function () {
    if (shouldSkip) return this.skip();
    const body = await $('body').getText();
    const hasStructureTab = body.includes('结构') || body.includes('Structure');
    expect(hasStructureTab).toBe(true);
  });

  it('结构标签应显示列信息（名称、类型等） (KW-016)', async function () {
    if (shouldSkip) return this.skip();
    await switchSubTab('结构');
    await browser.pause(2000);

    const body = await $('body').getText();
    // Structure view should contain column type keywords
    const hasColumnInfo =
      body.includes('varchar') ||
      body.includes('int') ||
      body.includes('bigint') ||
      body.includes('text') ||
      body.includes('datetime') ||
      body.includes('VARCHAR') ||
      body.includes('INT');
    expect(hasColumnInfo).toBe(true);
  });

  // ── SQL Query Execution ──

  it('新建查询标签应显示 CodeMirror 编辑器 (KW-017)', async function () {
    if (shouldSkip) return this.skip();
    await openQueryTab();
    const editor = await $('.cm-editor');
    await expect(editor).toBeDisplayed();
  });

  it('应能执行 SELECT 1 查询 (KW-018)', async function () {
    if (shouldSkip) return this.skip();
    await openQueryTab();
    await executeSQL('SELECT 1');
    const body = await $('body').getText();
    // Should show result with "1" and execution time
    expect(body).toContain('1');
    expect(body).toContain('总耗时');
  });

  it('应能执行 SHOW DATABASES 查询 (KW-019)', async function () {
    if (shouldSkip) return this.skip();
    await openQueryTab();
    await executeSQL('SHOW DATABASES');
    const body = await $('body').getText();
    // Should list databases — at least "Database" header or some db name
    const hasDbOutput =
      body.includes('Database') ||
      body.includes('information_schema') ||
      body.includes('mysql');
    expect(hasDbOutput).toBe(true);
  });

  it('应能执行 SHOW TABLES 查询 (KW-020)', async function () {
    if (shouldSkip) return this.skip();
    await openQueryTab();
    await executeSQL('SHOW TABLES');
    const body = await $('body').getText();
    expect(body).toContain('总耗时');
  });

  it('应能执行带 WHERE 条件的 SELECT (KW-021)', async function () {
    if (shouldSkip) return this.skip();
    await openQueryTab();
    await executeSQL("SELECT 'kiwi_e2e_test' AS marker, 42 AS answer");
    const body = await $('body').getText();
    expect(body).toContain('kiwi_e2e_test');
    expect(body).toContain('42');
  });

  it('执行非法 SQL 应显示错误信息 (KW-022)', async function () {
    if (shouldSkip) return this.skip();
    await openQueryTab();
    await setEditorContent('SELEC INVALID SYNTAX');
    const execBtn = await $('button*=执行');
    await execBtn.click();
    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('error') || body.includes('Error') || body.includes('错误') || body.includes('text-red');
      },
      { timeout: 15000, timeoutMsg: '等待错误提示超时' },
    );
    const body = await $('body').getText();
    const hasError =
      body.includes('error') ||
      body.includes('Error') ||
      body.includes('syntax') ||
      body.includes('错误');
    expect(hasError).toBe(true);
  });

  it('应能执行多条 SQL 语句 (KW-023)', async function () {
    if (shouldSkip) return this.skip();
    await openQueryTab();
    await executeSQL("SELECT 'stmt1' AS tag; SELECT 'stmt2' AS tag");
    const body = await $('body').getText();
    expect(body).toContain('stmt1');
    expect(body).toContain('stmt2');
  });

  it('应能执行 SHOW CREATE TABLE 获取建表语句 (KW-024)', async function () {
    if (shouldSkip) return this.skip();
    // First get a real table name from the sidebar
    const aside = await $('aside');
    const asideText = await aside.getText();
    // Try to extract the first table name from sidebar
    const tableButtons = await $$('aside button');
    let tableName = '';
    for (const btn of tableButtons) {
      const cls = (await btn.getAttribute('class')) || '';
      const text = (await btn.getText()).trim();
      if (cls.includes('text-left') && cls.includes('13px') && text.length > 0) {
        tableName = text;
        break;
      }
    }
    if (!tableName) {
      console.warn('No table found in sidebar, skipping SHOW CREATE TABLE test');
      return;
    }

    await openQueryTab();
    await executeSQL(`SHOW CREATE TABLE \`${tableName}\``);
    const body = await $('body').getText();
    const hasCreate =
      body.includes('CREATE TABLE') || body.includes('Create Table');
    expect(hasCreate).toBe(true);
  });

  it('查询结果应显示列名和数据行 (KW-025)', async function () {
    if (shouldSkip) return this.skip();
    await openQueryTab();
    await executeSQL("SELECT 'Alice' AS name, 30 AS age UNION SELECT 'Bob', 25");
    const body = await $('body').getText();
    expect(body).toContain('name');
    expect(body).toContain('age');
    expect(body).toContain('Alice');
    expect(body).toContain('Bob');
  });
});

// ═════════════════════════════════════════════════════════════════════
// Part 3: Connection management (create, edit, delete)
// ═════════════════════════════════════════════════════════════════════

describe('Kiwi 连接管理 (KW-030~KW-034)', () => {
  let mainWindow: string;
  const TEST_CONN = 'E2E-Kiwi-Temp';
  const shouldSkip = skipIfNoCredentials();

  before(async function () {
    if (shouldSkip) return this.skip();
    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);
  });

  afterEach(async () => {
    if (shouldSkip) return;
    await closeExtraWindows(mainWindow);
    await browser.pause(300);
  });

  after(async () => {
    if (shouldSkip) return;
    // Clean up test connection
    await browser.switchToWindow(mainWindow);
    const conns: any[] = await browser.executeAsync((done: (r: any) => void) => {
      (window as any).__TAURI_INTERNALS__
        .invoke('get_connections')
        .then((r: any) => done(r))
        .catch(() => done([]));
    });
    for (const c of conns) {
      if (c.name === TEST_CONN) {
        await browser.executeAsync((id: string, done: (r: any) => void) => {
          (window as any).__TAURI_INTERNALS__
            .invoke('delete_connection', { id })
            .then(() => done(null))
            .catch(() => done(null));
        }, c.id);
      }
    }
    await browser.pause(300);
  });

  it('应能保存 Kiwi 连接并在主窗口显示 (KW-030)', async function () {
    if (shouldSkip) return this.skip();

    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const kiwiBtn = await $('button*=Kiwi');
    await kiwiBtn.click();
    await browser.pause(300);

    const nameInput = await $('input[placeholder="例如：主数据库"]');
    await nameInput.setValue(TEST_CONN);

    const urlInput = await $('input[placeholder="https://kiwi.akusre.com"]');
    await urlInput.clearValue();
    await urlInput.setValue(KIWI_URL);

    const tokenInput = await $('input[type="password"]');
    await tokenInput.setValue(KIWI_TOKEN);
    await browser.pause(500);

    const domainInput = await $('input[placeholder*="rwlb"]');
    if (await domainInput.isExisting()) {
      await domainInput.clearValue();
      await domainInput.setValue(KIWI_DOMAIN);
    }

    const saveBtn = await $('button*=保存');
    await saveBtn.click();

    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length === 1,
      { timeout: 10000, timeoutMsg: '保存 Kiwi 连接后窗口未关闭' },
    );

    await browser.switchToWindow(mainWindow);
    await expandAllGroups();
    await browser.pause(500);

    const card = await findCardByName(TEST_CONN);
    expect(card).not.toBeNull();
  });

  it('保存的 Kiwi 连接应显示 Kiwi 类型标识 (KW-031)', async function () {
    if (shouldSkip) return this.skip();

    await expandAllGroups();
    const card = await findCardByName(TEST_CONN);
    if (!card) {
      console.warn('Test connection not found, skipping');
      return;
    }
    const cardText = await card.getText();
    expect(cardText).toContain('Ki');
  });

  it('测试连接应返回成功 (KW-032)', async function () {
    if (shouldSkip) return this.skip();

    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const kiwiBtn = await $('button*=Kiwi');
    await kiwiBtn.click();
    await browser.pause(300);

    const urlInput = await $('input[placeholder="https://kiwi.akusre.com"]');
    await urlInput.clearValue();
    await urlInput.setValue(KIWI_URL);

    const tokenInput = await $('input[type="password"]');
    await tokenInput.setValue(KIWI_TOKEN);
    await browser.pause(500);

    const domainInput = await $('input[placeholder*="rwlb"]');
    if (await domainInput.isExisting()) {
      await domainInput.clearValue();
      await domainInput.setValue(KIWI_DOMAIN);
    }

    const testBtn = await $('button*=测试连接');
    await testBtn.click();
    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('连接成功') || body.includes('Error');
      },
      { timeout: 20000, timeoutMsg: '等待 Kiwi 测试连接结果超时' },
    );

    const body = await $('body').getText();
    expect(body).toContain('连接成功');
  });

  it('无效 Token 测试连接应报错 (KW-033)', async function () {
    if (shouldSkip) return this.skip();

    const btn = await $('button*=新建连接');
    await btn.click();
    await switchToNewWindow(mainWindow);

    const kiwiBtn = await $('button*=Kiwi');
    await kiwiBtn.click();
    await browser.pause(300);

    const urlInput = await $('input[placeholder="https://kiwi.akusre.com"]');
    await urlInput.clearValue();
    await urlInput.setValue(KIWI_URL);

    const tokenInput = await $('input[type="password"]');
    await tokenInput.setValue('invalid-token-that-should-definitely-fail-auth');
    await browser.pause(500);

    const domainInput = await $('input[placeholder*="rwlb"]');
    if (await domainInput.isExisting()) {
      await domainInput.clearValue();
      await domainInput.setValue(KIWI_DOMAIN);
    }

    const testBtn = await $('button*=测试连接');
    await testBtn.click();
    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return body.includes('连接成功') || body.includes('Error') || body.includes('error') || body.includes('失败');
      },
      { timeout: 20000, timeoutMsg: '等待 Kiwi 错误测试连接结果超时' },
    );

    const body = await $('body').getText();
    const hasError =
      body.includes('Error') ||
      body.includes('error') ||
      body.includes('失败') ||
      body.includes('Authentication');
    expect(hasError).toBe(true);
  });

  it('应能删除 Kiwi 连接 (KW-034)', async function () {
    if (shouldSkip) return this.skip();

    await expandAllGroups();
    const card = await findCardByName(TEST_CONN);
    if (!card) {
      console.warn('Test connection not found, skipping delete test');
      return;
    }

    // Right-click to open context menu
    await browser.execute((n: string) => {
      const items = document.querySelectorAll('[data-conn-item]');
      for (const item of items) {
        if (item.textContent?.includes(n)) {
          item.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, cancelable: true }));
          return;
        }
      }
    }, TEST_CONN);
    await browser.pause(500);

    // Click delete option in context menu
    const deleteOption = await $('div*=删除');
    if (await deleteOption.isExisting()) {
      await deleteOption.click();
      await browser.pause(500);

      // Confirm deletion
      const confirmBtn = await $('button*=删除');
      if (await confirmBtn.isExisting()) {
        await confirmBtn.click();
        await browser.pause(1000);
      }
    }

    // Verify the connection is gone
    await expandAllGroups();
    const cardAfter = await findCardByName(TEST_CONN);
    expect(cardAfter).toBeNull();
  });
});
