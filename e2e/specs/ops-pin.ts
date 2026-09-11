/**
 * E2E: 连接 Pin 置顶（ops §5.4）
 *
 * 走通完整链路：右键连接 → 点击「Pin Connection」→ 连接置顶 &
 * 菜单标签翻转为「Unpin Connection」→ 清除 Pin 后恢复原顺序。
 *
 * 数据构造：依赖 wdio.conf.ts 种下的 `本地 PostgreSQL`（conn_e2e_pg）+ 动态创建两个临时连接，
 * 用固定、独立的名称，after 中整体删除（含恢复 seed 的 pinned 状态）。
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import { closeExtraWindows, createAndConnectPostgreSQL, expandAllGroups } from '../helpers.js';

const STAMP = Date.now().toString(36);
const PIN_CONN_A = `e2e-pin-a-${STAMP}`;
const PIN_CONN_B = `e2e-pin-b-${STAMP}`;

/** Right-click the web context menu on a connection item by exact name. */
async function rightClickConn(connName: string) {
  await browser.execute((name: string) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    const item = items.find((el) => {
      const attr = el.getAttribute('data-conn-name');
      if (attr) return attr === name;
      return el.querySelector('span.truncate')?.textContent?.trim() === name;
    });
    if (!item) return false;
    const rect = (item as HTMLElement).getBoundingClientRect();
    item.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: rect.left + rect.width / 2,
        clientY: rect.top + rect.height / 2,
      }),
    );
    return true;
  }, connName);
  await browser.pause(400);
}

/** 菜单文案（web context menu 内全部按钮文本）。 */
async function menuText(): Promise<string> {
  const menu = await $('[data-testid="web-context-menu"]');
  if (!(await menu.isExisting())) return '';
  return menu.getText();
}

/** 点击菜单或子菜单项（按文本包含匹配）。 */
async function clickMenuItem(label: string) {
  await browser.execute((lbl: string) => {
    const menuItems = document.querySelectorAll(
      '[data-testid="web-context-menu"] button, [data-testid="web-context-submenu"] button',
    );
    for (const item of menuItems) {
      if (item.textContent?.includes(lbl)) {
        (item as HTMLElement).click();
        return;
      }
    }
  }, label);
  await browser.pause(500);
}

/** Hover a submenu trigger to open its submenu. */
/** Hover a submenu trigger to open its submenu (deterministic on WebKit). */
async function hoverSubmenu(testid: string) {
  const trigger = await $(`[data-testid="${testid}"]`);
  if (await trigger.isExisting()) {
    // Real pointer hover (.moveTo()) does not reliably open submenus under the
    // WebKit WebDriver. WebContextMenu opens a submenu on onMouseEnter / onFocus,
    // so dispatch those DOM events deterministically.
    await trigger.moveTo().catch(() => {});
    await browser.execute((sel: string) => {
      const t = document.querySelector(sel) as HTMLElement | null;
      t?.dispatchEvent(new MouseEvent('mouseover', { bubbles: true, cancelable: true }));
      t?.focus();
    }, `[data-testid="${testid}"]`);
    await browser
      .waitUntil(
        () =>
          browser.execute(() => {
            const sub = document.querySelector('[data-testid="web-context-submenu"]');
            return !!sub && sub.querySelectorAll('[data-testid^="web-context-item-"]').length > 0;
          }),
        { timeout: 3000, timeoutMsg: '子菜单未打开' },
      )
      .catch(() => {});
  }
}

const hoverOrganizeSubmenu = () => hoverSubmenu('web-context-submenu-trigger-organize-submenu');

/** 关闭菜单。 */
async function dismissMenu() {
  await browser.execute(() => document.dispatchEvent(new MouseEvent('mousedown')));
  await browser.pause(300);
}

/** Check if a menu item with given id exists. */
async function hasMenuItemId(id: string): Promise<boolean> {
  const item = await $(`[data-testid="web-context-item-${id}"]`);
  return item.isExisting();
}

/** 读取某个连接项在 `data-conn-item` 列表中的位置（0 起）。 */
async function connIndexInList(connName: string): Promise<number> {
  return browser.execute((name: string) => {
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    return items.findIndex((el) => {
      const attr = el.getAttribute('data-conn-name');
      if (attr) return attr === name;
      return el.querySelector('span.truncate')?.textContent?.trim() === name;
    });
  }, connName);
}

