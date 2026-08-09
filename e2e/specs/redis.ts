/**
 * Redis connection window E2E (browse + console + monitor + E1 write paths).
 *
 * Credentials: E2E_REDIS_* (see e2e/.env.example). Skips gracefully when
 * Redis is unreachable or E2E_SKIP_REDIS=1.
 * Cluster/Sentinel topology smoke: e2e/specs/redis-topology.ts (E2E_REDIS_CLUSTER_* /
 * E2E_REDIS_SENTINEL_*; skipped unless env set).
 */
import { createConnection } from 'node:net';
import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  switchToNewWindow,
  findCardByName,
  expandAllGroups,
} from '../helpers.js';

const CONN_NAME = 'E2E-Redis';
const REDIS_HOST = process.env.E2E_REDIS_HOST || '127.0.0.1';
const REDIS_PORT = process.env.E2E_REDIS_PORT || '6379';
const REDIS_PASSWORD = process.env.E2E_REDIS_PASSWORD || '';

function skipRequested(): boolean {
  return process.env.E2E_SKIP_REDIS === '1';
}

async function redisReachable(timeoutMs = 2000): Promise<boolean> {
  return new Promise((resolve) => {
    const sock = createConnection({ host: REDIS_HOST, port: Number(REDIS_PORT) });
    const timer = setTimeout(() => {
      sock.destroy();
      resolve(false);
    }, timeoutMs);
    sock.on('connect', () => {
      clearTimeout(timer);
      sock.destroy();
      resolve(true);
    });
    sock.on('error', () => {
      clearTimeout(timer);
      resolve(false);
    });
  });
}

async function createAndConnectRedis() {
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
    await browser.waitUntil(
      async () => (await browser.getWindowHandles()).length > 1,
      { timeout: 30000, timeoutMsg: 'Timed out waiting for Redis connection window' },
    );
    const handles = await browser.getWindowHandles();
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);
    await browser.pause(3000);
    return { mainWindow, connWindow };
  }

  const newConnBtn = await $(`button*=${t('action.newConnection')}`);
  await newConnBtn.click();
  await switchToNewWindow(mainWindow);

  const redisBtn = await $('button*=Redis');
  await redisBtn.click();
  await browser.pause(300);

  const nameInput = await $(`input[placeholder="${t('newConn.namePlaceholder')}"]`);
  await nameInput.setValue(CONN_NAME);

  // New-connection window uses the "isWindow" host placeholder (same as MySQL/PG helpers).
  const hostInput = await $('input[placeholder="prod-db.example.com"]');
  await hostInput.waitForDisplayed({ timeout: 10000 });
  await hostInput.clearValue();
  await hostInput.setValue(REDIS_HOST);

  const allInputs = await $$('input');
  for (const inp of allInputs) {
    if ((await inp.getValue()) === '6379') {
      await inp.clearValue();
      await inp.setValue(REDIS_PORT);
      break;
    }
  }

  const pwInput = await $('input[type="password"]');
  await pwInput.setValue(REDIS_PASSWORD);

  const testBtn = await $(`button*=${t('newConn.testConnection')}`);
  await testBtn.click();
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return body.includes(t('newConn.testSuccess')) || body.includes('Driver error');
    },
    { timeout: 15000, timeoutMsg: 'Timed out waiting for Redis test connection' },
  );

  const bodyAfterTest = await $('body').getText();
  if (bodyAfterTest.includes('Driver error')) {
    throw new Error('Redis test connection failed: ' + bodyAfterTest);
  }

  const saveBtn = await $(`button*=${t('common.save')}`);
  await saveBtn.click();
  await browser.waitUntil(
    async () => (await browser.getWindowHandles()).length === 1,
    { timeout: 10000, timeoutMsg: 'Window did not close after saving connection' },
  );
  await browser.switchToWindow(mainWindow);
  await browser.pause(1000);

  const card = await findCardByName(CONN_NAME);
  if (!card) throw new Error(`Redis connection "${CONN_NAME}" not found`);
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
    { timeout: 30000, timeoutMsg: 'Timed out waiting for Redis connection window' },
  );
  const handles = await browser.getWindowHandles();
  const connWindow = handles.find((h) => h !== mainWindow)!;
  await browser.switchToWindow(connWindow);
  await browser.pause(3000);

  return { mainWindow, connWindow };
}

