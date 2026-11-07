/**
 * WR-E2 HTTP 契约表测试（P2b/E2）——表驱动遍历 `httpRoutes.ts` 的整张
 * 分发表（41 条），逐条对照 `docs/architecture/platform/http-api-contract.md`
 * §6.1–§6.9 各域表与 §4 总账断言 `createWebBackendTransport().call()` 的
 * wire 行为：
 *
 * - 精确 HTTP method + 路径模板插值（含 encodeURIComponent 抽查）+ 查询串
 *   + 请求体 JSON 形状（构造器特殊形状：PATCH 双服务、取消幂等凭据、
 *   `onEvent` 通道替换）；
 * - 2xx fixture → call() 解析结果；`commitQueryResultCell` 的 200 空体 →
 *   `undefined`（§6.8 v1 裁决）；
 * - 非 2xx 四字段体 `{code,message,requestId,retryDisposition}` → `ApiError`
 *   字段保真（§6.3 wire 形状）；体不可读 → `ServiceUnavailable`；
 *   405/501 与 wire camelCase 代码 → `CapabilityUnsupported` + `never`；
 * - CSRF：变更类请求带 `x-dz-csrf-token`（§5；头名以 backendTransport.ts
 *   实现为准），GET 安全方法不带；一律 `credentials: 'include'`（§4.4）；
 * - `readArtifact` 三取法互斥（§6.5）+ 双模式响应解码；
 * - `unsupportedMethods`：每个声明键 call → `CapabilityUnsupported` +
 *   假 fetch 零调用；分发表与不支持名单不相交（§7 能力诚实）。
 *
 * fetch 用 `vi.stubGlobal` 假体（transport 默认 fetchImpl 透传到
 * globalThis.fetch）；CSRF 令牌注入（缺省读 `__Host-dz_csrf` cookie，node
 * 环境无 document，不触发该路径）。
 */
import { afterEach, describe, expect, it, vi } from 'vitest';
import {
  ApiError,
  createBackendGuard,
  defaultRetryDisposition,
  deserializeApiError,
} from '@datazen/backend-client';

import { WEB_BACKEND_ID, createWebBackendTransport } from '../backendTransport';
import { HTTP_ROUTES, type RouteSpec } from '../httpRoutes';
import { buildWebUnsupportedMethods } from '../webUnsupportedMethods';

/* ------------------------------------------------------------------ */
/* 假 fetch 基建（vi.stubGlobal；transport 缺省 fetchImpl 透传到这里）。 */
/* ------------------------------------------------------------------ */

const CSRF_TOKEN = 'csrf-token-123';
const CSRF_HEADER = 'x-dz-csrf-token';

interface RecordedRequest {
  url: RequestInfo | URL;
  init: RequestInit;
}

const calls: RecordedRequest[] = [];

