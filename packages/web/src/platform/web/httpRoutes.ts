/**
 * Web HTTP 分发表（WR-E2）——`docs/architecture/platform/http-api-contract.md`
 * §6.1–§6.9 冻结的域面路由。本表是 web 形态「方法 → { HTTP 方法、路径模板、
 * 请求体形状 }」的唯一定点；逐条对照契约各域表，execution/job 两域用概要
 * §6.3 的骨架表（契约 §10 声明其被收编）。
 *
 * 覆盖账目（对照契约 §4 总账；call 面 43 个域方法、排除 1、服务 42；
 * WR-F6 给 artifact 域补上 §6.5 在账端点 uploads 的 MethodMap 键
 * `createArtifactUpload`——端点本身已在账，补的是方法表面不是新路由）：
 *
 * | 域 | call 方法 | 分发条目 | 依据 |
 * | settings | 2 | 2 | §6.1 |
 * | identity | 2 | 2 | §6.2 |
 * | connection | 4 | 4（PATCH 一条路由双服务 update/disable） | §6.3 |
 * | session | 6 | 6 | §6.4 |
 * | execution | 4 | 4 | 概要 §6.3 骨架 |
 * | job | 4 | 4 | 概要 §6.3 骨架 |
 * | artifact | 2 | 2（uploads 单请求信封 + 三模式互斥查询参数） | §6.5 |
 * | tableData | 14 | 14 | §6.7–§6.9 |
 * | driverGateway | 5 | 4（open_base64_with_dialog 排除不上表） | §6.9/§8 |
 * | **call 合计** | **43** | **42（41 条路径，PATCH 双服务）** | |
 *
 * events 域的 2 条路由（POST /events/subscriptions + GET /events/{streamId}，
 * §6.6 v1.1）不在本表：它们由 `backendTransport.ts` 的 `subscribe()` 两步
 * 流水线一体实现，服务 `subscribeEvents`。两表相加 = 契约 HTTP 面服务的
 * **108/112** 个域方法（§4 总账行 v1.9；`createEventSubscription` 无 `MethodMap`
 * 键，是流水线的登记步而非独立可调方法）。
 *
 * WR-F1 扩展账：mcp / security / aiHost 三域（三缺口合拢）的八条契约路由
 * 登记在文件尾的 `WR_F1_HTTP_ROUTES`（点分契约键，**不在本表**——本表键型
 * 被 `keyof MethodMap` 锁定，而 108 条的账目断言由冻结的
 * `domainContractTables.test.ts` 钉住；wire-direct 域不经 `transport.call`，
 * 逐域登记理由见 `packages/backend-client/src/{mcpClient,securityClient}.ts`
 * 头注）。
 *
 * 约定（契约 §3）：请求体一律 JSON；查询参数只用于简单标量；POST 读有意
 * 且照过 CSRF 头校验；{id} 从 payload 插值。除 `readArtifact`（双模式解码）
 * 外的响应处理统一在 `backendTransport.ts`。
 *
 * 类型保守：构造器一律消费 `unknown` 语境的 payload（局部窄化 + 缺字段抛
 * `InvalidArgument`），泛型窄化只发生在 `call()` 的单点。
 */

import { ApiError } from '@datazen/backend-client';
import type { MethodMap } from '@datazen/backend-client';

/** 分发表里出现的 HTTP 方法（契约 v1 面无 HEAD/OPTIONS）。 */
export type WebHttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE';

/** 响应解码器：payload（回显用）+ 原始响应。缺省走 JSON 解析。 */
export type RouteDecoder = (payload: unknown, response: Response, raw: Uint8Array) => unknown;

/** 一条域面路由：HTTP 方法 + 路径模板 + 路径参数/查询串/请求体构造。 */
export interface RouteSpec {
  httpMethod: WebHttpMethod;
  /** 路径模板，`{id}` 由 `pathId` 从 payload 插值；相对 baseUrl（/api/v1 之后）。 */
  pathTemplate: string;
  /** `{id}` 的取值函数；模板无 `{id}` 时缺省。 */
  pathId?: (payload: unknown) => string;
  /**
   * 复合路径构造（多段资源 id 的逃生舱，W8 刀3d 引入）：在场时优先于
   * pathTemplate/pathId，直接返回完整相对路径。dashboard 组件族
   * （`/dashboards/{id}/widgets/{widgetId}/...`）三段 id 超出单 `{id}`
   * 插值面的表达力；pathTemplate 保留作路由形状的文档面与测试账目键。
   */
  buildPath?: (payload: unknown) => string;
  /** 查询串构造（返回含 `?` 的已编码串或空串）；缺省 = 无查询参数。 */
  query?: (payload: unknown) => string;
  /**
   * 请求体构造；缺省 = payload 原样作为 JSON 体（payload 为 undefined 即无体）。
   * 显式返回 undefined = 无请求体（GET 读、或负载已被查询串/路径吃掉的方法）。
   */
  body?: (payload: unknown) => unknown;
  /** 特殊响应解码（readArtifact 的元数据/字节双模式）；缺省 = JSON。 */
  decode?: RouteDecoder;
}

/* ------------------------------------------------------------------ */
/* unknown 语境的小窄化原语（全表共用；缺字段抛 InvalidArgument）。      */
/* ------------------------------------------------------------------ */

function field(record: unknown, key: string): unknown {
  if (typeof record !== 'object' || record === null || Array.isArray(record)) return undefined;
  return (record as Record<string, unknown>)[key];
}

function requireField(record: unknown, key: string): unknown {
  const value = field(record, key);
  if (value === undefined) {
    throw new ApiError(
      'InvalidArgument',
      `The ${key} field required to address the HTTP route is missing from the payload.`,
    );
  }
  return value;
}

/** 简单标量 → 查询串（契约 §3 规则 1：查询参数只用于简单标量）。 */
function scalarQuery(entries: ReadonlyArray<readonly [string, unknown]>): string {
  const params = new URLSearchParams();
  for (const [key, value] of entries) {
    if (typeof value === 'string' || typeof value === 'number' || typeof value === 'boolean') {
      params.set(key, String(value));
    }
  }
  const text = params.toString();
  return text.length > 0 ? `?${text}` : '';
}

/** 无请求体（GET 读 / 负载已进路径与查询串）。 */
function noBody(): undefined {
  return undefined;
}

