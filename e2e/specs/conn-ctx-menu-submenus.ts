/**
 * E2E: 连接右键菜单分组子菜单结构 & 未打开连接时历史查询 pending 机制
 *
 * CM-SUB-001~005: 右键连接 → 子菜单分组（Connection / Server / Organize）正确展示与交互
 * CM-SUB-006: 已连接 PostgreSQL 服务器子菜单含进程列表/服务器状态/备份/恢复
 * CM-SUB-010: 未打开连接时点击历史查询 → 打开连接 + 自动弹出历史查询侧边栏
 *
 * 依赖 wdio.conf.ts 种下的 `本地 PostgreSQL`（conn_e2e_pg）。
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  closeExtraWindows,
  clickCardConnectButton,
  connectSeededPgInWorkspace,
  expandAllGroups,
  openConnectionsWorkspace,
  stubClipboardCapture,
  readStubbedClipboard,
  restoreClipboardStub,
} from '../helpers.js';

const CM_SUB_008_CONN = 'E2E-MultiDb-CreateMenu';
const CM_SUB_008_CONN_ID = 'conn_e2e_multi_db_create_menu';

async function invokeBackend<T>(cmd: string, args: Record<string, unknown> = {}): Promise<T> {
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
  if (result && typeof result === 'object' && '__error' in (result as object)) {
    throw new Error(String((result as { __error: string }).__error));
  }
  return result as T;
}

/** Right-click the first connection item via data-conn-item attribute. */
async function rightClickFirstConn() {
  await browser.execute(() => {
    const items = document.querySelectorAll('[data-conn-item]');
    if (items.length === 0) return;
    const el = items[0] as HTMLElement;
    const rect = el.getBoundingClientRect();
    el.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: rect.left + rect.width / 2,
        clientY: rect.top + rect.height / 2,
      }),
    );
  });
  await browser.pause(500);
}

/** Dismiss any open web context menu. */
async function dismissMenu() {
  await browser.execute(() => document.dispatchEvent(new MouseEvent('mousedown')));
  await browser.pause(300);
}

/** Check if a submenu trigger button exists by its data-testid. */
async function hasSubmenuTrigger(triggerTestId: string): Promise<boolean> {
  const el = await $(`[data-testid="${triggerTestId}"]`);
  return el.isExisting();
}

/** Hover a submenu trigger to open its submenu. */
async function hoverSubmenuTrigger(triggerTestId: string) {
  await browser.execute((testId: string) => {
    const trigger = document.querySelector(`[data-testid="${testId}"]`) as HTMLElement | null;
    if (!trigger) throw new Error(`Submenu trigger not found: ${testId}`);
    trigger.dispatchEvent(new MouseEvent('mouseenter', { bubbles: true }));
    trigger.focus();
  }, triggerTestId);
  await browser.pause(500);
  await browser.waitUntil(async () => await $('[data-testid="web-context-submenu"]').isExisting(), {
    timeout: 5000,
    timeoutMsg: `子菜单未打开: ${triggerTestId}`,
  });
}

/** Check if a submenu item exists by its item id. */
async function hasSubmenuItem(itemId: string): Promise<boolean> {
  const el = await $(
    `[data-testid="web-context-submenu"] [data-testid="web-context-item-${itemId}"]`,
  );
  return el.isExisting();
}

/** Click a submenu item by its item id. */
async function clickSubmenuItem(itemId: string) {
  const el = await $(
    `[data-testid="web-context-submenu"] [data-testid="web-context-item-${itemId}"]`,
  );
  await el.waitForExist({ timeout: 5000 });
  await el.click();
  await browser.pause(500);
}

/** Check if a top-level context menu item exists by its data-testid. */
async function hasTopLevelItem(itemId: string): Promise<boolean> {
  const el = await $(`[data-testid="web-context-item-${itemId}"]`);
  return el.isExisting();
}

