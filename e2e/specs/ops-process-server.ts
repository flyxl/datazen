/**
 * E2E: 进程列表 + 服务器状态面板
 *
 * 完整链路：连接 PG → 右键连接「Process List…」→ 面板展示进程行 → 选中可 Kill 的行 →
 * 点 Kill → 确认 → 断言该 pid 从真实 pg_stat_activity 消失（落库断言）。
 * 服务器状态：右键「服务器状态…」→ 面板展示关键指标 + 刷新。
 *
 * 数据构造：用 IPC 额外起一条**独立的空闲 PG 连接**（可识别 pid），确保 Kill 不会打掉 E2E
 * 主会话连接；after 清理连接配置。
 *
 * 合并了原 ops-server-status-processes.ts 的轻量 UI 渲染断言。
 */
import { expect, browser, $ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  connectBackend,
  connectSeededPgInWorkspace,
  closeExtraWindows,
  disconnectBackend,
  E2E_PG_CONN_NAME,
  invokeBackend,
  queryScalar,
  type QueryResultPayload,
} from '../helpers.js';

const STAMP = Date.now().toString(36);
const PROC_CONN_ID = `e2e_proc_${STAMP}`;
const PROC_CONN_NAME = `E2E-Procs-${STAMP}`;
const SEEDED_CONN_ID = 'conn_e2e_pg';

/**
 * 清掉上一个 spec 残留的 seeded 连接后端会话。
 *
 * WDIO 全程复用同一个 Tauri 进程，而每个 spec 的 worker 数据库是按 spec
 * 创建后即删除的；Rust 侧 `connect` 对同一 connectionId 会复用仍存活的会话，
 * 于是本 spec 的 UI 连接拿到的是绑在已删除 worker 库上的旧会话，
 * `list_processes` / `server_status_snapshot` / `execute_query` 全部报
 * `database "e2e_w..." does not exist`，面板拿不到任何行。
 * 这里强制探测并断开残留会话，随后 UI 连接会基于本 spec 的 worker 库新建会话。
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
 * 关闭失败不再被 .catch 静默吞掉——会带 timeoutMsg 抛错。
 */