/* ------------------------------------------------------------------ */
/* 路径参数（{id} 从 payload 插值）。                                    */
/* ------------------------------------------------------------------ */

const pathConnectionId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'connectionId')));

/** 会话族路由的 {id}：SessionHandle 的 dbSessionId（运行时句柄的稳定半边）。 */
const pathSessionId = (payload: unknown): string => {
  const handle = requireField(payload, 'handle');
  return encodeURIComponent(String(requireField(handle, 'dbSessionId')));
};

const pathExecutionId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'executionId')));

const pathJobId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'jobId')));

const pathArtifactId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'artifactId')));

const pathNamedViewId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'id')));

/** queryAssets：收藏与历史条目的 {id}。 */
const pathFavoriteId = pathNamedViewId;
const pathHistoryEntryId = pathNamedViewId;

/** schemaDiff：profile 与 job 的路径参数。 */
const pathSchemaDiffProfileId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'profileId')));
const pathSchemaDiffJobId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'jobId')));

/* ------------------------------------------------------------------ */
/* 请求体构造（契约逐行要求的特殊形状；其余方法体 = payload 原样）。      */
/* ------------------------------------------------------------------ */

/** PATCH 双服务的 update 半边：`{ expectedRevision, patch }`（§6.3）。 */
const updateConnectionBody = (payload: unknown): unknown => ({
  expectedRevision: field(payload, 'expectedRevision'),
  patch: field(payload, 'patch'),
});

/**
 * PATCH 双服务的 disable 半边：`{ expectedRevision, enabled: false }`（§6.3）。
 * `ProfilePatch` 永远碰不到 `enabled`，顶层 `enabled: false` 只属于 disable
 * 语义——判别无歧义，本构造器就是那条判别的落地。
 */
const disableConnectionBody = (payload: unknown): unknown => ({
  expectedRevision: field(payload, 'expectedRevision'),
  enabled: false,
});

/** 取消路由：executionId/jobId 进路径，体只留幂等凭据（SubmitRequest 半边）。 */
const submissionTokenBody = (payload: unknown): unknown => ({
  submissionToken: field(payload, 'submissionToken'),
});

/**
 * saveSchemaDiffProfile：桌面域形状是 `{ profile, submissionToken }`，HTTP
 * 体是 profile 平铺 + 令牌（契约 §6.13：`SchemaDiffProfile & SubmitRequest`）。
 */
const saveSchemaDiffProfileBody = (payload: unknown): unknown => ({
  ...((field(payload, 'profile') as Record<string, unknown> | undefined) ?? {}),
  submissionToken: field(payload, 'submissionToken'),
});

/**
 * 写面响应的数字包装拆封：HTTP 回 `{ deleted }` / `{ purged }`，桌面域形状
 * 是裸 number（契约以 backend-client 为准，本表把两形归一）。
 */
const numberFieldOf =
  (key: string): RouteDecoder =>
  (_payload, _response, raw): unknown => {
    const text = new TextDecoder().decode(raw).trim();
    if (text.length === 0) return 0;
    try {
      const parsed = JSON.parse(text) as Record<string, unknown>;
      const value = parsed[key];
      return typeof value === 'number' ? value : 0;
    } catch {
      return 0;
    }
  };

/** cancelSchemaDiffDeploy：HTTP 回 `{ cancelled }`，域形状是裸 boolean。 */
const cancelledFieldDecode: RouteDecoder = (_payload, _response, raw): unknown => {
  const text = new TextDecoder().decode(raw).trim();
  if (text.length === 0) return false;
  try {
    const parsed = JSON.parse(text) as Record<string, unknown>;
    return parsed['cancelled'] === true;
  } catch {
    return false;
  }
};

/** clearQueryHistory：HTTP 回 `{ purged }`，域形状是 void（丢弃包装）。 */
const voidDecode: RouteDecoder = (): unknown => undefined;

/**
 * verifySchemaDiffJobRecovery：jobId 进路径，体只留目标会话 + 令牌
 * （契约 §6.13：`{ targetDbSessionId } & SubmitRequest`）。
 */
const recoveryVerificationBody = (payload: unknown): unknown => ({
  targetDbSessionId: field(payload, 'targetDbSessionId'),
  submissionToken: field(payload, 'submissionToken'),
});

/**
 * 流式驱动命令：`onEvent` 通道由传输替换（契约 §6.9）——web 形态事件经统一
 * 事件端点投递，Tauri Channel 不上 wire，其余字段（嵌套请求、
 * `applyResultLimit`/`recordHistory`）随体透传。
 */
const dropOnEventBody = (payload: unknown): unknown => {
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) return payload;
  const { onEvent: _replacedBySse, ...rest } = payload as Record<string, unknown>;
  return rest;
};

/* ------------------------------------------------------------------ */
/* 查询串构造。                                                          */
/* ------------------------------------------------------------------ */

/** listJobs：过滤包平铺为简单标量查询参数（概要 §6.3：GET /api/v1/jobs）。 */
const listJobsQuery = (payload: unknown): string => {
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) return '';
  return scalarQuery(Object.entries(payload as Record<string, unknown>));
};

/** named-view 的 scope 四元组（契约 §3 规则 1 点名的合法查询参数）。 */
const SCOPE_KEYS = ['connectionId', 'database', 'schema', 'table'] as const;

const listNamedViewsQuery = (payload: unknown): string => {
  const scope = field(payload, 'scope');
  if (typeof scope !== 'object' || scope === null) return '';
  return scalarQuery(SCOPE_KEYS.map((key) => [key, field(scope, key)] as const));
};

const driverCommandsQuery = (payload: unknown): string =>
  scalarQuery([['driverType', field(payload, 'driverType')]]);

/** listQueryFavorites：可选 connectionId 过滤（契约 §6.12）。 */
const queryFavoritesQuery = (payload: unknown): string =>
  scalarQuery([['connectionId', field(payload, 'connectionId')]]);

/** getQueryHistoryPage：过滤平铺为查询参数（契约 §6.12；listJobs 同款）。 */
const QUERY_HISTORY_KEYS = [
  'limit',
  'connectionId',
  'database',
  'schema',
  'search',
  'since',
  'until',
  'order',
] as const;

const queryHistoryPageQuery = (payload: unknown): string => {
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) return '';
  return scalarQuery(QUERY_HISTORY_KEYS.map((key) => [key, field(payload, key)] as const));
};