describe('连接右键菜单子菜单结构 (CM-SUB)', () => {
  before(async () => {
    await expandAllGroups();
    await stubClipboardCapture();
    await browser.pause(500);
  });

  after(async () => {
    await restoreClipboardStub();
  });

  it('CM-SUB-001: 连接右键菜单复制类操作均在 Connection 子菜单', async () => {
    await rightClickFirstConn();
    expect(await hasTopLevelItem('copy-name')).toBe(false);
    expect(await hasTopLevelItem('copy-connection-url')).toBe(false);
    expect(await hasSubmenuTrigger('web-context-submenu-trigger-connection-submenu')).toBe(true);

    await hoverSubmenuTrigger('web-context-submenu-trigger-connection-submenu');
    expect(await hasSubmenuItem('copy-connection-url')).toBe(true);
    expect(await hasSubmenuItem('copy-name')).toBe(true);
    await dismissMenu();
  });

  it('CM-SUB-003: Connection 子菜单「复制名称」点击后应复制到剪贴板', async () => {
    await rightClickFirstConn();
    await hoverSubmenuTrigger('web-context-submenu-trigger-connection-submenu');
    await clickSubmenuItem('copy-name');
    await browser.pause(300);

    const clip = await readStubbedClipboard();
    expect(clip.length).toBeGreaterThan(0);
  });

  it('CM-SUB-002: 连接右键菜单应包含 Connection 子菜单并含编辑、复制连接', async () => {
    await rightClickFirstConn();
    expect(await hasSubmenuTrigger('web-context-submenu-trigger-connection-submenu')).toBe(true);

    await hoverSubmenuTrigger('web-context-submenu-trigger-connection-submenu');
    expect(await hasSubmenuItem('edit-connection')).toBe(true);
    expect(await hasSubmenuItem('duplicate-connection')).toBe(true);
    await dismissMenu();
  });

  it('CM-SUB-007: Connection 子菜单「复制连接 URL」应写入剪贴板', async () => {
    await rightClickFirstConn();
    await hoverSubmenuTrigger('web-context-submenu-trigger-connection-submenu');
    await clickSubmenuItem('copy-connection-url');
    await browser.pause(300);

    const clip = await readStubbedClipboard();
    expect(clip.length).toBeGreaterThan(0);
    expect(clip.startsWith('postgresql://') || clip.includes('postgres')).toBe(true);
  });

  it('CM-SUB-005: Connection 子菜单中的「编辑连接」应能正常打开编辑对话框', async () => {
    await rightClickFirstConn();
    await hoverSubmenuTrigger('web-context-submenu-trigger-connection-submenu');
    await clickSubmenuItem('edit-connection');
    await browser.pause(300);

    const dialog = await $('[role="dialog"]');
    const exists = await dialog.isExisting();
    expect(exists).toBe(true);
    if (exists) {
      await browser.execute(() => {
        const closeBtn = document.querySelector(
          '[role="dialog"] button[aria-label="common.close"]',
        ) as HTMLElement;
        if (closeBtn) {
          closeBtn.click();
        } else {
          document.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
        }
      });
      await browser.pause(500);
    }
  });

  it('CM-SUB-004: 顶级菜单项应简洁：打开/断开、新建查询、历史查询、复制、刷新、删除', async () => {
    await rightClickFirstConn();
    const hasOpenOrDisconnect =
      (await hasTopLevelItem('open-connection')) || (await hasTopLevelItem('disconnect'));
    expect(hasOpenOrDisconnect).toBe(true);
    expect(await hasTopLevelItem('new-query')).toBe(true);
    expect(await hasTopLevelItem('query-history')).toBe(true);
    expect(await hasTopLevelItem('copy-name')).toBe(false);
    expect(await hasTopLevelItem('refresh')).toBe(true);
    expect(await hasTopLevelItem('delete-connection')).toBe(true);
    await dismissMenu();
  });
});

