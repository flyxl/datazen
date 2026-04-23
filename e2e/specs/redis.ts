import { expect, browser, $, $$ } from '@wdio/globals';
import {
  closeExtraWindows,
  switchToNewWindow,
  findCardByName,
  expandAllGroups,
  openQueryTab,
  executeSQL,
  setEditorContent,
} from '../helpers.js';

const CONN_NAME = 'E2E-Redis';
const REDIS_HOST = 'REDACTED_HOST';
const REDIS_PORT = '6379';
const REDIS_PASSWORD = 'REDACTED_REDIS_PASSWORD';

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
      { timeout: 30000, timeoutMsg: '等待 Redis 连接窗口打开超时' },
    );
    const handles = await browser.getWindowHandles();
    const connWindow = handles.find((h) => h !== mainWindow)!;
    await browser.switchToWindow(connWindow);
    await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
    await browser.pause(2000);
    return { mainWindow, connWindow };
  }

  const newConnBtn = await $('button*=新建连接');
  await newConnBtn.click();
  await switchToNewWindow(mainWindow);

  const redisBtn = await $('button*=Redis');
  await redisBtn.click();
  await browser.pause(300);

  const nameInput = await $('input[placeholder="例如：主数据库"]');
  await nameInput.setValue(CONN_NAME);

  const hostInput = await $('input[placeholder="127.0.0.1"]');
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

  const testBtn = await $('button*=测试连接');
  await testBtn.click();
  await browser.waitUntil(
    async () => {
      const body = await $('body').getText();
      return body.includes('连接成功') || body.includes('text-red-400') || body.includes('Driver error');
    },
    { timeout: 15000, timeoutMsg: '等待 Redis 测试连接超时' },
  );
  await browser.pause(500);

  const saveBtn = await $('button*=保存');
  await saveBtn.click();
  await browser.waitUntil(
    async () => (await browser.getWindowHandles()).length === 1,
    { timeout: 10000, timeoutMsg: '保存连接后窗口未关闭' },
  );
  await browser.switchToWindow(mainWindow);
  await browser.pause(1000);

  const card = await findCardByName(CONN_NAME);
  if (!card) throw new Error(`未找到 Redis 连接 "${CONN_NAME}"`);
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
    { timeout: 30000, timeoutMsg: '等待 Redis 连接窗口打开超时' },
  );
  const handles = await browser.getWindowHandles();
  const connWindow = handles.find((h) => h !== mainWindow)!;
  await browser.switchToWindow(connWindow);
  await $('button*=新建查询').waitForDisplayed({ timeout: 20000 });
  await browser.pause(2000);

  return { mainWindow, connWindow };
}