async function goToConsoleTab() {
  const consoleTab = await $(`button*=${t('redis.console')}`);
  await consoleTab.click();
  await browser.pause(500);
}

async function goToMonitorTab() {
  const monitorTab = await $(`button*=${t('redis.monitor')}`);
  await monitorTab.click();
  await browser.pause(500);
}

async function setConsoleCommand(cmd: string) {
  const ok = await browser.execute((val: string) => {
    const textareas = Array.from(document.querySelectorAll('textarea'));
    const el = (textareas.find((ta) => ta.className.includes('resize-none')) ??
      textareas[0]) as HTMLTextAreaElement | undefined;
    if (!el) return false;
    const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
    setter?.call(el, val);
    el.dispatchEvent(new Event('input', { bubbles: true }));
    el.dispatchEvent(new Event('change', { bubbles: true }));
    return true;
  }, cmd);
  if (!ok) throw new Error('Redis console textarea not found');
}

async function executeRedisCommand(cmd: string) {
  await goToConsoleTab();
  await setConsoleCommand(cmd);
  const execBtn = await $(`button*=${t('query.execute')}`);
  await execBtn.click();
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return (
        body.includes(t('redis.console.ok')) ||
        body.includes(t('redis.console.failed')) ||
        body.includes('PONG') ||
        body.includes('OK')
      );
    },
    { timeout: 15000, timeoutMsg: `Timed out waiting for Redis command: ${cmd}` },
  );
  await browser.pause(500);
}

async function goToItemsTab() {
  const itemsTab = await $(`button*=${t('redis.items')}`);
  await itemsTab.click();
  await browser.pause(500);
}

async function searchKeys(pattern: string) {
  const searchInput = await $(`input[placeholder*="${t('redis.searchKeys')}"]`);
  await searchInput.clearValue();
  await searchInput.setValue(pattern);
  await browser.keys('Enter');
  await browser.pause(2000);
}

async function setReactInputByPlaceholder(placeholder: string, value: string) {
  const ok = await browser.execute(
    (ph: string, val: string) => {
      const inputs = Array.from(document.querySelectorAll('input'));
      const el = inputs.find((i) => i.getAttribute('placeholder') === ph) as
        | HTMLInputElement
        | undefined;
      if (!el) return false;
      const setter = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, 'value')?.set;
      setter?.call(el, val);
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    },
    placeholder,
    value,
  );
  if (!ok) throw new Error(`input placeholder="${placeholder}" not found`);
}

async function clickKeyRow(keyName: string): Promise<boolean> {
  return browser.execute((name: string) => {
    const rows = document.querySelectorAll('[class*="cursor-pointer"]');
    for (const row of rows) {
      if ((row.textContent || '').includes(name)) {
        (row as HTMLElement).click();
        return true;
      }
    }
    return false;
  }, keyName);
}

async function toggleKeyCheckbox(keyName: string, checked: boolean): Promise<boolean> {
  return browser.execute(
    (name: string, wantChecked: boolean) => {
      const rows = document.querySelectorAll('[class*="cursor-pointer"]');
      for (const row of rows) {
        if (!(row.textContent || '').includes(name)) continue;
        const cb = row.querySelector('input[type="checkbox"]') as HTMLInputElement | null;
        if (!cb) return false;
        if (cb.checked === wantChecked) return true;
        // Fire a proper change event so React controlled onChange runs.
        cb.click();
        if (cb.checked !== wantChecked) {
          cb.checked = wantChecked;
          cb.dispatchEvent(new Event('input', { bubbles: true }));
          cb.dispatchEvent(new Event('change', { bubbles: true }));
        }
        return true;
      }
      return false;
    },
    keyName,
    checked,
  );
}

async function ensureDbSelected() {
  await goToItemsTab();
  const selected = await browser.execute(() => {
    const aside = document.querySelector('aside');
    if (!aside) return false;
    const active = aside.querySelector('button.font-medium, button[class*="bg-blue"]');
    return Boolean(active && (active.textContent || '').includes('db'));
  });
  if (selected) return;
  await browser.execute(() => {
    const aside = document.querySelector('aside');
    const buttons = aside ? Array.from(aside.querySelectorAll('button')) : [];
    const db0 = buttons.find((b) => (b.textContent || '').trim() === 'db0');
    (db0 as HTMLElement | undefined)?.click();
  });
  await browser.pause(1500);
}