describe('连接右键菜单 Server 子菜单含服务器操作 (CM-SUB-MAN)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('CM-SUB-006: 已连接的 PostgreSQL 右键 Server 子菜单应含进程列表、备份等', async () => {
    await rightClickFirstConn();
    expect(await hasSubmenuTrigger('web-context-submenu-trigger-server-submenu')).toBe(true);
    expect(await hasSubmenuTrigger('web-context-submenu-trigger-manage-submenu')).toBe(false);

    await hoverSubmenuTrigger('web-context-submenu-trigger-server-submenu');
    expect(await hasSubmenuItem('process-list')).toBe(true);
    expect(await hasSubmenuItem('server-status')).toBe(true);
    await dismissMenu();
  });

  it('CM-SUB-008: 新建子菜单应显示短标签「数据库」「用户」', async () => {
    // create-database only appears for multi-db sessions (blank database field).
    await openConnectionsWorkspace(mainWindow);
    await invokeBackend('save_connection', {
      config: {
        id: CM_SUB_008_CONN_ID,
        name: CM_SUB_008_CONN,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: '',
        group: 'E2E 测试',
        colorTag: 'blue',
        sslMode: 'disable',
        options: {},
      },
    });
    await browser.refresh();
    await browser.pause(300);
    await openConnectionsWorkspace(mainWindow);
    await clickCardConnectButton(CM_SUB_008_CONN);
    await browser.pause(300);

    await browser.execute((name: string) => {
      const items = document.querySelectorAll('[data-conn-item]');
      for (const el of items) {
        if (!(el.textContent ?? '').includes(name)) continue;
        const rect = el.getBoundingClientRect();
        el.dispatchEvent(
          new MouseEvent('contextmenu', {
            bubbles: true,
            cancelable: true,
            clientX: rect.left + rect.width / 2,
            clientY: rect.top + rect.height / 2,
          }),
        );
        return;
      }
      throw new Error(`Connection card not found: ${name}`);
    }, CM_SUB_008_CONN);
    await browser.pause(500);

    expect(await hasSubmenuTrigger('web-context-submenu-trigger-create-new-submenu')).toBe(true);

    await hoverSubmenuTrigger('web-context-submenu-trigger-create-new-submenu');
    await browser.waitUntil(async () => hasSubmenuItem('create-database'), {
      timeout: 8000,
      timeoutMsg: '等待新建子菜单项出现超时',
    });
    await browser.waitUntil(async () => hasSubmenuItem('create-user'), {
      timeout: 8000,
      timeoutMsg: '等待新建用户菜单项出现超时',
    });

    const dbLabel = await $(
      '[data-testid="web-context-submenu"] [data-testid="web-context-item-create-database"]',
    ).getText();
    const userLabel = await $(
      '[data-testid="web-context-submenu"] [data-testid="web-context-item-create-user"]',
    ).getText();

    expect(dbLabel).toBe(t('common.database'));
    expect(dbLabel).not.toBe(t('common.createDatabase'));
    expect(userLabel).toBe(t('common.user'));
    expect(userLabel).not.toBe(t('common.createUser'));
    await dismissMenu();
    await invokeBackend('delete_connection', { id: CM_SUB_008_CONN_ID }).catch(() => undefined);
  });
});

describe('未打开连接历史查询 pending 机制 (CM-SUB-QH)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await openConnectionsWorkspace(mainWindow);
    const toolbar = await $('[data-testid="conn-toolbar-new-query"]');
    if (await toolbar.isDisplayed().catch(() => false)) {
      await rightClickFirstConn();
      const disconnect = await $('[data-testid="web-context-item-disconnect"]');
      if (await disconnect.isExisting()) {
        await disconnect.click();
        await browser.pause(300);
      }
      await dismissMenu();
    }
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('CM-SUB-010: 未打开连接时右键「历史查询」应打开连接并弹出历史查询侧栏', async () => {
    await browser.switchToWindow(mainWindow);
    await openConnectionsWorkspace(mainWindow);
    await browser.pause(500);

    await rightClickFirstConn();

    const historyBtn = await $('[data-testid="web-context-item-query-history"]');
    expect(await historyBtn.isExisting()).toBe(true);
    await historyBtn.click();

    const toolbar = await $('[data-testid="conn-toolbar-new-query"]');
    await toolbar.waitForDisplayed({ timeout: 20000 });

    await browser.waitUntil(
      async () => {
        const body = await $('body').getText();
        return (
          body.includes(t('query.historyTitle')) &&
          (body.includes(t('query.noHistory')) ||
            body.includes(t('query.historyScopeCurrent')) ||
            body.includes(t('query.historyScopeAll')))
        );
      },
      { timeout: 20000, timeoutMsg: '等待历史查询侧栏出现超时' },
    );
  });
});