/** listSchemaDiffJobs：states 数组逗号串接（服务端 `states=a,b` 口径）+ limit。 */
const schemaDiffJobsQuery = (payload: unknown): string => {
  const states = field(payload, 'states');
  const limit = field(payload, 'limit');
  const entries: Array<readonly [string, unknown]> = [];
  if (Array.isArray(states) && states.length > 0) {
    entries.push(['states', states.join(',')]);
  }
  if (typeof limit === 'number') entries.push(['limit', limit]);
  return scalarQuery(entries);
};

/* ------------------------------------------------------------------ */
/* W8 刀3c/3d/3e web 收口（workflow / dashboard / dataSync）。             */
/* ------------------------------------------------------------------ */

/** workflow：定义与运行历史的 {id}（payload 字段名 = IPC 参数名 workflowId）。 */
const pathWorkflowId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'workflowId')));

/** dashboard：{id} 取 payload 的 dashboardId（createWidgetFrom* 的路径副本）。 */
const pathDashboardId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'dashboardId')));

/** workflow 运行历史：`?workflowId=`（缺席 = 全量，§6.14）。 */
const workflowRunsQuery = (payload: unknown): string =>
  scalarQuery([['workflowId', field(payload, 'workflowId')]]);

/** saveWorkflow：`{ workflow, submissionToken }` → 契约平铺体（saveSchemaDiffProfile 同款）。 */
const saveWorkflowBody = (payload: unknown): unknown => ({
  ...((field(payload, 'workflow') as Record<string, unknown> | undefined) ?? {}),
  submissionToken: field(payload, 'submissionToken'),
});

/** executeWorkflow：workflowId 进路径，体 = 其余字段 + 令牌。 */
const executeWorkflowBody = (payload: unknown): unknown => {
  if (typeof payload !== 'object' || payload === null || Array.isArray(payload)) return payload;
  const { workflowId: _inPath, ...rest } = payload as Record<string, unknown>;
  return rest;
};

/** getWorkflowYaml：HTTP 回 `{ yaml }` 标量包装，域形状 = 裸 string。 */
const yamlFieldDecode: RouteDecoder = (_payload, _response, raw): unknown => {
  const text = new TextDecoder().decode(raw).trim();
  try {
    const parsed = JSON.parse(text) as Record<string, unknown>;
    return typeof parsed['yaml'] === 'string' ? parsed['yaml'] : '';
  } catch {
    return '';
  }
};

/** dashboard：save 的 `{ dashboard, submissionToken }` → 契约平铺体。 */
const saveDashboardBody = (payload: unknown): unknown => ({
  ...((field(payload, 'dashboard') as Record<string, unknown> | undefined) ?? {}),
  submissionToken: field(payload, 'submissionToken'),
});

/** setDashboardRefreshPaused：id 进路径，体 = `{ paused }` + 令牌（§6.15）。 */
const refreshPausedBody = (payload: unknown): unknown => ({
  paused: field(payload, 'paused'),
  submissionToken: field(payload, 'submissionToken'),
});

/**
 * createWidgetFrom* / updateHiddenWidgetSql：`{ params, submissionToken }` →
 * params 平铺 + 令牌（契约 `CreateWidgetFromSqlParams & SubmitRequest` 同形）。
 * createWidgetFrom* 的顶层 dashboardId 是 web 路径插值副本，不进体。
 */
const widgetParamsBody = (payload: unknown): unknown => ({
  ...((field(payload, 'params') as Record<string, unknown> | undefined) ?? {}),
  submissionToken: field(payload, 'submissionToken'),
});

/** authorizeMonitorSchedule：ids 进路径，体 = `{ ttlSecs }` + 令牌。 */
const authorizeMonitorScheduleBody = (payload: unknown): unknown => ({
  ttlSecs: field(payload, 'ttlSecs'),
  submissionToken: field(payload, 'submissionToken'),
});

/** dashboard 组件族的复合路径前缀：`/dashboards/{id}/widgets/{widgetId}`。 */
const dashboardWidgetPrefix = (payload: unknown): string =>
  `/dashboards/${encodeURIComponent(String(requireField(payload, 'dashboardId')))}` +
  `/widgets/${encodeURIComponent(String(requireField(payload, 'widgetId')))}`;

/** listWidgetRuns：`?limit=`（服务端缺省 20，§6.15）。 */
const widgetRunsQuery = (payload: unknown): string =>
  scalarQuery([['limit', field(payload, 'limit')]]);

/** dataSync：任务/档案的 {id}（payload 字段名 = IPC 参数名）。 */
const pathSyncTaskId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'taskId')));
const pathSyncProfileId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'profileId')));
const pathMigrationRunId = (payload: unknown): string =>
  encodeURIComponent(String(requireField(payload, 'runId')));

/** saveSyncProfile：`{ profile, submissionToken }` → 契约平铺体。 */
const saveSyncProfileBody = (payload: unknown): unknown => ({
  ...((field(payload, 'profile') as Record<string, unknown> | undefined) ?? {}),
  submissionToken: field(payload, 'submissionToken'),
});

/** listMigrationRuns：过滤平铺 + 分页（§6.16：GET /api/v1/migration-runs）。 */
const MIGRATION_RUN_FILTER_KEYS = ['operation', 'status', 'profileId', 'connectionId'] as const;

const migrationRunsQuery = (payload: unknown): string => {
  const entries: Array<readonly [string, unknown]> = [];
  const filter = field(payload, 'filter');
  if (typeof filter === 'object' && filter !== null && !Array.isArray(filter)) {
    for (const key of MIGRATION_RUN_FILTER_KEYS) {
      const value = field(filter, key);
      if (value !== undefined && value !== null) entries.push([key, value]);
    }
  }
  const offset = field(payload, 'offset');
  const limit = field(payload, 'limit');
  if (typeof offset === 'number') entries.push(['offset', offset]);
  if (typeof limit === 'number') entries.push(['limit', limit]);
  return scalarQuery(entries);
};

/**
 * verifyDataSyncRecovery：jobId 进路径，体 = 复核请求本体（会话对 +
 * 库/schema；路径上的 jobId 不重复进体，§6.16）。
 */
const syncRecoveryBody = (payload: unknown): unknown => {
  const request = field(payload, 'request');
  if (typeof request !== 'object' || request === null || Array.isArray(request)) return {};
  const { jobId: _inPath, ...rest } = request as Record<string, unknown>;
  return rest;
};

