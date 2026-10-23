/**
 * 回归：未配置 database 的连接配置也能打开会话。
 *
 * 缺陷现场：连接表单对非文件型驱动允许不填 database（合法状态），但
 * `connect` 在宿主侧断言「必须有 database」，于是保存成功的连接一打开就报
 * `connect requires a configured database`，而 `test_connection` 却是好的，
 * 用户看到的是「测试能连、双击打不开」。
 *
 * 契约：是否必须给 database 由驱动的 `namespaceShape` 决定；没给时由驱动按
 * 自身默认值解析（PostgreSQL → 与服务端同名默认库），宿主不再拦。
 */
import { expect } from '@wdio/globals';
import { executeQuery, invokeBackend, parseQueryRows } from '../helpers.js';

const CONN_ID = 'conn_e2e_pg_no_database';

function profileWithoutDatabase(): Record<string, unknown> {
  return {
    id: CONN_ID,
    name: 'E2E-PG-No-Database',
    databaseType: 'postgresql',
    host: process.env.E2E_PG_HOST || process.env.PG_HOST || '127.0.0.1',
    port: Number(process.env.E2E_PG_PORT || process.env.PG_PORT) || 5432,
    username: process.env.E2E_PG_USER || process.env.PG_USER || 'postgres',
    password: process.env.E2E_PG_PASSWORD || process.env.PG_PASSWORD || '',
    group: 'E2E 测试',
    sslMode: 'disable',
  };
}

describe('未配置 database 的连接 (CONN-NODB)', () => {
  const sessions: string[] = [];

  before(async () => {
    await invokeBackend('delete_connection', { id: CONN_ID }).catch(() => {});
    await invokeBackend('save_connection', { config: profileWithoutDatabase() });
  });

  after(async () => {
    for (const dbSessionId of sessions) {
      await invokeBackend('disconnect', { dbSessionId }).catch(() => {});
    }
    await invokeBackend('delete_connection', { id: CONN_ID }).catch(() => {});
  });

  it('CONN-NODB-01: 保存的配置不带 database 时仍能打开会话', async () => {
    const dbSessionId = await invokeBackend<string>('connect', { connectionId: CONN_ID });
    sessions.push(dbSessionId);

    expect(typeof dbSessionId).toBe('string');
    expect(dbSessionId.length).toBeGreaterThan(0);
  });

  it('CONN-NODB-02: 会话落到驱动默认库，查询可直接执行', async () => {
    const payload = await executeQuery(sessions[0], 'select current_database() as current_db');
    const rows = parseQueryRows(payload);

    expect(rows.length).toBeGreaterThan(0);
    const currentDb = String(rows[0][0]);
    expect(currentDb.length).toBeGreaterThan(0);
    expect(currentDb).not.toBe('undefined');
  });

  it('CONN-NODB-03: 重复打开同一配置不会因缺 database 失败', async () => {
    const again = await invokeBackend<string>('connect', { connectionId: CONN_ID });
    sessions.push(again);

    const payload = await executeQuery(again, 'select 1 as ok');
    expect(parseQueryRows(payload)[0][0]).toBe(1);
  });

  it('CONN-NODB-04: connect_dedicated 同样接受缺省 database', async () => {
    const dedicated = await invokeBackend<string>('connect_dedicated', { connectionId: CONN_ID });
    sessions.push(dedicated);

    const payload = await executeQuery(dedicated, 'select current_database() as current_db');
    expect(parseQueryRows(payload).length).toBeGreaterThan(0);
  });
});