function stubFetchWith(respond: (request: RecordedRequest, index: number) => Response): void {
  calls.length = 0;
  vi.stubGlobal(
    'fetch',
    vi.fn(async (url: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
      const request: RecordedRequest = { url, init: init ?? {} };
      const index = calls.length;
      calls.push(request);
      return respond(request, index);
    }),
  );
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(body === undefined ? '' : JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function errorResponse(status: number, body: unknown): Response {
  return new Response(typeof body === 'string' ? body : JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function rawResponse(bytes: Uint8Array, headers: Record<string, string>): Response {
  return new Response(bytes as BodyInit, {
    status: 200,
    headers: { 'content-type': 'application/octet-stream', ...headers },
  });
}

function newTransport(): ReturnType<typeof createWebBackendTransport> {
  return createWebBackendTransport({ csrfTokenProvider: () => CSRF_TOKEN });
}

type UnsafeCall = (method: string, payload: unknown) => Promise<unknown>;

function callOf(transport: ReturnType<typeof createWebBackendTransport>): UnsafeCall {
  return transport.call.bind(transport) as unknown as UnsafeCall;
}

/** call() 的拒绝侧取值：成功则测试失败（给出 method 名便于定位）。 */
async function rejectionOf(
  transport: ReturnType<typeof createWebBackendTransport>,
  method: string,
  payload: unknown,
): Promise<ApiError> {
  try {
    await callOf(transport)(method, payload);
  } catch (error: unknown) {
    return error as ApiError;
  }
  throw new Error(`${method} was expected to be rejected.`);
}

afterEach(() => {
  vi.unstubAllGlobals();
});

/* ------------------------------------------------------------------ */
/* 逐条 fixture：payload 最小化 + 期望 method/path/query/body/response。  */
/* path 为插值后的完整路径（无查询串）；query 含前导 '?'。               */
/* ------------------------------------------------------------------ */

interface Row {
  payload: unknown;
  path: string;
  query?: string;
  /** 显式期望体（构造器特殊形状）；缺省 = passthrough（体 = payload）。 */
  body?: unknown;
  noBody?: boolean;
  response?: unknown;
  /**
   * fetch 的 2xx 响应体；缺省 = response（无 decoder 的路由两值同源）。
   * 带 decoder 的路由（{deleted}/{purged}/{cancelled} 拆封）两值不同：
   * wireResponse 是 HTTP 体，response 是解码后的 call() 结果。
   */
  wireResponse?: unknown;
}

/** 写面 fixture 的提交令牌（形状即可——服务器侧由 fixture 直接回 2xx）。 */
const TOKEN = { idempotencyKey: 'idem-1', expiresAt: '2026-01-01T00:00:00Z' };

const ROWS: Record<string, Row> = {
  // ── settings（§6.1）───────────────────────────────────────────────
  getSettings: { payload: undefined, path: '/settings', noBody: true, response: { theme: 'dark' } },
  saveSettings: { payload: { settings: { theme: 'dark' } }, path: '/settings' },

  // ── identity（§6.2）───────────────────────────────────────────────
  getPlatformIdentity: {
    payload: undefined,
    path: '/identity',
    noBody: true,
    response: { clientInstanceId: 'ci-1', organizationId: 'org-1', principalId: 'p-1' },
  },
  issueSubmissionToken: {
    payload: { operation: 'execute' },
    path: '/submission-tokens',
    response: { token: 'tok-1' },
  },

  // ── connection（§6.3；PATCH 一条路由双服务）────────────────────────
  listConnections: { payload: undefined, path: '/connections', noBody: true, response: [] },
  createConnection: {
    payload: { name: 'pg-main', engine: 'postgres' },
    path: '/connections',
    response: { id: 'conn-1' },
  },
  updateConnection: {
    payload: { connectionId: 'conn 7', expectedRevision: 3, patch: { name: 'renamed' } },
    path: '/connections/conn%207',
    body: { expectedRevision: 3, patch: { name: 'renamed' } },
  },
  disableConnection: {
    payload: { connectionId: 'conn-1', expectedRevision: 5 },
    path: '/connections/conn-1',
    body: { expectedRevision: 5, enabled: false },
  },

  // ── session（§6.4；{id} = handle.dbSessionId）──────────────────────
  openSession: {
    payload: { connectionId: 'conn-1', database: 'db1' },
    path: '/sessions',
    response: { handle: { dbSessionId: 'sess-7' } },
  },
  getSession: {
    payload: { handle: { dbSessionId: 'sess-7' } },
    path: '/sessions/sess-7',
    noBody: true,
  },
  setSessionContext: {
    payload: { handle: { dbSessionId: 'sess-7' }, context: { database: 'db2' } },
    path: '/sessions/sess-7/context',
  },
  attachSession: {
    payload: { handle: { dbSessionId: 'sess-7' }, targetConnectionId: 'conn-2' },
    path: '/sessions/sess-7/attach',
  },
  detachSession: {
    payload: { handle: { dbSessionId: 'sess-7' } },
    path: '/sessions/sess-7/detach',
  },
  closeSession: { payload: { handle: { dbSessionId: 'sess-7' } }, path: '/sessions/sess-7/close' },

  // ── execution（概要 §6.3 骨架）─────────────────────────────────────
  executeInSession: {
    payload: { handle: { dbSessionId: 'sess-7' }, sql: 'SELECT 1' },
    path: '/sessions/sess-7/executions',
    response: { executionId: 'exec-9' },
  },
  executeAtTarget: {
    payload: { target: { connectionId: 'conn-1' }, sql: 'SELECT 1' },
    path: '/executions',
    response: { executionId: 'exec-10' },
  },
  getExecution: { payload: { executionId: 'exec-9' }, path: '/executions/exec-9', noBody: true },
  cancelExecution: {
    payload: { executionId: 'exec-9', submissionToken: 'tok-1' },
    path: '/executions/exec-9/cancel',
    body: { submissionToken: 'tok-1' },
  },

  // ── job（概要 §6.3 骨架；过滤平铺为查询参数）────────────────────────
  startJob: {
    payload: { jobKind: 'sync', target: 'db1' },
    path: '/jobs',
    response: { jobId: 'job-4' },
  },
  listJobs: {
    payload: { state: 'running', limit: 20 },
    path: '/jobs',
    query: '?state=running&limit=20',
    noBody: true,
    response: [],
  },
  getJob: { payload: { jobId: 'job-4' }, path: '/jobs/job-4', noBody: true },
  cancelJob: {
    payload: { jobId: 'job-4', submissionToken: 'tok-2' },
    path: '/jobs/job-4/cancel',
    body: { submissionToken: 'tok-2' },
  },

  // ── artifact（§6.5；uploads 单请求信封 + 三取法细则在下方专属 describe）──
  createArtifactUpload: {
    payload: {
      executionId: 'exec-1',
      targetFingerprint: 'fp-1',
      ttlSeconds: 60,
      payloadBase64: 'aGk=',
      expectedSha256: 'ab',
    },
    path: '/artifacts/uploads',
    // 体 = payload 原样（envelope 已在域客户端拼好，路由不二次构造）。
    response: { artifactId: 'art-9' },
  },
  readArtifact: {
    payload: { artifactId: 'art-5', chunkIndex: 2 },
    path: '/artifacts/art-5',
    query: '?chunkIndex=2',
    noBody: true,
    // 服务端回信封 JSON（无 bytes 数组）→ 解码原样返回；bytes 归一在专属用例。
    response: { artifactId: 'art-5', totalChunks: 4 },
  },

  // ── tableData：读与预览（§6.7，POST 读、体 = payload 原样）──────────
  getTableData: {
    payload: { connectionId: 'conn-1', request: { table: 't' } },
    path: '/table-data/reads',
    response: { rows: [] },
  },
  previewTableReadSql: {
    payload: { sql: 'SELECT 1' },
    path: '/table-data/read-sql-previews',
    response: { sql: 'SELECT 1' },
  },
  estimateTableRows: {
    payload: { table: 't' },
    path: '/table-data/row-count-estimates',
    response: 42,
  },
  resolveForeignKeyTargets: {
    payload: { table: 't', column: 'c' },
    path: '/table-data/foreign-key-targets',
    response: { targets: [] },
  },
  listInboundForeignKeys: {
    payload: { table: 't' },
    path: '/table-data/inbound-foreign-keys',
    response: [],
  },
  searchTableValues: {
    payload: { table: 't', needle: 'a' },
    path: '/table-data/value-searches',
    response: { matches: [] },
  },
  resolveResultOrigin: {
    payload: { resultId: 'r-1' },
    path: '/table-data/result-origins',
    response: { origin: 'result' },
  },

  // ── tableData：变更（§6.8）────────────────────────────────────────
  previewPendingChanges: {
    payload: { connectionId: 'conn-1', plan: [] },
    path: '/table-data/change-plan-previews',
    response: { fingerprint: 'fp-1' },
  },
  commitPendingChanges: {
    payload: { planFingerprint: 'fp-1' },
    path: '/table-data/change-commits',
    response: { committed: 3 },
  },
  // §6.8 v1 裁决：200 + 空体（204 语义）→ 归 undefined。
  commitQueryResultCell: {
    payload: { cell: { row: 1, column: 2, value: 'x' } },
    path: '/table-data/result-cell-commits',
  },
  previewResultCellChange: {
    payload: { cell: { row: 0, column: 0 } },
    path: '/table-data/result-cell-change-previews',
    response: { diff: [] },
  },

  // ── tableData：命名视图（§6.9）────────────────────────────────────
  listNamedViews: {
    payload: { scope: { connectionId: 'conn-1', database: 'db', schema: 'public', table: 't' } },
    path: '/named-views',
    query: '?connectionId=conn-1&database=db&schema=public&table=t',
    noBody: true,
    response: [],
  },
  saveNamedView: {
    payload: { view: { id: '', name: 'v' } },
    path: '/named-views',
    response: { id: 'nv-1', name: 'v' },
  },
  deleteNamedView: {
    payload: { id: 'nv/1' },
    path: '/named-views/nv%2F1',
    noBody: true,
    response: [],
  },

  // ── driverGateway（§6.9 过渡域）───────────────────────────────────
  get_driver_commands: {
    payload: { driverType: 'postgres' },
    path: '/driver-gateway/commands',
    query: '?driverType=postgres',
    noBody: true,
    response: [],
  },
  get_connection_commands: {
    payload: { dbSessionId: 'sess-7' },
    path: '/driver-gateway/commands',
    query: '?dbSessionId=sess-7',
    noBody: true,
    response: [],
  },
  execute_driver_command: {
    payload: { command: 'ping', args: {} },
    path: '/driver-gateway/executions',
    response: { ok: true },
  },
  execute_driver_command_stream: {
    // onEvent 通道由传输替换（§6.9）：体里不得出现 onEvent。
    payload: { command: 'stream', onEvent: { channel: 'ch-1' }, applyResultLimit: 10 },
    path: '/driver-gateway/stream-executions',
    body: { command: 'stream', applyResultLimit: 10 },
    response: { streamId: 'stream-1' },
  },

  // ── queryAssets（§6.12；W8 刀3a web 收口）─────────────────────────
  listQueryFavorites: {
    payload: { connectionId: 'conn-1' },
    path: '/query-favorites',
    query: '?connectionId=conn-1',
    noBody: true,
    response: [{ id: 'fav-1', connectionId: 'conn-1', title: 'T', sql: 'SELECT 1' }],
  },
  createQueryFavorite: {
    payload: {
      connectionId: 'conn-1',
      title: 'T',
      sql: 'SELECT 1',
      submissionToken: TOKEN,
    },
    path: '/query-favorites',
    response: { id: 'fav-2', connectionId: 'conn-1', title: 'T', sql: 'SELECT 1' },
  },
  deleteQueryFavorite: {
    payload: { id: 'fav-1', submissionToken: TOKEN },
    path: '/query-favorites/fav-1',
    body: { submissionToken: TOKEN },
  },
  getQueryHistoryPage: {
    payload: { limit: 50, connectionId: 'conn-1', order: 'recent' },
    path: '/query-history',
    query: '?limit=50&connectionId=conn-1&order=recent',
    noBody: true,
    response: { entries: [], total: 0 },
  },
  deleteQueryHistory: {
    payload: { id: 'h-1', submissionToken: TOKEN },
    path: '/query-history/h-1',
    body: { submissionToken: TOKEN },
    wireResponse: { deleted: 2 },
    response: 2,
  },
  clearQueryHistory: {
    payload: { submissionToken: TOKEN },
    path: '/query-history',
    body: { submissionToken: TOKEN },
    wireResponse: { purged: 5 },
  },
  purgeHistory: {
    payload: { scope: 'query', retainDays: 30, submissionToken: TOKEN },
    path: '/query-history/purge',
    wireResponse: { purged: 7 },
    response: 7,
  },

  // ── schemaDiff（§6.13；W8 刀3b web 收口）─────────────────────────
  listSchemaDiffProfiles: {
    payload: {},
    path: '/schema-diff/profiles',
    noBody: true,
    response: [],
  },
  saveSchemaDiffProfile: {
    payload: {
      profile: { version: 1, id: 'sd-1', name: 'Pair' },
      submissionToken: TOKEN,
    },
    path: '/schema-diff/profiles',
    body: { version: 1, id: 'sd-1', name: 'Pair', submissionToken: TOKEN },
  },
  deleteSchemaDiffProfile: {
    payload: { profileId: 'sd-1', submissionToken: TOKEN },
    path: '/schema-diff/profiles/sd-1',
    body: { submissionToken: TOKEN },
  },
  compareTableSchemas: {
    payload: {
      sourceDbSessionId: 'sess-1',
      targetDbSessionId: 'sess-2',
      sourceTableName: 't',
      targetTableName: 't',
    },
    path: '/schema-diff/comparisons',
    response: { table: 't', added: [], removed: [], changed: [] },
  },
  prepareSchemaDiffPlan: {
    payload: {
      sourceDbSessionId: 'sess-1',
      targetDbSessionId: 'sess-2',
      tableNames: ['t'],
      allowDestructive: false,
      submissionToken: TOKEN,
    },
    path: '/schema-diff/plans',
    response: { jobId: 'j-1', kind: 'schemaDiffPrepare', state: 'queued' },
  },
  prepareSchemaUnifiedPlan: {
    payload: {
      sourceDbSessionId: 'sess-1',
      targetDbSessionId: 'sess-2',
      tableNames: ['t'],
      allowDestructive: false,
      sourceObjects: [],
      targetObjects: [],
      submissionToken: TOKEN,
    },
    path: '/schema-diff/unified-plans',
    response: { jobId: 'j-2', kind: 'schemaDiffPrepare', state: 'queued' },
  },
  executeSchemaDiffDeploy: {
    payload: {
      targetDbSessionId: 'sess-2',
      planId: 'plan-1',
      selectionRevision: 3,
      submissionToken: TOKEN,
    },
    path: '/schema-diff/deploys',
    response: { jobId: 'j-3', kind: 'schemaDiffApply', state: 'queued' },
  },
  listSchemaDiffJobs: {
    payload: { states: ['queued', 'failed'], limit: 100 },
    path: '/schema-diff/jobs',
    query: '?states=queued%2Cfailed&limit=100',
    noBody: true,
    response: [],
  },
  getSchemaDiffJobDetails: {
    payload: { jobId: 'j-1' },
    path: '/schema-diff/jobs/j-1',
    noBody: true,
    response: { details: { job: { jobId: 'j-1' } }, prepared: null },
  },
  cancelSchemaDiffDeploy: {
    payload: { jobId: 'j-1', submissionToken: TOKEN },
    path: '/schema-diff/jobs/j-1/cancel',
    body: { submissionToken: TOKEN },
    wireResponse: { cancelled: true },
    response: true,
  },
  verifySchemaDiffJobRecovery: {
    payload: { jobId: 'j-1', targetDbSessionId: 'sess-2', submissionToken: TOKEN },
    path: '/schema-diff/jobs/j-1/recovery-verification',
    body: { targetDbSessionId: 'sess-2', submissionToken: TOKEN },
    response: { details: { job: { jobId: 'j-1' } }, prepared: null },
  },
};

/* ------------------------------------------------------------------ */
/* 分发表账目（§4 总账 + §6 逐域）。                                     */
/* ------------------------------------------------------------------ */

describe('HTTP_ROUTES 分发表账目（契约 §4 总账）', () => {
  it('60 条分发条目（W8 刀3a/刀3b web 收口 +18：queryAssets 7 + schemaDiff 11）；PATCH 双服务 + driver-gateway 双查询共用 → 58 个唯一 (method, pathTemplate)', () => {
    const entries = Object.entries(HTTP_ROUTES) as [string, RouteSpec][];
    expect(entries).toHaveLength(60);
    const shapes = entries.map(([, route]) => `${route.httpMethod} ${route.pathTemplate}`);
    const unique = new Set(shapes);
    expect(unique.size).toBe(58);
    // 两条共用路由都有契约出处：PATCH /connections/{id}（§6.3 一路由双服务，
    // 体判别）与 GET /driver-gateway/commands（§6.9，driverType/dbSessionId 查询判别）。
    for (const shared of ['PATCH /connections/{id}', 'GET /driver-gateway/commands']) {
      expect(shapes.filter((shape) => shape === shared)).toHaveLength(2);
    }
  });

  it('updateConnection 与 disableConnection 共用 PATCH /connections/{id}，靠体判别', () => {
    const update = HTTP_ROUTES['updateConnection'];
    const disable = HTTP_ROUTES['disableConnection'];
    expect(update).toBeDefined();
    expect(disable).toBeDefined();
    expect(update!.httpMethod).toBe('PATCH');
    expect(disable!.httpMethod).toBe('PATCH');
    expect(update!.pathTemplate).toBe(disable!.pathTemplate);
    expect(update!.body!({ expectedRevision: 1 })).toEqual({
      expectedRevision: 1,
      patch: undefined,
    });
    expect(disable!.body!({ expectedRevision: 1 })).toEqual({
      expectedRevision: 1,
      enabled: false,
    });
  });

  it('open_base64_with_dialog 不上表（§6.9/§8 排除行）；表 + 它 = 61 个域方法（W8 收口后：43 + queryAssets 7 + schemaDiff 11）', () => {
    expect(HTTP_ROUTES['open_base64_with_dialog']).toBeUndefined();
    const keys = new Set([...Object.keys(HTTP_ROUTES), 'open_base64_with_dialog']);
    expect(keys.size).toBe(61);
  });
});

/* ------------------------------------------------------------------ */
/* 逐条路由：method / 路径插值 / query / body / 2xx 解析 / CSRF / credentials。 */
/* ------------------------------------------------------------------ */

describe('分发表逐条契约（§6.1–§6.9 + §3/§5 HTTP 约定）', () => {
  it('fixture 行与分发表逐键对齐（漏行/多行即失败）', () => {
    expect(new Set(Object.keys(ROWS))).toEqual(new Set(Object.keys(HTTP_ROUTES)));
  });

  for (const [method, route] of Object.entries(HTTP_ROUTES) as [string, RouteSpec][]) {
    const row = ROWS[method]!;
    it(`${method} → ${route.httpMethod} /api/v1${row.path}${row.query ?? ''}`, async () => {
      stubFetchWith(() => jsonResponse(row.wireResponse ?? row.response));
      const transport = newTransport();
      const result = await callOf(transport)(method, row.payload);

      expect(calls).toHaveLength(1);
      const { url, init } = calls[0]!;
      expect(init.method).toBe(route.httpMethod);
      expect(String(url)).toBe(`/api/v1${row.path}${row.query ?? ''}`);
      expect(init.credentials).toBe('include');

      const headers = new Headers(init.headers);
      if (route.httpMethod === 'GET') {
        // §5.2：安全方法免 CSRF 头。
        expect(headers.get(CSRF_HEADER)).toBeNull();
      } else {
        // 变更类（含 POST 读，契约 §3 规则 2）一律带头。
        expect(headers.get(CSRF_HEADER)).toBe(CSRF_TOKEN);
      }

      if (row.noBody === true) {
        expect(init.body).toBeUndefined();
        expect(headers.get('content-type')).toBeNull();
      } else {
        const expectedBody = 'body' in row ? row.body : row.payload;
        expect(init.body).toBe(JSON.stringify(expectedBody));
        expect(headers.get('content-type')).toBe('application/json');
      }

      expect(result).toEqual(row.response);
    });
  }

  it('commitQueryResultCell：200 空体 → undefined（§6.8 v1 裁决）', async () => {
    stubFetchWith(() => new Response('', { status: 200 }));
    const transport = newTransport();
    const result = await callOf(transport)('commitQueryResultCell', { cell: {} });
    expect(result).toBeUndefined();
  });

  it('CSRF 令牌缺位：变更类请求不带头但请求照发（server 侧 401/403 裁决）', async () => {
    stubFetchWith(() => jsonResponse({ settings: {} }));
    const transport = createWebBackendTransport({ csrfTokenProvider: () => null });
    await callOf(transport)('saveSettings', { settings: {} });
    expect(calls).toHaveLength(1);
    const headers = new Headers(calls[0]!.init.headers);
    expect(headers.get(CSRF_HEADER)).toBeNull();
    expect(calls[0]!.init.method).toBe('PUT');
  });
});

/* ------------------------------------------------------------------ */
/* 错误侧：非 2xx → ApiError（§6.3 wire 四字段体；errors.ts 归一词汇）。  */
/* ------------------------------------------------------------------ */

const WIRE_REJECTION = {
  code: 'PermissionDenied',
  message: 'denied by policy',
  requestId: 'req-42',
  retryDisposition: 'never',
} as const;

describe('错误归一（§6.3 wire 形状 × errors.ts）', () => {
  it('每条路由：非 2xx 四字段体 → ApiError 字段保真', async () => {
    const transport = newTransport();
    for (const [method, route] of Object.entries(HTTP_ROUTES) as [string, RouteSpec][]) {
      stubFetchWith(() => errorResponse(403, WIRE_REJECTION));
      const error = await rejectionOf(transport, method, ROWS[method]!.payload);
      expect(error).toBeInstanceOf(ApiError);
      expect(error.code).toBe('PermissionDenied');
      expect(error.message).toBe('denied by policy');
      expect(error.requestId).toBe('req-42');
      expect(error.retryDisposition).toBe('never');
      expect(route).toBeDefined(); // 遍历以分发表为准，防止表空转。
    }
  });

  it('体不可读（HTML 5xx）→ ServiceUnavailable，带状态码', async () => {
    stubFetchWith(() => new Response('<html>boom</html>', { status: 503 }));
    const error = await rejectionOf(newTransport(), 'getSettings', undefined);
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('ServiceUnavailable');
    expect(error.message).toContain('HTTP 503');
    expect(error.retryDisposition).toBe('safeRead');
  });

  it('405 + wire camelCase 代码（capabilityUnsupported）→ CapabilityUnsupported，缺省 retryDisposition=never', async () => {
    stubFetchWith(() =>
      errorResponse(405, {
        code: 'capabilityUnsupported',
        message: 'method not allowed',
        requestId: 'req-7',
      }),
    );
    const error = await rejectionOf(newTransport(), 'saveSettings', { settings: {} });
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('CapabilityUnsupported');
    expect(error.retryDisposition).toBe('never');
    expect(error.unsupportedKind).toBeUndefined(); // wire 不携带客户端侧判别。
  });

  it('501 + 完整信封 → CapabilityUnsupported 且 retryDisposition=never 不被覆盖', async () => {
    stubFetchWith(() =>
      errorResponse(501, {
        code: 'CapabilityUnsupported',
        message: 'not implemented',
        requestId: 'req-8',
        retryDisposition: 'never',
      }),
    );
    const error = await rejectionOf(newTransport(), 'executeAtTarget', { sql: 'SELECT 1' });
    expect(error.code).toBe('CapabilityUnsupported');
    expect(error.requestId).toBe('req-8');
    expect(error.retryDisposition).toBe('never');
  });

  it('网络层失败 → ServiceUnavailable（deserializeApiError 归一口径）', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn(async () => {
        throw new TypeError('fetch failed');
      }),
    );
    const error = await rejectionOf(newTransport(), 'getSettings', undefined);
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('ServiceUnavailable');
    expect(error.message).toBe('fetch failed');
  });
});

/* ------------------------------------------------------------------ */
/* readArtifact：三取法互斥（§6.5）+ 双模式响应解码。                    */
/* ------------------------------------------------------------------ */

describe('readArtifact 三取法（§6.5：互斥查询参数 + 双模式解码）', () => {
  it('metadata 取法：?metadata=1；JSON 信封的 bytes 数组归一为 Uint8Array', async () => {
    stubFetchWith(() =>
      jsonResponse({ artifactId: 'art-5', chunkIndex: 1, totalChunks: 4, bytes: [104, 105] }),
    );
    const result = (await callOf(newTransport())('readArtifact', {
      artifactId: 'art-5',
      metadata: true,
    })) as Record<string, unknown>;

    expect(String(calls[0]!.url)).toBe('/api/v1/artifacts/art-5?metadata=1');
    expect(calls[0]!.init.method).toBe('GET');
    expect(result['artifactId']).toBe('art-5');
    expect(result['totalChunks']).toBe(4);
    expect(result['bytes']).toEqual(new Uint8Array([104, 105]));
  });

  it('chunk 取法：?chunkIndex=2；原始字节响应 → 信封回显定位 + 响应头元数据', async () => {
    stubFetchWith(() =>
      rawResponse(new Uint8Array([7, 8, 9]), {
        'x-dz-total-chunks': '4',
        'x-dz-published-chunk-count': '3',
        'x-dz-published-byte-length': '9',
        'x-dz-result-completeness': 'truncated',
      }),
    );
    const result = (await callOf(newTransport())('readArtifact', {
      artifactId: 'art-5',
      chunkIndex: 2,
    })) as Record<string, unknown>;

    expect(String(calls[0]!.url)).toBe('/api/v1/artifacts/art-5?chunkIndex=2');
    expect(result['artifactId']).toBe('art-5');
    expect(result['chunkIndex']).toBe(2);
    expect(result['offset']).toBe(0);
    expect(result['totalChunks']).toBe(4);
    expect(result['publishedChunkCount']).toBe(3);
    expect(result['publishedByteLength']).toBe(9);
    expect(result['resultCompleteness']).toBe('truncated');
    expect(result['bytes']).toEqual(new Uint8Array([7, 8, 9]));
  });

  it('offset 取法：?offset=&limit=；缺省响应头 → completeness=complete、published* 缺省', async () => {
    stubFetchWith(() => rawResponse(new Uint8Array([1, 2]), { 'x-dz-total-chunks': '2' }));
    const result = (await callOf(newTransport())('readArtifact', {
      artifactId: 'art-5',
      offset: 100,
      limit: 50,
    })) as Record<string, unknown>;

    expect(String(calls[0]!.url)).toBe('/api/v1/artifacts/art-5?offset=100&limit=50');
    expect(result['chunkIndex']).toBe(0);
    expect(result['offset']).toBe(100);
    expect(result['totalChunks']).toBe(2);
    expect(result['publishedChunkCount']).toBeUndefined();
    expect(result['publishedByteLength']).toBeUndefined();
    expect(result['resultCompleteness']).toBe('complete');
    expect(result['bytes']).toEqual(new Uint8Array([1, 2]));
  });

  it('三取法全缺 → InvalidArgument，且不发请求（互斥语义的客户端半边）', async () => {
    stubFetchWith(() => jsonResponse({}));
    const error = await rejectionOf(newTransport(), 'readArtifact', { artifactId: 'art-5' });
    expect(error).toBeInstanceOf(ApiError);
    expect(error.code).toBe('InvalidArgument');
    expect(calls).toHaveLength(0);
  });
});

/* ------------------------------------------------------------------ */
/* unsupportedMethods + guard（§7 能力诚实；errors.ts/guard.ts 词汇）。  */
/* ------------------------------------------------------------------ */

describe('unsupportedMethods 声明与 guard（§7）', () => {
  it('每个声明键都是 capability，零 notImplemented（§7：能力差异≠未实现）', () => {
    const unsupported = buildWebUnsupportedMethods();
    const entries = Object.entries(unsupported);
    expect(entries.length).toBeGreaterThanOrEqual(42);
    for (const [, kind] of entries) expect(kind).toBe('capability');
  });

  it('open_base64_with_dialog 在声明中且为 capability（§6.9/§8 排除行）', () => {
    const unsupported = buildWebUnsupportedMethods();
    expect(unsupported['open_base64_with_dialog']).toBe('capability');
  });

  it('分发表与不支持名单不相交（上表方法不得再声明不可服务）', () => {
    const unsupported = buildWebUnsupportedMethods();
    const routeKeys = new Set(Object.keys(HTTP_ROUTES));
    const overlap = Object.keys(unsupported).filter((key) => routeKeys.has(key));
    expect(overlap).toEqual([]);
  });

  it('每个声明键：call → CapabilityUnsupported(kind capability)，假 fetch 零调用', async () => {
    stubFetchWith(() => {
      throw new Error('fetch must never be reached for unsupported methods');
    });
    const transport = newTransport();
    const unsupported = transport.unsupportedMethods;
    const keys = Object.keys(unsupported ?? {});
    expect(keys.length).toBeGreaterThan(0);
    for (const key of keys) {
      const error = await rejectionOf(transport, key, undefined);
      expect(error).toBeInstanceOf(ApiError);
      expect(error.code).toBe('CapabilityUnsupported');
      expect(error.unsupportedKind).toBe('capability');
    }
    expect(calls).toHaveLength(0);
  });

  it('分发表之外的 keyof MethodMap 兜底：call 直接抛 CapabilityUnsupported（backendTransport 兜底半边）', async () => {
    stubFetchWith(() => jsonResponse({}));
    const error = await rejectionOf(newTransport(), 'open_base64_with_dialog', { path: '/tmp' });
    expect(error.code).toBe('CapabilityUnsupported');
    expect(error.unsupportedKind).toBe('capability');
    expect(calls).toHaveLength(0);
  });

  it('guard 层先行拦截：createBackendGuard(WEB_BACKEND_ID, unsupportedMethods) → CapabilityUnsupported/never', async () => {
    const transport = newTransport();
    const guard = createBackendGuard(WEB_BACKEND_ID, transport.unsupportedMethods ?? {});
    expect(WEB_BACKEND_ID).toBe('web');
    const rejection = await guard('open_base64_with_dialog', async () => 'unreachable').then(
      () => null,
      (error: unknown) => error as ApiError,
    );
    expect(rejection).toBeInstanceOf(ApiError);
    expect(rejection!.code).toBe('CapabilityUnsupported');
    expect(rejection!.unsupportedKind).toBe('capability');
    expect(rejection!.retryDisposition).toBe('never');
    expect(rejection!.message).toContain('web backend');
  });

  it('405/501 语义与 retryDisposition=never（对照 errors.ts 缺省策略）', () => {
    expect(defaultRetryDisposition('CapabilityUnsupported')).toBe('never');
    const direct = new ApiError('CapabilityUnsupported', 'local refusal', {
      unsupportedKind: 'capability',
    });
    expect(direct.retryDisposition).toBe('never');
    const wire = deserializeApiError({
      code: 'CapabilityUnsupported',
      message: 'nope',
      requestId: 'r-1',
    });
    expect(wire.code).toBe('CapabilityUnsupported');
    expect(wire.retryDisposition).toBe('never');
    expect(wire.unsupportedKind).toBeUndefined();
  });
});
