/**
 * [tester] e2e-schema-tree 分裂脑根因闭环断言（测试子代理第 1 轮新增）
 *
 * 背景：wdio 每个 spec 文件在 onPrepare 里用 `save_connection`（带外 IPC，不断开会话）
 * 把 `conn_e2e_pg` 背后的 database 换成新的 worker 库；后端 `connect` 具有复用语义，
 * 可能交回仍绑死旧（已 drop）库的会话 —— 于是原始 SQL 打旧库、schema 元数据打新库。
 * helpers 的 `ensureSeededPgSessionFresh()` 探针即本轨 9 个 spec 修复的根因入口。
 *
 * 本文件把该不变量升级为显式、闭环、零文案断言：
 *   活会话 `SELECT current_database()` === 连接配置的 `database` 字段。
 *
 * 覆盖三条路径：
 *   SF-001 入口（UI 连接后）健康不变量；
 *   SF-002 工具栏已存在时复用入口（历史陈旧漏洞路径 `openSeededPgConnectionWindow`）；
 *   SF-003 主动制造脏会话（带外把配置改指 `postgres`），断言探针自愈后不变量恢复。
 *
 * 断言全部为数据回读（DB 名相等），不依赖任何产品界面文案；失败消息用中文诊断信息。
 * 尚未实际执行：本 worktree 无 webdriver 编译产物，仅完成静态评审与 tsc 门禁。
 */
import { expect, browser } from '@wdio/globals';
import {
  E2E_PG_CONN_NAME,
  closeExtraWindows,
  connectConfig,
  connectSeededPgInWorkspace,
  executeQuery,
  invokeBackend,
  openSeededPgConnectionWindow,
  parseQueryRows,
} from '../helpers.js';

/** Seed PG 配置（`get_connections` 行的必要子集，字段名对齐 save_connection 载荷）。 */
interface SeededPgRow {
  id: string;
  name?: string;
  database?: string;
  [k: string]: unknown;
}

/** 维护库名（与 `createWorkerDatabase` 里 `psql -d postgres` 一致，恒存在）。 */
const MAINTENANCE_DB = 'postgres';

/** 找到种子 PG 连接配置：优先“命名匹配 + worker 库”，退回命名匹配。 */
async function seededConfig(): Promise<SeededPgRow & { database: string }> {
  const conns = await invokeBackend<SeededPgRow[]>('get_connections');
  const cfg =
    conns.find((c) => c.name === E2E_PG_CONN_NAME && c.database?.startsWith('e2e_w')) ??
    conns.find(
      (c) => c.name === E2E_PG_CONN_NAME && typeof c.database === 'string' && c.database,
    ) ??
    conns.find((c) => c.name === E2E_PG_CONN_NAME);
  if (!cfg?.id || !cfg.database) {
    throw new Error(
      `未找到种子 PG 连接配置（name=${E2E_PG_CONN_NAME}，需含 database 字段）；` +
        `实际返回=${JSON.stringify(conns.map((c) => ({ id: c.id, name: c.name, database: c.database })))}`,
    );
  }
  return { ...cfg, database: cfg.database };
}

/**
 * 闭环核心：经后端 `connect`（复用语义，正是风险路径）拿到活会话，
 * 读 `current_database()` 与配置 `database` 对比。
 */
async function liveDatabaseMatchesConfig(): Promise<{ actual: string; expected: string }> {
  const cfg = await seededConfig();
  const sessionId = await connectConfig(cfg.id);
  const rows = parseQueryRows(await executeQuery(sessionId, 'SELECT current_database()'));
  const actual = String(rows[0]?.[0] ?? '');
  return { actual, expected: cfg.database };
}

describe('[tester] 分裂脑根因闭环 (test_tester TC-TESTER-SF)', () => {
  let mainWindow = '';
  let originalDatabase = '';

  before(async () => {
    mainWindow = await browser.getWindowHandle();
    await connectSeededPgInWorkspace();
    originalDatabase = (await seededConfig()).database ?? '';
  });

  after(async () => {
    // 容错清理：无论用例如何，都把配置的 database 还原为原 worker 库，
    // 不给后续 spec 留跨文件污染（本文件自身对污染免疫：探针会自愈）。
    try {
      const cfg = await seededConfig();
      await invokeBackend('save_connection', { config: { ...cfg, database: originalDatabase } });
    } catch (err) {
      console.warn('[tester] 还原种子配置 database 失败:', err);
    }
    try {
      await closeExtraWindows(mainWindow);
    } catch (err) {
      console.warn('[tester] closeExtraWindows 失败:', err);
    }
  });

  it('test_tester_SF-001: UI 连接后活会话 home 库必须等于配置的 worker 库', async () => {
    const { actual, expected } = await liveDatabaseMatchesConfig();
    expect(actual).toBe(expected);
  });

  it('test_tester_SF-002: 复用已存在工具栏的入口（openSeededPgConnectionWindow）同样保持会话新鲜', async () => {
    // 历史漏洞：工具栏已存在即跳过 connect，陈旧会话被原样沿用。
    await openSeededPgConnectionWindow(mainWindow);
    const { actual, expected } = await liveDatabaseMatchesConfig();
    expect(actual).toBe(expected);
  });

  it('test_tester_SF-003: 带外改配置制造脏会话后，入口探针必须自愈到新配置库', async () => {
    const cfg = await seededConfig();
    // 模拟 wdio 换库的同款带外写入：不断开现有会话，只改配置 database。
    // 旧活会话 home=e2e_w*、配置=postgres → 必然失配 → 探针 disconnect+reload。
    await invokeBackend('save_connection', { config: { ...cfg, database: MAINTENANCE_DB } });
    await openSeededPgConnectionWindow(mainWindow);
    const { actual, expected } = await liveDatabaseMatchesConfig();
    expect(expected).toBe(MAINTENANCE_DB);
    expect(actual).toBe(MAINTENANCE_DB);
  });
});