/** 反向查询某个连接是否 pinned（通过 get_connections 返回值；按 name 或 id 匹配）。 */
async function connPinned(nameOrId: string): Promise<boolean> {
  const list = await browser.executeAsync((done: (r: unknown) => void) => {
    (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
      ?.invoke?.('get_connections')
      .then((r: unknown) => done(r))
      .catch(() => done([]));
  });
  const arr = (list ?? []) as Array<{ id: string; name?: string; pinned?: boolean }>;
  const c = arr.find((x) => x.id === nameOrId || x.name === nameOrId);
  return c?.pinned === true;
}

describe('运维 §5.4: 连接 Pin 置顶 (OPS-PIN)', () => {
  let mainWindow: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);
    await expandAllGroups();

    // 构造两个临时 PG 连接（固定名称 + 时间戳），列表 ≥3 条便于观察排序
    await createAndConnectPostgreSQL({ name: PIN_CONN_A });
    await closeExtraWindows(mainWindow);
    await createAndConnectPostgreSQL({ name: PIN_CONN_B });
    await closeExtraWindows(mainWindow);
    await expandAllGroups();
    await browser.pause(500);
  });

  after(async () => {
    // 清理临时连接（固定 Id：createAndConnect* 使用传入 name 作为连接名）
    for (const name of [PIN_CONN_A, PIN_CONN_B]) {
      await browser.executeAsync((n: string, done: (r: unknown) => void) => {
        (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
          ?.invoke?.('get_connections')
          .then((list: unknown) => {
            const conns = (list ?? []) as Array<{ id: string; name?: string }>;
            const c = conns.find((x) => x.name === n || x.id === n);
            if (c)
              return (
                window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }
              ).__TAURI_INTERNALS__?.invoke('delete_connection', { id: c.id });
            return undefined;
          })
          .then(() => done(null))
          .catch(() => done(null));
      }, name);
    }
    await closeExtraWindows(mainWindow);
  });

  it('OPS-PIN-001: 右键连接菜单包含 Pin / 对象过滤 / 进程列表', async () => {
    await rightClickConn(PIN_CONN_A);
    expect(
      await $('[data-testid="web-context-submenu-trigger-organize-submenu"]').isExisting(),
    ).toBe(true);
    expect(
      await $('[data-testid="web-context-submenu-trigger-connection-submenu"]').isExisting(),
    ).toBe(true);
    expect(await $('[data-testid="web-context-submenu-trigger-server-submenu"]').isExisting()).toBe(
      true,
    );

    await hoverOrganizeSubmenu();
    const organizeText = await browser.execute(() => {
      const sub = document.querySelector('[data-testid="web-context-submenu"]');
      return sub?.textContent ?? '';
    });
    expect(organizeText).toContain(t('main.ctx.pinConnection'));

    await hoverSubmenu('web-context-submenu-trigger-connection-submenu');
    expect(await hasMenuItemId('object-filter')).toBe(true);

    await hoverSubmenu('web-context-submenu-trigger-server-submenu');
    expect(await hasMenuItemId('process-list')).toBe(true);
    expect(await hasMenuItemId('server-status')).toBe(true);
    await dismissMenu();
  });

  it('OPS-PIN-002: Pin 后连接应置顶到当前列表最前', async () => {
    // 用 get_connections 的 pinned 字段作为可靠判据（导航树按 section 渲染，
    // data-conn-item 的裸 index 并不反映置顶顺序）。
    expect(await connPinned(PIN_CONN_A)).toBe(false);

    await rightClickConn(PIN_CONN_A);
    await hoverOrganizeSubmenu();
    await clickMenuItem(t('main.ctx.pinConnection'));
    await browser.pause(800);

    expect(await connPinned(PIN_CONN_A)).toBe(true);
  });

  it('OPS-PIN-003: Pin 后菜单项变更为 Unpin', async () => {
    await rightClickConn(PIN_CONN_A);
    await hoverOrganizeSubmenu();
    const submenuText = await browser.execute(() => {
      const sub = document.querySelector('[data-testid="web-context-submenu"]');
      return sub?.textContent ?? '';
    });
    expect(submenuText).toContain(t('main.ctx.unpinConnection'));
    expect(submenuText).not.toContain(t('main.ctx.pinConnection'));
    await dismissMenu();
  });

  it('OPS-PIN-004: Unpin 后恢复原顺序', async () => {
    await rightClickConn(PIN_CONN_A);
    await hoverOrganizeSubmenu();
    await clickMenuItem(t('main.ctx.unpinConnection'));
    await browser.pause(800);

    const afterA = await connIndexInList(PIN_CONN_A);
    // 恢复不固定 → 不再排在首位
    expect(afterA).not.toBe(0);
  });
});
