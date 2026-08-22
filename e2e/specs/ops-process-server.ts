/**
 * E2E: 进程列表 + 服务器状态面板（ops §5.4）
 *
 * 完整链路：连接 PG → 右键连接「Process List…」→ 面板展示进程行 → 选中可 Kill 的行 →
 * 点 Kill → 确认 → 断言该 pid 从真实 pg_stat_activity 消失（落库断言）。
 * 服务器状态：右键「服务器状态…」→ 面板展示关键指标。
 *
 * 数据构造：用 IPC 额外起一条**独立的空闲 PG 连接**（可识别 pid），确保 Kill 不会打掉 E2E
 * 主会话连接；after 清理连接配置。
 */
import { expect, browser, $, $$ } from '@wdio/globals';
import { t } from '../i18n.js';
import {
  connectSeededPgInWorkspace,
  closeExtraWindows,
  E2E_PG_CONN_NAME,
  invokeBackend,
  queryScalar,
} from '../helpers.js';

const STAMP = Date.now().toString(36);
const PROC_CONN_ID = `e2e_proc_${STAMP}`;
const PROC_CONN_NAME = `E2E-Procs-${STAMP}`;

async function rightClickConn() {
  await browser.execute(() => {
    const item = document.querySelector('[data-conn-item]');
    if (!item) return;
    const rect = (item as HTMLElement).getBoundingClientRect();
    item.dispatchEvent(
      new MouseEvent('contextmenu', {
        bubbles: true,
        cancelable: true,
        clientX: rect.left + rect.width / 2,
        clientY: rect.top + rect.height / 2,
      }),
    );
  });
  await browser.pause(400);
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
  await browser.execute(() => document.dispatchEvent(new MouseEvent('mousedown')));
  await browser.pause(300);
}

/** 面板标题/指标是否显示在当前页面。 */
async function bodyContains(text: string): Promise<boolean> {
  return (await $('body').getText()).includes(text);
}

/** 进程列表面板或服务器状态面板是否有数据行。 */
async function anyTableRows(): Promise<boolean> {
  await browser.pause(800);
  return browser.execute(() => {
    if (document.querySelectorAll('[data-dt-row]').length > 0) return true;
    const tbody = document.querySelector('table tbody');
    return !!tbody && tbody.querySelectorAll('tr').length > 0;
  });
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

describe('运维 §5.4: 进程列表与服务器状态 (OPS-PROC)', () => {
  let mainWindow: string;
  let procConnectionId: string;

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
    procConnectionId = await invokeBackend<string>('connect', { configId: PROC_CONN_ID });

    // 记下该空闲连接的 pid 供断言
    await invokeBackend('execute_query', {
      connectionId: procConnectionId,
      sql: 'SELECT pg_backend_pid() AS pid',
    });

    // 回到主窗口连接 seeded PG 展示面板
    await connectSeededPgInWorkspace();
    await browser.pause(1500);
  });

  after(async () => {
    try {
      await invokeBackend('delete_connection', { id: PROC_CONN_ID });
    } catch {
      /* best effort */
    }
    await closeExtraWindows(mainWindow);
  });

  it('OPS-PROC-001: 右键连接菜单含「进程列表 / 服务器状态」', async () => {
    await rightClickConn();
    const text = await menuText();
    expect(text).toContain(t('main.ctx.processList'));
    expect(text).toContain(t('main.ctx.serverStatus'));
    await dismissMenu();
  });

  it('OPS-PROC-002: 打开进程列表面板并出现至少一行', async () => {
    await rightClickConn();
    await clickMenuItem(t('main.ctx.processList'));
    await browser.pause(1500);
    expect(await bodyContains(t('processList.title'))).toBe(true);
    expect(await anyTableRows()).toBe(true);
  });

  it('OPS-PROC-003: 服务器仪表盘子标签（仪表盘 ⇄ 状态变量 ⇄ 服务器详情）展示关键内容与连接标识', async () => {
    await rightClickConn();
    await clickMenuItem(t('main.ctx.serverStatus'));
    await browser.pause(1500);
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

  it('OPS-PROC-004: Kill 独立连接并断言 pid 从进程列表消失', async () => {
    // 目标 pid
    const raw = await invokeBackend('execute_query', {
      connectionId: procConnectionId,
      sql: 'SELECT pg_backend_pid() AS pid',
    });
    const targetPid = queryScalar(raw, 'pid');
    expect(targetPid).toBeGreaterThan(0);

    // 切到进程列表面板
    await rightClickConn();
    await clickMenuItem(t('main.ctx.processList'));
    await browser.pause(1500);

    // 先确认目标 pid 出现在面板中
    const seenBefore = await browser.execute((pidText: string) => {
      return Array.from(document.querySelectorAll('[data-dt-col]')).some(
        (c) =>
          c.getAttribute('data-dt-col')?.toLowerCase() === 'pid' &&
          c.textContent?.trim() === pidText,
      );
    }, String(targetPid));
    expect(seenBefore).toBe(true);

    // 高亮目标行
    const clicked = await clickRowByPid(targetPid);
    expect(clicked).toBe(true);
    await browser.pause(300);

    // 点击 Kill → 确认对话框
    const killBtn = await $$('button').filter(async (b) =>
      (await b.getText()).includes(t('processList.kill')),
    );
    const kill = killBtn[0];
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
    const checkConn = await invokeBackend<string>('connect', { configId: checkId });
    const cnt = await invokeBackend('execute_query', {
      connectionId: checkConn,
      sql: `SELECT count(*)::int AS c FROM pg_stat_activity WHERE pid = ${targetPid}`,
    });
    expect(queryScalar(cnt, 'c')).toBe(0);
    try {
      await invokeBackend('delete_connection', { id: checkId });
    } catch {
      /* ok */
    }
  });
});