const connectionCommandsQuery = (payload: unknown): string =>
  scalarQuery([['dbSessionId', field(payload, 'dbSessionId')]]);

/* ------------------------------------------------------------------ */
/* readArtifact：三模式互斥的查询参数 + 双模式响应解码（§6.5 / team-server §11）。 */
/* ------------------------------------------------------------------ */

const readArtifactQuery = (payload: unknown): string => {
  // metadata 模式：契约 §6.5 的 `?metadata=1`。types/ 的 ArtifactReadRequest
  // 联合尚未登记该模式（见报告的不一致清单）；这里按「payload.metadata === true」
  // 接受，不破坏既有两种形状。
  if (field(payload, 'metadata') === true) return '?metadata=1';
  const chunkIndex = field(payload, 'chunkIndex');
  if (chunkIndex !== undefined) {
    return `?chunkIndex=${encodeURIComponent(String(chunkIndex))}`;
  }
  const offset = field(payload, 'offset');
  const limit = field(payload, 'limit');
  if (offset !== undefined && limit !== undefined) {
    return `?offset=${encodeURIComponent(String(offset))}&limit=${encodeURIComponent(String(limit))}`;
  }
  throw new ApiError(
    'InvalidArgument',
    'readArtifact requires chunkIndex or offset+limit (contract §6.5: the three read modes are mutually exclusive).',
  );
};

/** 信封补充字段的服务端响应头（WR-D2 落地前按 team-server §11 语义约定的键名）。 */
const ARTIFACT_HEADER_TOTAL_CHUNKS = 'x-dz-total-chunks';
const ARTIFACT_HEADER_PUBLISHED_CHUNKS = 'x-dz-published-chunk-count';
const ARTIFACT_HEADER_PUBLISHED_BYTES = 'x-dz-published-byte-length';
const ARTIFACT_HEADER_COMPLETENESS = 'x-dz-result-completeness';

function headerCounter(response: Response, name: string): number | null {
  const value = response.headers.get(name);
  if (value === null) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : null;
}

function headerCompleteness(response: Response): 'pending' | 'complete' | 'truncated' {
  const value = response.headers.get(ARTIFACT_HEADER_COMPLETENESS);
  return value === 'pending' || value === 'truncated' ? value : 'complete';
}

function parseJsonBytes(raw: Uint8Array): unknown {
  return JSON.parse(new TextDecoder().decode(raw)) as unknown;
}

/**
 * readArtifact 解码（§6.5：体是原始字节，元数据模式返回 JSON）。
 *
 * - JSON 响应：元数据模式（或服务端选择回信封 JSON）——`bytes` 数组按桌面
 *   传输同款归一为 `Uint8Array`；
 * - 原始字节响应：chunk/offset 模式——信封回显请求定位 + 响应头携带发布
 *   元数据（键名见上，随 WR-D2 落地复核）。
 */
const readArtifactDecode: RouteDecoder = (payload, response, raw) => {
  const contentType = response.headers.get('content-type') ?? '';
  if (contentType.includes('json')) {
    const parsed = parseJsonBytes(raw);
    if (typeof parsed === 'object' && parsed !== null && Array.isArray(field(parsed, 'bytes'))) {
      const chunk = parsed as Record<string, unknown>;
      chunk['bytes'] = new Uint8Array(chunk['bytes'] as number[]);
    }
    return parsed;
  }
  const chunkIndex = field(payload, 'chunkIndex');
  const offset = field(payload, 'offset');
  return {
    artifactId: String(requireField(payload, 'artifactId')),
    // offset+limit 模式没有 chunk 定位；wire 未钉该模式下的 chunkIndex 语义
    // （WR-D2 复核项），此处回显 0 仅保证信封形状完整。
    chunkIndex: chunkIndex !== undefined ? Number(chunkIndex) : 0,
    totalChunks: headerCounter(response, ARTIFACT_HEADER_TOTAL_CHUNKS),
    publishedChunkCount: headerCounter(response, ARTIFACT_HEADER_PUBLISHED_CHUNKS) ?? undefined,
    publishedByteLength: headerCounter(response, ARTIFACT_HEADER_PUBLISHED_BYTES) ?? undefined,
    offset: offset !== undefined ? Number(offset) : 0,
    bytes: raw,
    resultCompleteness: headerCompleteness(response),
  };
};

/* ------------------------------------------------------------------ */
/* 分发表本体：108 条分发条目覆盖 112 个 call 方法中的 108 个           */
/* （open_base64_with_dialog 排除；PATCH 一条路由双服务）。            */
/* 键受 keyof MethodMap 约束（Partial），错名/多写编译期即失败。          */
/* ------------------------------------------------------------------ */

