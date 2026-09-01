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
import { closeExtraWindows, connectSeededPgInWorkspace, expandAllGroups } from '../helpers.js';

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
  const trigger = await $(`[data-testid="${triggerTestId}"]`);
  if (await trigger.isExisting()) {
    await trigger.moveTo();
    await browser.pause(400);
  }
}

/** Check if an item button exists in the submenu by data-testid containing the id. */
async function hasSubmenuItem(itemId: string): Promise<boolean> {
  return browser.execute((id: string) => {
    const items = document.querySelectorAll('[data-testid="web-context-submenu"] button');
    for (const item of items) {
      const tid = item.getAttribute('data-testid');
      if (tid && tid.includes(id)) return true;
    }
    return false;
  }, itemId);
}

/** Click a submenu item by its data-testid containing the id. */
async function clickSubmenuItem(itemId: string) {
  await browser.execute((id: string) => {
    const items = document.querySelectorAll('[data-testid="web-context-submenu"] button');
    for (const item of items) {
      const tid = item.getAttribute('data-testid');
      if (tid && tid.includes(id)) {
        (item as HTMLElement).click();
        return;
      }
    }
  }, itemId);
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
    await browser.pause(500);
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

    const clip = await browser.execute(() => {
      return new Promise<string>((resolve) => {
        navigator.clipboard
          .readText()
          .then(resolve)
          .catch(() => resolve(''));
      });
    });
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

    const clip = await browser.execute(() => {
      return new Promise<string>((resolve) => {
        navigator.clipboard
          .readText()
          .then(resolve)
          .catch(() => resolve(''));
      });
    });
    expect(clip.length).toBeGreaterThan(0);
    expect(clip.startsWith('postgresql://') || clip.includes('postgres')).toBe(true);
  });

  it('CM-SUB-005: Connection 子菜单中的「编辑连接」应能正常打开编辑对话框', async () => {
    await rightClickFirstConn();
    await hoverSubmenuTrigger('web-context-submenu-trigger-connection-submenu');
    await clickSubmenuItem('edit-connection');
    await browser.pause(1000);

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
    await rightClickFirstConn();
    expect(await hasSubmenuTrigger('web-context-submenu-trigger-create-new-submenu')).toBe(true);

    await hoverSubmenuTrigger('web-context-submenu-trigger-create-new-submenu');
    const labels = await browser.execute(() => {
      const out: Record<string, string> = {};
      const items = document.querySelectorAll('[data-testid="web-context-submenu"] button');
      for (const item of items) {
        const tid = item.getAttribute('data-testid') ?? '';
        if (tid.includes('create-database') || tid.includes('create-user')) {
          const key = tid.includes('create-database') ? 'create-database' : 'create-user';
          out[key] = item.textContent?.trim() ?? '';
        }
      }
      return out;
    });
    expect(labels['create-database']).toBe(t('common.database'));
    expect(labels['create-database']).not.toBe(t('common.createDatabase'));
    expect(labels['create-user']).toBe(t('common.user'));
    expect(labels['create-user']).not.toBe(t('common.createUser'));
    await dismissMenu();
  });
});

describe('未打开连接历史查询 pending 机制 (CM-SUB-QH)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
  });

  after(async () => {
    await closeExtraWindows(mainWindow);
  });

  it('CM-SUB-010: 未打开连接时右键「历史查询」应打开连接并弹出历史查询侧栏', async () => {
    await browser.switchToWindow(mainWindow);
    await browser.pause(500);

    await rightClickFirstConn();

    const historyBtn = await $('[data-testid="web-context-item-query-history"]');
    expect(await historyBtn.isExisting()).toBe(true);
    await historyBtn.click();
    await browser.pause(2000);

    const toolbar = await $('[data-testid="conn-toolbar-new-query"]');
    const toolbarExists = await toolbar.isDisplayed().catch(() => false);
    expect(toolbarExists).toBe(true);

    const historyVisible = await browser.execute(() => {
      const el = document.querySelector('[data-testid="query-history"]');
      return el !== null && el.children.length > 0;
    });
    expect(historyVisible).toBe(true);
  });
});