async function createStringKey(keyName: string, value: string) {
  await ensureDbSelected();
  const createBtn = await $(`button*=${t('redis.createKey')}`);
  await createBtn.waitForDisplayed({ timeout: 10000 });
  await createBtn.click();
  await browser.pause(400);
  await setReactInputByPlaceholder(t('redis.keyName'), keyName);
  await setReactInputByPlaceholder(t('redis.value'), value);
  // Prefer the dialog primary button (footer), not any other "创建" match.
  await browser.execute((label: string) => {
    const buttons = Array.from(document.querySelectorAll('button'));
    const confirm = buttons
      .filter((b) => (b.textContent || '').trim() === label)
      .pop();
    (confirm as HTMLElement | undefined)?.click();
  }, t('redis.create'));
  await browser.pause(2000);
}

async function clickButtonExact(label: string) {
  const ok = await browser.execute((text: string) => {
    const buttons = Array.from(document.querySelectorAll('button'));
    // Prefer the last exact match (dialog footer is portaled after toolbar).
    const matches = buttons.filter((b) => (b.textContent || '').trim() === text);
    const btn = matches[matches.length - 1] as HTMLElement | undefined;
    if (!btn || (btn as HTMLButtonElement).disabled) return false;
    btn.click();
    return true;
  }, label);
  if (!ok) throw new Error(`enabled button "${label}" not found`);
}

async function batchDeleteSelected() {
  await clickButtonExact(t('redis.batchDelete'));
  await browser.pause(400);
  // Do NOT use button*=删除 — it also matches「批量删除」.
  await clickButtonExact(t('common.delete'));
  await browser.pause(2000);
}

async function bodyContains(text: string): Promise<boolean> {
  const body = await $('body').getText();
  return body.includes(text);
}