export const HTTP_ROUTES: Partial<Record<keyof MethodMap, RouteSpec>> = {
  // ── settings（§6.1）───────────────────────────────────────────────
  getSettings: { httpMethod: 'GET', pathTemplate: '/settings', body: noBody },
  saveSettings: { httpMethod: 'PUT', pathTemplate: '/settings' },

  // ── identity（§6.2）───────────────────────────────────────────────
  getPlatformIdentity: { httpMethod: 'GET', pathTemplate: '/identity', body: noBody },
  issueSubmissionToken: { httpMethod: 'POST', pathTemplate: '/submission-tokens' },

  // ── connection（§6.3；PATCH 一条路由双服务）────────────────────────
  listConnections: { httpMethod: 'GET', pathTemplate: '/connections', body: noBody },
  createConnection: { httpMethod: 'POST', pathTemplate: '/connections' },
  updateConnection: {
    httpMethod: 'PATCH',
    pathTemplate: '/connections/{id}',
    pathId: pathConnectionId,
    body: updateConnectionBody,
  },
  disableConnection: {
    httpMethod: 'PATCH',
    pathTemplate: '/connections/{id}',
    pathId: pathConnectionId,
    body: disableConnectionBody,
  },

  // ── session（§6.4；{id} = handle.dbSessionId）──────────────────────
  openSession: { httpMethod: 'POST', pathTemplate: '/sessions' },
  getSession: {
    httpMethod: 'GET',
    pathTemplate: '/sessions/{id}',
    pathId: pathSessionId,
    body: noBody,
  },
  setSessionContext: {
    httpMethod: 'PUT',
    pathTemplate: '/sessions/{id}/context',
    pathId: pathSessionId,
  },
  attachSession: {
    httpMethod: 'POST',
    pathTemplate: '/sessions/{id}/attach',
    pathId: pathSessionId,
  },
  detachSession: {
    httpMethod: 'POST',
    pathTemplate: '/sessions/{id}/detach',
    pathId: pathSessionId,
  },
  closeSession: {
    httpMethod: 'POST',
    pathTemplate: '/sessions/{id}/close',
    pathId: pathSessionId,
  },

  // ── execution（概要 §6.3 骨架；异步接受 202 + 稳定 ID，team-server §9.3）──
  executeInSession: {
    httpMethod: 'POST',
    pathTemplate: '/sessions/{id}/executions',
    pathId: pathSessionId,
  },
  executeAtTarget: { httpMethod: 'POST', pathTemplate: '/executions' },
  getExecution: {
    httpMethod: 'GET',
    pathTemplate: '/executions/{id}',
    pathId: pathExecutionId,
    body: noBody,
  },
  cancelExecution: {
    httpMethod: 'POST',
    pathTemplate: '/executions/{id}/cancel',
    pathId: pathExecutionId,
    body: submissionTokenBody,
  },

  // ── job（概要 §6.3 骨架；listJobs 平铺过滤走查询参数）───────────────
  startJob: { httpMethod: 'POST', pathTemplate: '/jobs' },
  listJobs: {
    httpMethod: 'GET',
    pathTemplate: '/jobs',
    query: listJobsQuery,
    body: noBody,
  },
  getJob: { httpMethod: 'GET', pathTemplate: '/jobs/{id}', pathId: pathJobId, body: noBody },
  cancelJob: {
    httpMethod: 'POST',
    pathTemplate: '/jobs/{id}/cancel',
    pathId: pathJobId,
    body: submissionTokenBody,
  },

  // ── artifact（§6.5；uploads 单请求信封 + 三模式互斥查询参数 + 双模式解码）
  createArtifactUpload: {
    httpMethod: 'POST',
    // §6.5 在账端点（wave1 server 定形）：单请求 begin+append+finalize，
    // 请求体即 v1 envelope `{ executionId, targetFingerprint, ttlSeconds?,
    // payloadBase64, expectedSha256? }`，201 → `{ artifactId }`。体 = payload
    // 原样（信封已在域客户端拼好，这里不做二次构造）。
    pathTemplate: '/artifacts/uploads',
  },
  readArtifact: {
    httpMethod: 'GET',
    pathTemplate: '/artifacts/{id}',
    pathId: pathArtifactId,
    query: readArtifactQuery,
    body: noBody,
    decode: readArtifactDecode,
  },

  // ── tableData：读与预览（§6.7，POST 读、查询文档体）────────────────
  getTableData: { httpMethod: 'POST', pathTemplate: '/table-data/reads' },
  previewTableReadSql: { httpMethod: 'POST', pathTemplate: '/table-data/read-sql-previews' },
  estimateTableRows: { httpMethod: 'POST', pathTemplate: '/table-data/row-count-estimates' },
  resolveForeignKeyTargets: {
    httpMethod: 'POST',
    pathTemplate: '/table-data/foreign-key-targets',
  },
  listInboundForeignKeys: {
    httpMethod: 'POST',
    pathTemplate: '/table-data/inbound-foreign-keys',
  },
  searchTableValues: { httpMethod: 'POST', pathTemplate: '/table-data/value-searches' },
  resolveResultOrigin: { httpMethod: 'POST', pathTemplate: '/table-data/result-origins' },

  // ── tableData：变更（§6.8，计划-确认两段式；提交同步回执 200）────────
  previewPendingChanges: { httpMethod: 'POST', pathTemplate: '/table-data/change-plan-previews' },
  commitPendingChanges: { httpMethod: 'POST', pathTemplate: '/table-data/change-commits' },
  // 200 + 空体（204 语义，契约 §6.8 v1 裁决）→ 空体分支统一归 undefined。
  commitQueryResultCell: { httpMethod: 'POST', pathTemplate: '/table-data/result-cell-commits' },
  previewResultCellChange: {
    httpMethod: 'POST',
    pathTemplate: '/table-data/result-cell-change-previews',
  },

  // ── tableData：命名视图（§6.9；DELETE 是 v1 面唯一删除路由）─────────
  listNamedViews: {
    httpMethod: 'GET',
    pathTemplate: '/named-views',
    query: listNamedViewsQuery,
    body: noBody,
  },
  saveNamedView: { httpMethod: 'PUT', pathTemplate: '/named-views' },
  deleteNamedView: {
    httpMethod: 'DELETE',
    pathTemplate: '/named-views/{id}',
    pathId: pathNamedViewId,
    body: noBody,
  },

  // ── driverGateway（§6.9 过渡域；open_base64_with_dialog 排除不上表）──
  get_driver_commands: {
    httpMethod: 'GET',
    pathTemplate: '/driver-gateway/commands',
    query: driverCommandsQuery,
    body: noBody,
  },
  get_connection_commands: {
    httpMethod: 'GET',
    pathTemplate: '/driver-gateway/commands',
    query: connectionCommandsQuery,
    body: noBody,
  },
  execute_driver_command: { httpMethod: 'POST', pathTemplate: '/driver-gateway/executions' },
  execute_driver_command_stream: {
    httpMethod: 'POST',
    pathTemplate: '/driver-gateway/stream-executions',
    body: dropOnEventBody,
  },

  // ── queryAssets（§6.12；W8 刀3a web 收口）────────────────────────────
  // 归属 = per (organization, principal)：请求无归属字段（INV-01），服务端
  // 从认证链取。写面体 = 业务字段 + submissionToken（payload 原样）；响应
  // 差异（{deleted}/{purged} 包装）由 decoder 归一到桌面域形状。
  listQueryFavorites: {
    httpMethod: 'GET',
    pathTemplate: '/query-favorites',
    query: queryFavoritesQuery,
    body: noBody,
  },
  createQueryFavorite: { httpMethod: 'POST', pathTemplate: '/query-favorites' },
  deleteQueryFavorite: {
    httpMethod: 'DELETE',
    pathTemplate: '/query-favorites/{id}',
    pathId: pathFavoriteId,
    body: submissionTokenBody,
  },
  getQueryHistoryPage: {
    httpMethod: 'GET',
    pathTemplate: '/query-history',
    query: queryHistoryPageQuery,
    body: noBody,
  },
  deleteQueryHistory: {
    httpMethod: 'DELETE',
    pathTemplate: '/query-history/{id}',
    pathId: pathHistoryEntryId,
    body: submissionTokenBody,
    decode: numberFieldOf('deleted'),
  },
  clearQueryHistory: {
    httpMethod: 'DELETE',
    pathTemplate: '/query-history',
    body: submissionTokenBody,
    decode: voidDecode,
  },
  purgeHistory: {
    httpMethod: 'POST',
    pathTemplate: '/query-history/purge',
    decode: numberFieldOf('purged'),
  },

  // ── schemaDiff（§6.13；W8 刀3b web 收口）────────────────────────────
  // profiles 是组织级资产（归属 = organization）；save 的桌面域形状
  // `{ profile, submissionToken }` 在 body 构造器里展开成 HTTP 平铺体。
  // cancel 的 {cancelled} 包装由 decoder 拆封。prepare/execute 的体 =
  // payload 原样（令牌已折在域客户端 payload 里）。
  listSchemaDiffProfiles: {
    httpMethod: 'GET',
    pathTemplate: '/schema-diff/profiles',
    body: noBody,
  },
  saveSchemaDiffProfile: {
    httpMethod: 'PUT',
    pathTemplate: '/schema-diff/profiles',
    body: saveSchemaDiffProfileBody,
  },
  deleteSchemaDiffProfile: {
    httpMethod: 'DELETE',
    pathTemplate: '/schema-diff/profiles/{id}',
    pathId: pathSchemaDiffProfileId,
    body: submissionTokenBody,
  },
  compareTableSchemas: {
    httpMethod: 'POST',
    pathTemplate: '/schema-diff/comparisons',
  },
  prepareSchemaDiffPlan: {
    httpMethod: 'POST',
    pathTemplate: '/schema-diff/plans',
  },
  prepareSchemaUnifiedPlan: {
    httpMethod: 'POST',
    pathTemplate: '/schema-diff/unified-plans',
  },
  executeSchemaDiffDeploy: {
    httpMethod: 'POST',
    pathTemplate: '/schema-diff/deploys',
  },
  listSchemaDiffJobs: {
    httpMethod: 'GET',
    pathTemplate: '/schema-diff/jobs',
    query: schemaDiffJobsQuery,
    body: noBody,
  },
  getSchemaDiffJobDetails: {
    httpMethod: 'GET',
    pathTemplate: '/schema-diff/jobs/{id}',
    pathId: pathSchemaDiffJobId,
    body: noBody,
  },
  cancelSchemaDiffDeploy: {
    httpMethod: 'POST',
    pathTemplate: '/schema-diff/jobs/{id}/cancel',
    pathId: pathSchemaDiffJobId,
    body: submissionTokenBody,
    decode: cancelledFieldDecode,
  },
  verifySchemaDiffJobRecovery: {
    httpMethod: 'POST',
    pathTemplate: '/schema-diff/jobs/{id}/recovery-verification',
    pathId: pathSchemaDiffJobId,
    body: recoveryVerificationBody,
  },

  // ── workflow（§6.14；W8 刀3c web 收口）────────────────────────────
  // 定义管理 + 同步执行 + 运行历史。getWorkflowYaml 的 `{ yaml }` 与
  // clearWorkflowRuns 的 `{ cleared }` 由 decoder 拆封；DELETE 面
  // （delete / clear-runs）无体无令牌（§6.14 裁决）；save/save-yaml/
  // execute 的令牌已折在域客户端 payload 里。
  listWorkflows: {
    httpMethod: 'GET',
    pathTemplate: '/workflows',
    body: noBody,
  },
  saveWorkflow: {
    httpMethod: 'PUT',
    pathTemplate: '/workflows/{id}',
    pathId: (payload) =>
      encodeURIComponent(String(requireField(requireField(payload, 'workflow'), 'id'))),
    body: saveWorkflowBody,
  },
  saveWorkflowYaml: {
    httpMethod: 'POST',
    pathTemplate: '/workflows/from-yaml',
  },
  getWorkflow: {
    httpMethod: 'GET',
    pathTemplate: '/workflows/{id}',
    pathId: pathWorkflowId,
    body: noBody,
  },
  getWorkflowYaml: {
    httpMethod: 'GET',
    pathTemplate: '/workflows/{id}/yaml',
    pathId: pathWorkflowId,
    body: noBody,
    decode: yamlFieldDecode,
  },
  deleteWorkflow: {
    httpMethod: 'DELETE',
    pathTemplate: '/workflows/{id}',
    pathId: pathWorkflowId,
    body: noBody,
  },
  executeWorkflow: {
    httpMethod: 'POST',
    pathTemplate: '/workflows/{id}/execute',
    pathId: pathWorkflowId,
    body: executeWorkflowBody,
  },
  reloadWorkflows: {
    httpMethod: 'POST',
    pathTemplate: '/workflows/reload',
    body: noBody,
  },
  listWorkflowRuns: {
    httpMethod: 'GET',
    pathTemplate: '/workflow-runs',
    query: workflowRunsQuery,
    body: noBody,
  },
  getWorkflowRun: {
    httpMethod: 'GET',
    pathTemplate: '/workflow-runs/{id}',
    pathId: (payload) => encodeURIComponent(String(requireField(payload, 'historyId'))),
    body: noBody,
  },
  clearWorkflowRuns: {
    httpMethod: 'DELETE',
    pathTemplate: '/workflow-runs',
    query: workflowRunsQuery,
    body: noBody,
    decode: numberFieldOf('cleared'),
  },

  // ── dashboard（§6.15；W8 刀3d web 收口）─────────────────────────
  // save 的 `{ dashboard }` 袋在 body 构造器展开；组件族三段 id 走
  // buildPath；PATCH 面（refresh-paused）是本表第二个 PATCH。
  listDashboards: {
    httpMethod: 'GET',
    pathTemplate: '/dashboards',
    body: noBody,
  },
  getDashboard: {
    httpMethod: 'GET',
    pathTemplate: '/dashboards/{id}',
    pathId: pathNamedViewId,
    body: noBody,
  },
  saveDashboard: {
    httpMethod: 'PUT',
    pathTemplate: '/dashboards/{id}',
    pathId: (payload) =>
      encodeURIComponent(String(requireField(requireField(payload, 'dashboard'), 'id'))),
    body: saveDashboardBody,
  },
  deleteDashboard: {
    httpMethod: 'DELETE',
    pathTemplate: '/dashboards/{id}',
    pathId: pathNamedViewId,
    body: noBody,
  },
  setDashboardRefreshPaused: {
    httpMethod: 'PATCH',
    pathTemplate: '/dashboards/{id}/refresh-paused',
    pathId: pathNamedViewId,
    body: refreshPausedBody,
  },
  findDashboardWorkflowRefs: {
    httpMethod: 'GET',
    pathTemplate: '/dashboards/workflow-refs',
    query: workflowRunsQuery,
    body: noBody,
  },
  createWidgetFromSql: {
    httpMethod: 'POST',
    pathTemplate: '/dashboards/{id}/widgets/from-sql',
    pathId: pathDashboardId,
    body: widgetParamsBody,
  },
  createWidgetFromWorkflow: {
    httpMethod: 'POST',
    pathTemplate: '/dashboards/{id}/widgets/from-workflow',
    pathId: pathDashboardId,
    body: widgetParamsBody,
  },
  updateHiddenWidgetSql: {
    httpMethod: 'POST',
    pathTemplate: '/dashboards/hidden-widget-sql',
    body: widgetParamsBody,
  },
  runDashboardWidget: {
    httpMethod: 'POST',
    pathTemplate: '/dashboards/{id}/widgets/{widgetId}/run',
    buildPath: (payload) => `${dashboardWidgetPrefix(payload)}/run`,
    body: submissionTokenBody,
  },
  listWidgetRuns: {
    httpMethod: 'GET',
    pathTemplate: '/dashboards/{id}/widgets/{widgetId}/runs',
    buildPath: (payload) => `${dashboardWidgetPrefix(payload)}/runs`,
    query: widgetRunsQuery,
    body: noBody,
  },
  getWidgetRun: {
    httpMethod: 'GET',
    pathTemplate: '/dashboards/{id}/widgets/{widgetId}/runs/{runId}',
    buildPath: (payload) =>
      `${dashboardWidgetPrefix(payload)}/runs/${encodeURIComponent(
        String(requireField(payload, 'runId')),
      )}`,
    body: noBody,
  },
  getMonitorScheduleGrant: {
    httpMethod: 'GET',
    pathTemplate: '/dashboards/{id}/widgets/{widgetId}/schedule-grant',
    buildPath: (payload) => `${dashboardWidgetPrefix(payload)}/schedule-grant`,
    body: noBody,
  },
  authorizeMonitorSchedule: {
    httpMethod: 'POST',
    pathTemplate: '/dashboards/{id}/widgets/{widgetId}/schedule-grant',
    buildPath: (payload) => `${dashboardWidgetPrefix(payload)}/schedule-grant`,
    body: authorizeMonitorScheduleBody,
  },
  revokeMonitorSchedule: {
    httpMethod: 'DELETE',
    pathTemplate: '/dashboards/{id}/widgets/{widgetId}/schedule-grant',
    buildPath: (payload) => `${dashboardWidgetPrefix(payload)}/schedule-grant`,
    body: noBody,
  },

  // ── dataSync（§6.16，v1.9；W8 刀3e web 收口）────────────────────
  // saveSyncProfile 的 `{ profile }` 袋展开；cancel 的 `{ cancelled }` 由
  // decoder 拆封；迁移历史是顶层资源 /migration-runs；执行面九方法体 =
  // payload 原样（令牌缺席是有意裁决——幂等由受理键/纯读承担）。
  getSyncTasks: {
    httpMethod: 'GET',
    pathTemplate: '/sync/tasks',
    body: noBody,
  },
  deleteSyncTask: {
    httpMethod: 'DELETE',
    pathTemplate: '/sync/tasks/{id}',
    pathId: pathSyncTaskId,
    body: noBody,
  },
  getSyncProfiles: {
    httpMethod: 'GET',
    pathTemplate: '/sync/profiles',
    body: noBody,
  },
  saveSyncProfile: {
    httpMethod: 'PUT',
    pathTemplate: '/sync/profiles',
    body: saveSyncProfileBody,
  },
  deleteSyncProfile: {
    httpMethod: 'DELETE',
    pathTemplate: '/sync/profiles/{id}',
    pathId: pathSyncProfileId,
    body: noBody,
  },
  classifyDataSyncPair: {
    httpMethod: 'POST',
    pathTemplate: '/sync/pairings',
  },
  listMigrationRuns: {
    httpMethod: 'GET',
    pathTemplate: '/migration-runs',
    query: migrationRunsQuery,
    body: noBody,
  },
  getMigrationRun: {
    httpMethod: 'GET',
    pathTemplate: '/migration-runs/{id}',
    pathId: pathMigrationRunId,
    body: noBody,
  },
  listDataSyncJobs: {
    httpMethod: 'GET',
    pathTemplate: '/sync/jobs',
    body: noBody,
  },
  getDataSyncJob: {
    httpMethod: 'GET',
    pathTemplate: '/sync/jobs/{id}',
    pathId: pathJobId,
    body: noBody,
  },
  getDataSyncJobDetails: {
    httpMethod: 'GET',
    pathTemplate: '/sync/jobs/{id}/details',
    pathId: pathJobId,
    body: noBody,
  },
  getDataSyncJobPreview: {
    httpMethod: 'GET',
    pathTemplate: '/sync/jobs/{id}/preview',
    pathId: pathJobId,
    body: noBody,
  },
  cancelDataSync: {
    httpMethod: 'POST',
    pathTemplate: '/sync/jobs/{id}/cancel',
    pathId: pathJobId,
    body: submissionTokenBody,
    decode: cancelledFieldDecode,
  },
  inspectDataSync: {
    httpMethod: 'POST',
    pathTemplate: '/sync/inspections',
  },
  revalidateDataSync: {
    httpMethod: 'POST',
    pathTemplate: '/sync/revalidations',
  },
  compareDataSync: {
    httpMethod: 'POST',
    pathTemplate: '/sync/comparisons',
  },
  startDataSyncPrepareJob: {
    httpMethod: 'POST',
    pathTemplate: '/sync/jobs/prepare',
  },
  startDataSyncApplyJob: {
    httpMethod: 'POST',
    pathTemplate: '/sync/jobs/apply',
  },
  executeDataSync: {
    httpMethod: 'POST',
    pathTemplate: '/sync/executions',
  },
  generateDataSyncSql: {
    httpMethod: 'POST',
    pathTemplate: '/sync/sql-preview',
  },
  checkSyncConflicts: {
    httpMethod: 'POST',
    pathTemplate: '/sync/tasks/{id}/conflicts',
    pathId: pathSyncTaskId,
    body: noBody,
  },
  verifyDataSyncRecovery: {
    httpMethod: 'POST',
    pathTemplate: '/sync/jobs/{id}/recovery-verification',
    // jobId 在 `{ request }` 袋里（桌面 IPC 参数形），路径插值穿透一层。
    pathId: (payload) =>
      encodeURIComponent(String(requireField(requireField(payload, 'request'), 'jobId'))),
    body: syncRecoveryBody,
  },
};

