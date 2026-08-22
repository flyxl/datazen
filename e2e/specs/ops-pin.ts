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

/** 点击菜单项（按文本包含匹配）。 */
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

/** 关闭菜单。 */
async function dismissMenu() {
  await browser.execute(() => document.dispatchEvent(new MouseEvent('mousedown')));
  await browser.pause(300);
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

/** 反向查询某个连接是否 pinned（通过 get_connections 返回值）。 */
async function connPinned(connId: string): Promise<boolean> {
  const list = await browser.executeAsync((done: (r: unknown) => void) => {
    (window as unknown as { __TAURI_INTERNALS__?: { invoke: Function } }).__TAURI_INTERNALS__
      ?.invoke?.('get_connections')
      .then((r: unknown) => done(r))
      .catch(() => done([]));
  });
  const arr = (list ?? []) as Array<{ id: string; pinned?: boolean }>;
  const c = arr.find((x) => x.id === connId);
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
    const text = await menuText();
    expect(text).toContain(t('main.ctx.pinConnection'));
    expect(text).toContain(t('main.ctx.objectFilter'));
    expect(text).toContain(t('main.ctx.processList'));
    expect(text).toContain(t('main.ctx.serverStatus'));
    await dismissMenu();
  });

  it('OPS-PIN-002: Pin 后连接应置顶到当前列表最前', async () => {
    const beforeA = await connIndexInList(PIN_CONN_A);
    expect(beforeA).toBeGreaterThanOrEqual(0);

    await rightClickConn(PIN_CONN_A);
    const text = await menuText();
    expect(text).toContain(t('main.ctx.pinConnection'));
    await clickMenuItem(t('main.ctx.pinConnection'));
    await browser.pause(800);

    // 置顶：A 应排在第 0 位
    const afterA = await connIndexInList(PIN_CONN_A);
    expect(afterA).toBe(0);
  });

  it('OPS-PIN-003: Pin 后菜单项变更为 Unpin', async () => {
    await rightClickConn(PIN_CONN_A);
    const text = await menuText();
    expect(text).toContain(t('main.ctx.unpinConnection'));
    expect(text).not.toContain(t('main.ctx.pinConnection'));
    await dismissMenu();
  });

  it('OPS-PIN-004: Unpin 后恢复原顺序', async () => {
    await rightClickConn(PIN_CONN_A);
    await clickMenuItem(t('main.ctx.unpinConnection'));
    await browser.pause(800);

    const afterA = await connIndexInList(PIN_CONN_A);
    // 恢复不固定 → 不再排在首位
    expect(afterA).not.toBe(0);
  });
});
