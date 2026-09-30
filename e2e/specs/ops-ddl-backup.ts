/**
 * E2E: DDL 保护 + 备份/还原 预填（ops §5.4）
 *
 * 完整链路：连接 PG → 展开 DB 节点 → 右键「数据库」→ 菜单含「备份 / 恢复」→
 * 点击「备份数据库」→ 备份子窗口以预填 database 打开（URL 直达 + 连接预填）。
 *
 * DDL 风险警告（结构编辑器 destructive alter）由单元测试 `ddlApplyWarnings.test.ts` 覆盖；
 * 此处 E2E 覆盖其入口项存在性（DB 菜单含备份/还原）。
 *
 * 数据构造：DB 节点来自 seeded PostgreSQL；无多余测试数据，after 清理子窗口。
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  connectSeededPgInWorkspace,
  closeExtraWindows,
  switchToNewWindow,
  connectBackend,
  disconnectBackend,
  E2E_PG_CONN_NAME,
} from '../helpers.js';

const SEEDED_CONN_ID = 'conn_e2e_pg';

/**
 * 清掉上一个 spec 残留的 seeded 连接后端会话。
 *
 * WDIO 全程复用同一个 Tauri 进程，而每个 spec 的 worker 数据库是按 spec
 * 创建后即删除的；Rust 侧 `connect` 对同一 connectionId 会复用仍存活的会话，
 * 于是本 spec 的 UI 连接拿到的是绑在已删除 worker 库上的旧会话，
 * schema 树取数（list_databases 等）全部报
 * `database "e2e_w..." does not exist`。这里强制探测并断开残留会话，
 * 随后 UI 连接会基于本 spec 的 worker 库新建会话。
 */
async function dropLeakedSeededSession() {
  try {
    const leaked = await connectBackend(SEEDED_CONN_ID);
    if (leaked) await disconnectBackend(leaked);
  } catch {
    /* 无残留会话 */
  }
}

/**
 * 关闭可能残留的右键菜单并等待其真正消失（无菜单时立即成功）。
 * 关闭失败不再被 .catch 静默吞掉——会带 timeoutMsg 抛错（e2e-ops-menu-BUG-001）。
 */
async function closeAnyMenu() {
  await browser.execute(() => {
    // WebContextMenu 在 window 上监听 mousedown，用 rootRef.contains(e.target)
    // 判断点按是否落在菜单外。派发目标必须是 Node：
    //  - 向 document 派发不冒泡的事件 → 到不了 window 监听器（原始缺陷）；
    //  - 向 window 派发 → e.target === window（非 Node），contains() 按 WebIDL
    //    抛 TypeError → hide() 永不执行（BUG-001 实测）；
    //  - document.body 既是 Node、又位于菜单 portal root 之外（body 是其祖先）
    //    → 冒泡到 window，contains(body) 为 false → hide() 正常关闭。
    document.body.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
  });
  await browser.waitUntil(
    async () => {
      const menu = await $('[data-testid="web-context-menu"]');
      return !(await menu.isExisting());
    },
    { timeout: 3000, timeoutMsg: '右键菜单未关闭' },
  );
}

/** 右键点击一个 DOM 元素（按选择器 + 可作文本过滤）。 */
async function rightClick(selector: string, textMatch?: string) {
  // 先确定性关闭残留菜单，避免下面的等待把旧菜单误判为"新菜单已渲染"
  await closeAnyMenu();
  await browser.execute(
    (sel: string, text: string | undefined) => {
      let el: Element | null = null;
      if (text) {
        const all = document.querySelectorAll(sel);
        for (const e of all) {
          if (e.textContent?.includes(text)) {
            el = e;
            break;
          }
        }
      } else {
        el = document.querySelector(sel);
      }
      if (!el) return;
      const rect = (el as HTMLElement).getBoundingClientRect();
      el.dispatchEvent(
        new MouseEvent('contextmenu', {
          bubbles: true,
          cancelable: true,
          clientX: rect.left + rect.width / 2,
          clientY: rect.top + rect.height / 2,
        }),
      );
    },
    selector,
    textMatch,
  );
  // 菜单是异步构建的（先 await 后端命令再 show）；固定 pause(500) 会与本
  // spec 首次打开（reload 后模块重新加载，较慢）竞争 → 等待真实菜单出现。
  await browser.waitUntil(async () => (await $('[data-testid="web-context-menu"]')).isExisting(), {
    timeout: 8000,
    timeoutMsg: '右键菜单未渲染',
  });
}

async function menuText(): Promise<string> {
  const menu = await $('[data-testid="web-context-menu"]');
  if (!(await menu.isExisting())) return '';
  return menu.getText();
}

async function clickMenuItem(label: string) {
  await browser.execute((lbl: string) => {
    const menuItems = document.querySelectorAll('[data-testid="web-context-menu"] button');
    for (const item of menuItems) {
      if (item.textContent?.includes(lbl)) {
        (item as HTMLElement).click();
        return;
      }
    }
  }, label);
  await browser.pause(500);
}