/* ------------------------------------------------------------------ */
/* WR-F1 扩展账（三缺口合拢）：mcp / security / aiHost 的八条契约路由。     */
/* ------------------------------------------------------------------ */

/**
 * WR-F1 的**契约键**（点分域前缀；与 `keyof MethodMap` 的 camelCase 内核键、
 * snake_case legacy 键零重名面 —— `webUnsupportedMethods.ts` 无冲突可登记，
 * 键也不在 `unsupportedMethods` 声明面上：wire-direct 域不经 guard/transport.call）。
 *
 * 这些键**不是** `MethodMap` 键：`MethodMap` 对本刀冻结（交集在 transport.ts、
 * 门面覆盖证明在 client.ts，均在白名单之外），三个域因此走注入式 HTTP wire
 * 的直连域客户端（`packages/backend-client/src/{mcpClient,securityClient}.ts`）；
 * aiHost 两键的可执行面在 `ai/webAiAssembly.ts`。本表是「方法 → HTTP 形状」
 * 的登记台账，供契约文档 §6 扩展与线形复核；`mcpSecurityAssembly.test.ts`
 * 把台账逐键钉在实际出站请求上，两处不会漂移。
 */
export type WrF1RouteKey =
  | 'mcp.initialize'
  | 'mcp.listTools'
  | 'mcp.callTool'
  | 'mcp.closeSession'
  | 'security.resolveSecret'
  | 'security.putSecret'
  | 'aiHost.getSessionConfig'
  | 'aiHost.executeTarget';