async function closeAnyMenu() {
  await browser.execute(() => {
    // WebContextMenu 在 window 上监听 mousedown，用 rootRef.contains(e.target)
    // 判断点按是否落在菜单外。派发目标必须是 Node：
    //  - 向 document 派发不冒泡的事件 → 到不了 window 监听器（原始缺陷）；
    //  - 向 window 派发 → e.target === window（非 Node），contains() 按 WebIDL
    //    抛 TypeError → hide() 永不执行（实测）；
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

async function rightClickConn() {
  // 先确定性关闭残留菜单，避免下面的等待把旧菜单误判为"新菜单已渲染"
  await closeAnyMenu();
  await browser.execute((connName: string) => {
    // 按 data-conn-name 精确定位 seeded 连接：首个 [data-conn-item] 不保证
    // 是已连接的那条，未连接连接的菜单里没有 process-list / server-status。
    const items = Array.from(document.querySelectorAll('[data-conn-item]'));
    const item = items.find((el) => el.getAttribute('data-conn-name') === connName) ?? items[0];
    if (!item) return;
    const rect = item.getBoundingClientRect();
    item.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: rect.left + rect.width / 2,
        clientY: rect.top + rect.height / 2,
      }),
    );
  }, E2E_PG_CONN_NAME);
  // 连接菜单是异步构建的（先 await 后端命令再 show）；本 spec 开头 reload 后
  // 首次打开更慢，固定 pause(400) 会与渲染竞争 → 等待真实菜单出现。
  await browser.waitUntil(async () => (await $('[data-testid="web-context-menu"]')).isExisting(), {
    timeout: 8000,
    timeoutMsg: '连接右键菜单未渲染',
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
  // WebKit WebDriver. WebContextMenu opens a submenu on onMouseEnter / onFocus,
  // so dispatch those DOM events deterministically.
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

/** 面板标题/指标是否显示在当前页面。 */
async function bodyContains(text: string): Promise<boolean> {
  return (await $('body').getText()).includes(text);
}

/** 进程列表面板或服务器状态面板是否出现数据行（轮询等待，行是异步加载的）。 */
async function anyTableRows(timeout = 10000): Promise<boolean> {
  const deadline = Date.now() + timeout;
  for (;;) {
    const found = await browser.execute(() => {
      if (document.querySelectorAll('[data-dt-row]').length > 0) return true;
      const tbody = document.querySelector('table tbody');
      return !!tbody && tbody.querySelectorAll('tr').length > 0;
    });
    if (found) return true;
    if (Date.now() >= deadline) return false;
    await browser.pause(300);
  }
}

/** 点击某 pid 文本所在的行（高亮该行）。 */
async function clickRowByPid(pid: number): Promise<boolean> {
  return browser.execute((pidText: string) => {
    const pidCells = Array.from(document.querySelectorAll('[data-dt-col]')).filter(
      (c) => c.getAttribute('data-dt-col')?.toLowerCase() === 'pid',
    );
    const cell = pidCells.find((c) => c.textContent?.trim() === pidText);
    if (cell) {
      (cell.closest('[tabindex="0"]') as HTMLElement | null)?.click();
      return true;
    }
    const row = Array.from(document.querySelectorAll('[tabindex="0"]')).find((el) =>
      el.textContent?.includes(pidText),
    );
    if (!row) return false;
    (row as HTMLElement).click();
    return true;
  }, String(pid));
}

describe('运维: 进程列表与服务器状态', () => {
  let mainWindow: string;
  let procDbSessionId: string;

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await closeExtraWindows(mainWindow);

    // 独立空闲连接，作为可 KIl 的确定目标
    await invokeBackend('save_connection', {
      config: {
        id: PROC_CONN_ID,
        name: PROC_CONN_NAME,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: process.env.E2E_PG_DB || 'postgres',
        sslMode: 'disable',
      },
    });
    procDbSessionId = await invokeBackend<string>('connect', { connectionId: PROC_CONN_ID });

    // 记下该空闲连接的 pid 供断言
    await invokeBackend('execute_query', {
      dbSessionId: procDbSessionId,
      sql: 'SELECT pg_backend_pid() AS pid',
    });

    // 先清掉上一个 spec 残留的 seeded 后端会话（见 dropLeakedSeededSession 注释），
    // 再让 UI 连接基于本 spec 的 worker 库新建会话。
    await dropLeakedSeededSession();

    // 回到主窗口连接 seeded PG 展示面板
    await connectSeededPgInWorkspace();
    await browser.pause(1500);
  });

  after(async () => {
    try {
      if (procDbSessionId) {
        await disconnectBackend(procDbSessionId);
      }
    } catch {
      /* best effort */
    }
    try {
      await invokeBackend('delete_connection', { id: PROC_CONN_ID });
    } catch {
      /* best effort */
    }
    await closeExtraWindows(mainWindow);
  });

  it('右键连接菜单含「进程列表 / 服务器状态」', async () => {
    await rightClickConn();
    await hoverServerSubmenu();
    expect(await hasMenuItemId('process-list')).toBe(true);
    expect(await hasMenuItemId('server-status')).toBe(true);
    await dismissMenu();
  });

  // [tester] 关闭路径回归断言（复现用例）：
  // closeAnyMenu/dismissMenu 内部的 waitUntil 失败被 .catch 吞掉，"菜单已关闭"
  // 此前无任何硬断言验证。这里显式要求 dismissMenu 后菜单必须真正从 DOM 消失，
  // 防止关闭派发再次静默失效（window 派发 mousedown 时 onDown 的
  // rootRef.contains(window) 抛 TypeError → hide() 不执行）。
  it('[tester] dismissMenu 后右键菜单必须真正关闭', async () => {
    await rightClickConn();
    const menu = await $('[data-testid="web-context-menu"]');
    expect(await menu.isExisting()).toBe(true);
    await dismissMenu();
    await browser.waitUntil(async () => !(await menu.isExisting()), {
      timeout: 3000,
      timeoutMsg: 'dismissMenu 后右键菜单仍未从 DOM 消失（关闭派发失效）',
    });
    expect(await menu.isExisting()).toBe(false);
  });

  it('打开进程列表面板并出现至少一行', async () => {
    await rightClickConn();
    await hoverServerSubmenu();
    await clickMenuItemById('process-list');
    await browser.waitUntil(
      async () => (await $("[data-testid='process-list-view']")).isExisting(),
      { timeout: 8000, timeoutMsg: '进程列表面板未打开' },
    );
    expect(await anyTableRows()).toBe(true);
  });

  it('服务器仪表盘子标签（仪表盘 ⇄ 状态变量 ⇄ 服务器详情）展示关键内容与连接标识', async () => {
    await rightClickConn();
    await hoverServerSubmenu();
    await clickMenuItemById('server-status');
    await browser.waitUntil(
      async () => (await $('[data-testid="server-view-tab-dashboard"]')).isExisting(),
      {
        timeout: 10000,
        timeoutMsg: '服务器仪表盘标签未渲染（server-status 面板未打开或快照加载失败）',
      },
    );
    // 工具面板内显示当前连接名（Req#4）
    expect(await bodyContains(E2E_PG_CONN_NAME)).toBe(true);

    // 「仪表盘」默认：指标卡 + 趋势图
    const dashTab = await $('[data-testid="server-view-tab-dashboard"]');
    await expect(dashTab).toBeDisplayed();
    expect(await bodyContains(t('serverStatus.dashboardTitle'))).toBe(true);
    expect(await bodyContains(t('serverStatus.chartTitle'))).toBe(true);

    // 「状态变量」：PG 返回 pg_settings，出现状态变量表（Host 数据驱动渲染）
    const varsTab = await $('[data-testid="server-view-tab-variables"]');
    await expect(varsTab).toBeDisplayed();
    await varsTab.click();
    await browser.pause(800);
    expect(await bodyContains(t('serverStatus.statusVarsTitle'))).toBe(true);
    expect(await anyTableRows()).toBe(true);

    // 「服务器详情」：明细表
    const detTab = await $('[data-testid="server-view-tab-details"]');
    await expect(detTab).toBeDisplayed();
    await detTab.click();
    await browser.pause(800);
    expect(await bodyContains(t('serverStatus.detailTitle'))).toBe(true);
    expect(await anyTableRows()).toBe(true);
  });

  it('Kill 独立连接并断言 pid 从进程列表消失', async () => {
    // 目标 pid
    const raw = await invokeBackend<QueryResultPayload>('execute_query', {
      dbSessionId: procDbSessionId,
      sql: 'SELECT pg_backend_pid() AS pid',
    });
    const targetPid = queryScalar(raw, 'pid');
    expect(targetPid).toBeGreaterThan(0);

    // 切到进程列表面板
    await rightClickConn();
    await hoverServerSubmenu();
    await clickMenuItemById('process-list');
    await browser.waitUntil(
      async () => (await $("[data-testid='process-list-view']")).isExisting(),
      { timeout: 8000, timeoutMsg: '进程列表面板未打开' },
    );

    // 先确认目标 pid 出现在面板中（轮询等待：行数据由 list_processes 异步加载）
    await browser.waitUntil(
      async () =>
        browser.execute((pidText: string) => {
          return Array.from(document.querySelectorAll('[data-dt-col]')).some(
            (c) =>
              c.getAttribute('data-dt-col')?.toLowerCase() === 'pid' &&
              c.textContent?.trim() === pidText,
          );
        }, String(targetPid)),
      { timeout: 10000, timeoutMsg: `目标 pid ${targetPid} 未出现在进程列表` },
    );

    // 高亮目标行
    const clicked = await clickRowByPid(targetPid);
    expect(clicked).toBe(true);
    await browser.pause(300);

    // 点击 Kill → 确认对话框（按钮无 data-testid，按 title 属性精确定位；
    // 原先 $$().filter(async) 的异步谓词恒为真，会误取页面第一个按钮）
    const kill = await $(`button[title="${t('processList.kill')}"]`);
    await kill.waitForEnabled({ timeout: 5000, timeoutMsg: 'Kill 按钮未进入可用状态' });
    await kill.click();
    await browser.pause(500);
    const okBtn = await $('[data-testid="confirm-dialog-ok"]');
    await expect(okBtn).toBeDisplayed();
    await okBtn.click();
    await browser.pause(1200);

    // 落库断言：从另一条存活连接查询目标 pid 已不存在
    const checkId = `e2e_proc_check_${STAMP}`;
    await invokeBackend('save_connection', {
      config: {
        id: checkId,
        name: `E2E-ProcsCheck-${STAMP}`,
        databaseType: 'postgresql',
        host: process.env.E2E_PG_HOST || '127.0.0.1',
        port: Number(process.env.E2E_PG_PORT) || 5432,
        username: process.env.E2E_PG_USER || 'postgres',
        password: process.env.E2E_PG_PASSWORD || '',
        database: process.env.E2E_PG_DB || 'postgres',
        sslMode: 'disable',
      },
    });
    const checkDbSessionId = await invokeBackend<string>('connect', { connectionId: checkId });
    try {
      const cnt = await invokeBackend<QueryResultPayload>('execute_query', {
        dbSessionId: checkDbSessionId,
        sql: `SELECT count(*)::int AS c FROM pg_stat_activity WHERE pid = ${targetPid}`,
      });
      expect(queryScalar(cnt, 'c')).toBe(0);
    } finally {
      await disconnectBackend(checkDbSessionId);
    }
    try {
      await invokeBackend('delete_connection', { id: checkId });
    } catch {
      /* ok */
    }
  });

  // ── Lightweight UI rendering tests (from ops-server-status-processes.ts) ──

  it('refresh keeps panel healthy', async () => {
    // Open server status panel via context menu on main connection
    await rightClickConn();
    await hoverServerSubmenu();
    await clickMenuItemById('server-status');
    // The default dashboard tab renders the server STATUS VALUE (e.g. the PG
    // version string) and metric cards; the literal '版本' LABEL only exists on
    // the "details" sub-tab. Wait on the dashboard title that is actually shown
    // on the default tab instead (the server-dashboard case asserts it passes the same way).
    await browser.waitUntil(
      async () => (await $('body').getText()).includes(t('serverStatus.dashboardTitle')),
      { timeout: 10000, timeoutMsg: 'Server status panel did not render' },
    );
    // 刷新按钮改用稳定 testid（原先 button*=刷新 依赖文案且在 error/spinner
    // 态下按钮不存在）；快照加载中按钮会被 loading 态替换，等待其重新可见再点。
    const refresh = await $('[data-testid="server-dashboard-refresh"]');
    await refresh.waitForDisplayed({
      timeout: 10000,
      timeoutMsg: '服务器仪表盘刷新按钮未渲染',
    });
    await refresh.click();
    await browser.pause(800);
    const body = await $('body').getText();
    expect(body).toContain(t('serverStatus.dashboardTitle'));
  });

  it('process list table headers render specific columns', async () => {
    // Open process list on the main connection
    await rightClickConn();
    await hoverServerSubmenu();
    await clickMenuItemById('process-list');
    // DataTable headers are rendered as <div data-col-header>/[data-col-label],
    // NOT as <th> / [role="columnheader"]. Wait for a header cell (or a typed
    // row cell) to appear instead.
    await browser.waitUntil(
      async () => {
        const headers = await browser.execute(
          () =>
            document.querySelectorAll('[data-col-header], [role="columnheader"], table th').length,
        );
        const rows = await browser.execute(
          () => document.querySelectorAll('[data-dt-row], table tbody tr').length,
        );
        return headers > 0 || rows > 0;
      },
      { timeout: 10000, timeoutMsg: 'Process list table did not render' },
    );
    const body = await $('body').getText();
    expect(body).toContain(t('processList.colPid'));
    expect(body).toContain(t('processList.colUser'));
    expect(body).toContain(t('processList.colState'));
  });

  it('kill shows confirm then cancel (non-destructive)', async () => {
    const killBtn = await $(`button[title="${t('processList.kill')}"]`);
    if (!(await killBtn.isExisting())) return;
    // Kill 按钮在无高亮行时恒 disabled；先等进程行真正加载出来再选中。
    expect(await anyTableRows()).toBe(true);
    // The Kill button/confirmation requires a highlighted row; select a PID row
    // (mirrors clickRowByPid in the kill test) so the dialog actually opens,
    // otherwise the button stays disabled and no confirm dialog appears.
    await browser.execute(() => {
      const pidCells = Array.from(document.querySelectorAll('[data-dt-col]')).filter(
        (c) => c.getAttribute('data-dt-col')?.toLowerCase() === 'pid',
      );
      const cell = pidCells.find((c) => (c.textContent?.trim().length ?? 0) > 0);
      const row = cell?.closest('[tabindex="0"]') as HTMLElement | null;
      row?.click();
    });
    await browser.pause(300);
    // Kill button must be enabled now (a row is highlighted).
    await killBtn.waitForEnabled({ timeout: 5000, timeoutMsg: '选中行后 Kill 按钮仍不可用' });
    await expect(killBtn).toBeEnabled();
    await killBtn.click();
    await browser.pause(400);
    const body = await $('body').getText();
    expect(body).toContain(t('processList.killTitle'));
    const cancel = await $('button*=取消');
    if (await cancel.isExisting()) {
      await cancel.click();
      await browser.pause(300);
    }
  });
});