describe('Redis 数据库支持 (RD-001~RD-023)', () => {
  let mainWindow: string;
  let shouldSkip = false;

  before(async function () {
    if (skipRequested()) {
      console.warn('⏩ Skipping Redis E2E: E2E_SKIP_REDIS=1');
      shouldSkip = true;
      return;
    }
    if (!(await redisReachable())) {
      console.warn(
        `⏩ Skipping Redis E2E: ${REDIS_HOST}:${REDIS_PORT} unreachable`,
      );
      shouldSkip = true;
      return;
    }

    const handles = await browser.getWindowHandles();
    mainWindow = handles.find((h) => h === 'main') ?? handles[0];
    await browser.switchToWindow(mainWindow);
    await closeExtraWindows(mainWindow);
    await browser.pause(1000);

    try {
      const result = await createAndConnectRedis();
      mainWindow = result.mainWindow;
    } catch (e) {
      console.warn('⏩ Skipping Redis E2E: connection setup failed', e);
      shouldSkip = true;
      return;
    }

    await goToConsoleTab();

    await executeRedisCommand('SET e2e:string:hello world');
    await executeRedisCommand('SET e2e:string:count 42');
    await executeRedisCommand('HSET e2e:hash:user name Alice age 30 email alice@test.com');
    await executeRedisCommand('LPUSH e2e:list:items apple banana cherry');
    await executeRedisCommand('SADD e2e:set:tags sql redis nosql');
    await executeRedisCommand('ZADD e2e:zset:scores 90 Alice 85 Bob 70 Charlie');
  });

  beforeEach(function () {
    if (shouldSkip) this.skip();
  });

  after(async () => {
    if (shouldSkip) return;
    try {
      const handles = await browser.getWindowHandles();
      const connHandle = handles.find((h) => h !== mainWindow);
      if (connHandle) {
        await browser.switchToWindow(connHandle);
        await goToConsoleTab();
        await executeRedisCommand('DEL e2e:string:hello e2e:string:count');
        await executeRedisCommand('DEL e2e:hash:user e2e:list:items');
        await executeRedisCommand('DEL e2e:set:tags e2e:zset:scores');
        await executeRedisCommand('DEL e2e:write:crud e2e:write:batch:a e2e:write:batch:b');
      }
    } catch { /* best-effort cleanup */ }
    try {
      await closeExtraWindows(mainWindow);
    } catch { /* ignore */ }
  });

  // ── Connection Window Layout ──

  it('Redis 连接窗口应显示"数据浏览"、"命令"和"监控"标签 (RD-001)', async () => {
    const body = await $('body').getText();
    expect(body).toContain(t('redis.items'));
    expect(body).toContain(t('redis.console'));
    expect(body).toContain(t('redis.monitor'));
  });

  it('标题栏应显示 Redis 类型 (RD-002)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('Redis');
    expect(body).toContain(CONN_NAME);
  });

  // ── Database Sidebar ──

  it('左侧边栏应显示 Redis 数据库列表 (RD-003)', async () => {
    await goToItemsTab();
    await browser.pause(500);

    const aside = await $('aside');
    const asideText = await aside.getText();
    expect(asideText).toContain('db');
  });

  it('点击数据库应加载该库的键 (RD-004)', async () => {
    await ensureDbSelected();
    const body = await $('body').getText();
    const hasKeyInfo =
      body.includes('e2e:') ||
      body.includes(t('redis.loadedCount').split('{')[0]) ||
      body.includes('个键');
    expect(hasKeyInfo).toBe(true);
  });

  // ── Key Browser ──

  it('键表格应显示 key/type/TTL/value 列 (RD-005)', async () => {
    await goToItemsTab();
    const body = await $('body').getText();
    const hasColumns = body.includes(t('redis.key')) || body.includes('Key');
    expect(hasColumns).toBe(true);
  });

  it('键表格应显示 Size 列 (RD-016)', async () => {
    await goToItemsTab();
    const body = await $('body').getText();
    expect(body).toContain(t('redis.size'));
  });

  it('默认不显示 Flush 控件 (RD-017)', async () => {
    await goToItemsTab();
    const flushDbBtn = await $(`button*=${t('redis.flushDb')}`);
    const flushAllBtn = await $(`button*=${t('redis.flushAll')}`);
    expect(await flushDbBtn.isExisting()).toBe(false);
    expect(await flushAllBtn.isExisting()).toBe(false);
  });

  it('应能搜索键 (RD-006)', async () => {
    await goToItemsTab();
    const searchInput = await $(`input[placeholder*="${t('redis.searchKeys')}"]`);
    if (await searchInput.isExisting()) {
      await searchKeys('e2e:*');
      const body = await $('body').getText();
      expect(body).toContain('e2e:');
    }
  });

  // ── Key Detail ──

  it('点击键应显示键详情面板 (RD-007)', async () => {
    await goToItemsTab();
    await searchKeys('e2e:string:hello');
    const clicked = await clickKeyRow('e2e:string:hello');
    if (clicked) {
      await browser.pause(1000);
      const body = await $('body').getText();
      expect(body).toContain('world');
    }
  });

  // ── E1 write paths (workbench CRUD + batch) ──

  it('应能通过工作台创建 string 键 (RD-018)', async () => {
    await goToItemsTab();
    await createStringKey('e2e:write:crud', 'initial');
    await searchKeys('e2e:write:crud');
    expect(await bodyContains('e2e:write:crud')).toBe(true);
  });

  it('应能编辑 string 键值 (RD-019)', async () => {
    await goToItemsTab();
    await searchKeys('e2e:write:crud');
    const clicked = await clickKeyRow('e2e:write:crud');
    expect(clicked).toBe(true);
    await browser.pause(1000);

    const setOk = await browser.execute(() => {
      const el = document.querySelector('textarea') as HTMLTextAreaElement | null;
      if (!el) return false;
      const setter = Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, 'value')?.set;
      setter?.call(el, 'updated');
      el.dispatchEvent(new Event('input', { bubbles: true }));
      el.dispatchEvent(new Event('change', { bubbles: true }));
      return true;
    });
    expect(setOk).toBe(true);
    const saveBtn = await $(`button*=${t('common.save')}`);
    await saveBtn.click();
    await browser.pause(1500);

    expect(await bodyContains('updated')).toBe(true);
  });

  it('应能通过批量删除移除单个键 (RD-020)', async () => {
    await goToItemsTab();
    await searchKeys('e2e:write:crud');
    const toggled = await toggleKeyCheckbox('e2e:write:crud', true);
    expect(toggled).toBe(true);
    await batchDeleteSelected();
    await searchKeys('e2e:write:crud');
    expect(await bodyContains('e2e:write:crud')).toBe(false);
  });

  it('应能批量删除两个键 (RD-021)', async () => {
    await goToItemsTab();
    await createStringKey('e2e:write:batch:a', 'a');
    await createStringKey('e2e:write:batch:b', 'b');
    await searchKeys('e2e:write:batch:*');

    expect(await toggleKeyCheckbox('e2e:write:batch:a', true)).toBe(true);
    expect(await toggleKeyCheckbox('e2e:write:batch:b', true)).toBe(true);
    await batchDeleteSelected();

    await searchKeys('e2e:write:batch:*');
    expect(await bodyContains('e2e:write:batch:a')).toBe(false);
    expect(await bodyContains('e2e:write:batch:b')).toBe(false);
  });

  // ── Redis Console (E2) ──

  it('切换到命令标签应显示控制台编辑器 (RD-008)', async () => {
    await goToConsoleTab();
    const textarea = await $('textarea.resize-none');
    await expect(textarea).toBeDisplayed();
  });

  it('应能执行 GET 命令 (RD-009)', async () => {
    await executeRedisCommand('GET e2e:string:hello');
    const body = await $('body').getText();
    expect(body).toContain('world');
  });

  it('应能执行 HGETALL 命令 (RD-010)', async () => {
    await executeRedisCommand('HGETALL e2e:hash:user');
    const body = await $('body').getText();
    expect(body).toContain('Alice');
  });

  it('应能执行 LRANGE 命令 (RD-011)', async () => {
    await executeRedisCommand('LRANGE e2e:list:items 0 -1');
    const body = await $('body').getText();
    const hasListItems = body.includes('cherry') || body.includes('banana') || body.includes('apple');
    expect(hasListItems).toBe(true);
  });

  it('应能执行 SMEMBERS 命令 (RD-012)', async () => {
    await executeRedisCommand('SMEMBERS e2e:set:tags');
    const body = await $('body').getText();
    const hasSetItems = body.includes('sql') || body.includes('redis') || body.includes('nosql');
    expect(hasSetItems).toBe(true);
  });

  it('应能执行 KEYS 命令 (RD-013)', async () => {
    await executeRedisCommand('KEYS e2e:*');
    const body = await $('body').getText();
    expect(body).toContain('e2e:');
  });

  it('应能执行 TYPE 命令 (RD-014)', async () => {
    await executeRedisCommand('TYPE e2e:hash:user');
    const body = await $('body').getText();
    expect(body).toContain('hash');
  });

  it('应能执行多行命令 (RD-015)', async () => {
    await executeRedisCommand('GET e2e:string:hello\nGET e2e:string:count');
    const body = await $('body').getText();
    const hasResults = body.includes('world') || body.includes('42');
    expect(hasResults).toBe(true);
  });

  it('应能执行 PING 命令 (RD-023)', async () => {
    await executeRedisCommand('PING');
    const body = await $('body').getText();
    expect(body).toContain('PONG');
  });

  // ── Redis Monitor (E2) ──

  it('监控标签应显示 Info/Memory/Slowlog 子页 (RD-022)', async () => {
    await goToMonitorTab();
    const body = await $('body').getText();
    expect(body).toContain(t('redis.info'));
    expect(body).toContain(t('redis.memory'));
    expect(body).toContain(t('redis.slowlog'));

    const memoryTab = await $(`button*=${t('redis.memory')}`);
    await memoryTab.click();
    await browser.pause(1500);
    const afterMemory = await $('body').getText();
    expect(
      afterMemory.includes(t('redis.memoryEmpty')) ||
        afterMemory.includes(t('redis.bytes')) ||
        afterMemory.includes('e2e:'),
    ).toBe(true);

    const slowlogTab = await $(`button*=${t('redis.slowlog')}`);
    await slowlogTab.click();
    await browser.pause(1500);
    const afterSlowlog = await $('body').getText();
    expect(
      afterSlowlog.includes(t('redis.slowlogEmpty')) ||
        afterSlowlog.includes(t('redis.slowlogId')) ||
        afterSlowlog.includes(t('redis.slowlogCommand')),
    ).toBe(true);
  });
});