/** WR-F1 台账条目：RouteSpec + 基路径声明（/mcp 面在服务端根，不在 /api/v1 之后）。 */
export interface WrF1RouteSpec extends RouteSpec {
  /** 路由基：`'/api/v1'`（§6 域面缺省）或 `'/'`（服务端根，/mcp 面）。 */
  readonly base: '/api/v1' | '/';
}

/** 轨 4-e 冻结契约的两段路径参数：{id} = 编码后的 domain + '/' + 编码后的 key。 */
const secretPairPathId = (payload: unknown): string =>
  `${encodeURIComponent(String(requireField(payload, 'domain')))}/${encodeURIComponent(
    String(requireField(payload, 'key')),
  )}`;

/** putSecret 的体只留 `{ value }`（domain/key 进路径，绝不在体里重复）。 */
const putSecretBody = (payload: unknown): unknown => ({
  value: field(payload, 'value'),
});

/**
 * 八条契约路由（冻结形状，与两轨逐字一致）：
 *
 * | 键 | 形状 | 依据 |
 * | mcp.*（4 条，base `/`） | JSON-RPC 2.0 单条消息 POST /mcp；initialize 的响应头 `Mcp-Session-Id` 建会话、closeSession = DELETE /mcp | crates/datazen-server/src/mcp/http.rs + wire.rs |
 * | security.resolveSecret | POST /api/v1/secrets/resolve，体 `{domain,key}` → `{value}` | 轨 4-e 冻结契约 |
 * | security.putSecret | PUT /api/v1/secrets/{domain}/{key}，体 `{value}` → 204 | 轨 4-e 冻结契约 |
 * | aiHost.getSessionConfig | POST /api/v1/ai-host/session-config，体 = payload 原样，响应 JSON | 轨 4-e 冻结契约（F1#3 的 TS 半边） |
 * | aiHost.executeTarget | POST /api/v1/ai-host/execute-target，体 = payload 原样（claim 进体），响应 JSON | 轨 4-e 冻结契约（F1#3 的 TS 半边） |
 *
 * mcp 四条的**体**是 mcpClient.ts 内拼装的 JSON-RPC 信封（自增 id、会话头），
 * RouteSpec 的 payload 直通表达不了信封与响应头读取——执行面真相在该客户端，
 * 本表只钉 method/path/base（`body` 缺省 ≠ 实际体，见表内注释）。
 */