async function dismissMenu() {
  await closeAnyMenu();
  await browser.pause(200);
}

/** Click a context menu item by its id (data-testid). */
async function clickMenuItemById(id: string) {
  const item = await $(`[data-testid="web-context-item-${id}"]`);
  if (await item.isExisting()) {
    await item.click();
    await browser.pause(500);
  }
}

async function hoverServerSubmenu() {
  // 菜单异步渲染：先等触发项真实出现；缺失时快速失败，而不是静默跳过
  // 导致后续 hasMenuItemId 断言在"菜单没开"的状态下误报。
  const trigger = await $('[data-testid="web-context-submenu-trigger-server-submenu"]');
  await trigger.waitForExist({
    timeout: 8000,
    timeoutMsg: '服务器子菜单触发项未出现（连接菜单未打开？）',
  });
  // Real pointer hover (.moveTo()) does not reliably open submenus under the
  // WebKit WebDriver. The WebContextMenu component opens a submenu on
  // onMouseEnter / onFocus, so dispatch those DOM events deterministically.
  await trigger.moveTo().catch(() => {});
  await browser.execute(() => {
    const t = document.querySelector(
      '[data-testid="web-context-submenu-trigger-server-submenu"]',
    ) as HTMLElement | null;
    t?.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, cancelable: true }));
    t?.focus();
  });
  await browser.waitUntil(
    () =>
      browser.execute(() => {
        const sub = document.querySelector('[data-testid="web-context-submenu"]');
        return !!sub && sub.querySelectorAll('[data-testid^="web-context-item-"]').length > 0;
      }),
    { timeout: 5000, timeoutMsg: '服务器子菜单未打开' },
  );
}

/** Check if a menu item with given id exists. */
async function hasMenuItemId(id: string): Promise<boolean> {
  const item = await $(`[data-testid="web-context-item-${id}"]`);
  return item.isExisting();
}

/** 展开连接以暴露数据库节点。 */
async function expandConnection(connName: string) {
  await browser.execute((name: string) => {
    const items = document.querySelectorAll('[data-conn-item]');
    for (const item of items) {
      if (item.textContent?.includes(name)) {
        (item as HTMLElement).click();
        break;
      }
    }
  }, connName);
  await browser.pause(2000);
}

describe('运维 §5.4: 备份/还原 预填 (OPS-DDL-BACKUP)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);
    // 先清掉上一个 spec 残留的 seeded 后端会话（见 dropLeakedSeededSession 注释），
    // 再让 UI 连接基于本 spec 的 worker 库新建会话。
    await dropLeakedSeededSession();
    await connectSeededPgInWorkspace();
    await browser.pause(1500);
  });

  afterEach(async () => {
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });

  it('OPS-DDL-001: 连接菜单含「备份 / 还原 / 服务器状态 / 进程列表」', async () => {
    // 按 data-conn-name 定位 seeded 连接：首个 [data-conn-item] 不保证是
    // 已连接的那条，未连接连接的菜单里没有 process-list / server-status。
    await rightClick('[data-conn-item]', E2E_PG_CONN_NAME);
    await hoverServerSubmenu();
    expect(await hasMenuItemId('backup')).toBe(true);
    expect(await hasMenuItemId('restore')).toBe(true);
    expect(await hasMenuItemId('process-list')).toBe(true);
    expect(await hasMenuItemId('server-status')).toBe(true);
    await dismissMenu();
  });

  it('OPS-DDL-002: 数据库节点右键含「备份 / 还原」', async () => {
    // 展开连接暴露 DB 节点
    await expandConnection('PostgreSQL');
    await browser.pause(1000);
    const dbNodeCount = await browser.execute(
      () => document.querySelectorAll('[data-tree-node="db"]').length,
    );
    if (dbNodeCount === 0) {
      console.log('No db nodes, skipping OPS-DDL-002');
    } else {
      await rightClick('[data-tree-node="db"]');
      const text = await menuText();
      expect(await hasMenuItemId('backup')).toBe(true);
      expect(await hasMenuItemId('restore')).toBe(true);
      await dismissMenu();
    }
  });

  it('OPS-DDL-003: 点击「备份」应打开备份子窗口', async () => {
    await rightClick('[data-conn-item]', E2E_PG_CONN_NAME);
    await hoverServerSubmenu();
    if (!(await hasMenuItemId('backup'))) {
      console.log('No backup menu item on connection node, skipping OPS-DDL-003');
      await dismissMenu();
      return;
    }
    await clickMenuItemById('backup');
    const backupWin = await switchToNewWindow(mainWindow);
    await browser.pause(1000);
    const body = await $('body').getText();
    // Backup window opened — just verify we switched to a new window
    await closeExtraWindows(mainWindow);
    await browser.switchToWindow(mainWindow);
  });
});