describe('Redis 数据库支持 (RD-001~RD-015)', () => {
  let mainWindow: string;

  before(async () => {
    const handles = await browser.getWindowHandles();
    mainWindow = handles.find((h) => h === 'main') ?? handles[0];
    await browser.switchToWindow(mainWindow);
    await closeExtraWindows(mainWindow);
    await browser.pause(1000);

    const { connWindow } = await createAndConnectRedis();

    await openQueryTab();
    await executeSQL('SET e2e:string:hello world');
    await executeSQL('SET e2e:string:count 42');
    await executeSQL('HSET e2e:hash:user name Alice age 30 email alice@test.com');
    await executeSQL('LPUSH e2e:list:items apple banana cherry');
    await executeSQL('SADD e2e:set:tags sql redis nosql');
    await executeSQL('ZADD e2e:zset:scores 90 Alice 85 Bob 70 Charlie');
  });

  after(async () => {
    try {
      const handles = await browser.getWindowHandles();
      const connHandle = handles.find((h) => h !== mainWindow);
      if (connHandle) {
        await browser.switchToWindow(connHandle);
        await openQueryTab();
        await executeSQL('DEL e2e:string:hello e2e:string:count');
        await executeSQL('DEL e2e:hash:user e2e:list:items');
        await executeSQL('DEL e2e:set:tags e2e:zset:scores');
      }
    } catch { /* best-effort cleanup */ }
    try {
      await closeExtraWindows(mainWindow);
    } catch { /* ignore */ }
  });

  // ── Connection & Sidebar ──

  it('Redis 连接窗口应显示工具栏 (RD-001)', async () => {
    const toolbar = await $('button*=新建查询');
    await expect(toolbar).toBeDisplayed();
  });

  it('侧边栏应显示 Keys 而非 Tables (RD-002)', async () => {
    const aside = await $('aside');
    const asideText = await aside.getText();
    expect(asideText).toContain('Keys');
  });

  it('侧边栏应显示测试用的 key (RD-003)', async () => {
    const aside = await $('aside');
    const asideText = await aside.getText();
    expect(asideText).toContain('e2e:');
  });

  it('标题栏应显示 Redis 类型 (RD-004)', async () => {
    const body = await $('body').getText();
    expect(body).toContain('Redis');
  });

  it('不应显示"新建表"按钮 (RD-005)', async () => {
    const btns = await $$('button');
    let hasNewTable = false;
    for (const btn of btns) {
      const text = await btn.getText();
      if (text.includes('新建表')) {
        hasNewTable = true;
        break;
      }
    }
    expect(hasNewTable).toBe(false);
  });

  // ── Redis Commands ──

  it('应能执行 GET 命令 (RD-006)', async () => {
    await openQueryTab();
    await executeSQL('GET e2e:string:hello');
    const body = await $('body').getText();
    expect(body).toContain('world');
  });

  it('应能执行 HGETALL 命令 (RD-007)', async () => {
    await openQueryTab();
    await executeSQL('HGETALL e2e:hash:user');
    const body = await $('body').getText();
    expect(body).toContain('name');
    expect(body).toContain('Alice');
  });

  it('应能执行 LRANGE 命令 (RD-008)', async () => {
    await openQueryTab();
    await executeSQL('LRANGE e2e:list:items 0 -1');
    const body = await $('body').getText();
    expect(body).toContain('cherry');
    expect(body).toContain('banana');
    expect(body).toContain('apple');
  });

  it('应能执行 SMEMBERS 命令 (RD-009)', async () => {
    await openQueryTab();
    await executeSQL('SMEMBERS e2e:set:tags');
    const body = await $('body').getText();
    expect(body).toContain('sql');
    expect(body).toContain('redis');
    expect(body).toContain('nosql');
  });

  it('应能执行 ZRANGEBYSCORE 命令 (RD-010)', async () => {
    await openQueryTab();
    await executeSQL('ZRANGEBYSCORE e2e:zset:scores 80 100');
    const body = await $('body').getText();
    expect(body).toContain('Alice');
    expect(body).toContain('Bob');
  });

  it('应能执行 KEYS 命令 (RD-011)', async () => {
    await openQueryTab();
    await executeSQL('KEYS e2e:*');
    const body = await $('body').getText();
    expect(body).toContain('e2e:string:hello');
    expect(body).toContain('e2e:hash:user');
  });

  it('应能执行 TYPE 命令 (RD-012)', async () => {
    await openQueryTab();
    await executeSQL('TYPE e2e:hash:user');
    const body = await $('body').getText();
    expect(body).toContain('hash');
  });

  it('应能执行 TTL 命令 (RD-013)', async () => {
    await openQueryTab();
    await executeSQL('TTL e2e:string:hello');
    const body = await $('body').getText();
    expect(body).toContain('-1');
  });

  it('应能执行多行命令 (RD-014)', async () => {
    await openQueryTab();
    await executeSQL('GET e2e:string:hello\nGET e2e:string:count');
    const body = await $('body').getText();
    expect(body).toContain('world');
    expect(body).toContain('42');
  });

  it('INFO 命令应返回服务器信息 (RD-015)', async () => {
    await openQueryTab();
    await executeSQL('INFO server');
    const body = await $('body').getText();
    expect(body).toContain('redis_version');
  });
});