export const WR_F1_HTTP_ROUTES: Readonly<Record<WrF1RouteKey, WrF1RouteSpec>> = {
  // ── mcp（服务端 /mcp 面；JSON-RPC 信封由 mcpClient 拼装，体直通仅示意）──
  'mcp.initialize': { httpMethod: 'POST', pathTemplate: '/mcp', base: '/' },
  'mcp.listTools': { httpMethod: 'POST', pathTemplate: '/mcp', base: '/' },
  'mcp.callTool': { httpMethod: 'POST', pathTemplate: '/mcp', base: '/' },
  'mcp.closeSession': { httpMethod: 'DELETE', pathTemplate: '/mcp', base: '/' },

  // ── security（轨 4-e 冻结契约：路径/体/响应逐字）────────────────────
  'security.resolveSecret': {
    httpMethod: 'POST',
    pathTemplate: '/secrets/resolve',
    base: '/api/v1',
  },
  'security.putSecret': {
    httpMethod: 'PUT',
    pathTemplate: '/secrets/{id}',
    pathId: secretPairPathId,
    body: putSecretBody,
    base: '/api/v1',
  },

  // ── aiHost（F1#3 的 TS 半边；路由由轨 4-e 落地，体 = payload 原样）────
  'aiHost.getSessionConfig': {
    httpMethod: 'POST',
    pathTemplate: '/ai-host/session-config',
    base: '/api/v1',
  },
  'aiHost.executeTarget': {
    httpMethod: 'POST',
    pathTemplate: '/ai-host/execute-target',
    base: '/api/v1',
  },
};
