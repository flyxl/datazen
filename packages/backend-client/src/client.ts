/**
 * 门面：`BackendClient`（21 个业务方法）+ `backendId` 注入登记。
 *
 * 门面**绑定单个 backend**：桌面本地与团队远端各持一个实例，业务代码只见方法，
 * 不见传输（§7.3）。`backendId` 只用于选择门面——它**不是业务方法参数**，也**不进
 * `ExecutionTarget`**（§7.1）；因此下面 21 个方法的签名里没有任何一个接收 backendId。
 *
 * 方法清单以概要 §6.2 的表为权威：
 * ConnectionClient 12 + ProfileClient 3 + JobClient 4 + ArtifactClient 1 +
 * SubmissionTokenClient 1 = 21。驱动 Command / schema 方法不在此表，它们留在
 * `@datazen/driver-sdk`（§7.2），共用同一个 `BackendTransport` 但另有门面。
 *
 * **未注入即抛错**：绝不静默回退到 Tauri 直连（§7.4）。React 版 `useBackendClient()`
 * Hook 属于前端层，不在本包——本包零 React 依赖（门禁 F-05）。
 */

import { backendError, parseApiError, UNBOUND_MESSAGE, type BackendError } from './errors';
import { createPushEventStream, type ConnectionEventStream } from './streams';
import type {
  BackendTransport,
  MethodMap,
  MethodName,
  CreateConnectionRequest,
  DisableConnectionRequest,
  ExecutionRef,
  ListJobsRequest,
  SessionRef,
  UpdateConnectionRequest,
} from './transport';
import type {
  ArtifactChunk,
  AttachmentRequest,
  CancelReceipt,
  CloseReceipt,
  CloseSessionRequest,
  ConnectionEvent,
  ContextChangeReceipt,
  EventEnvelope,
  ExecutionReceipt,
  ExecutionView,
  ExecuteAtTargetRequest,
  ExecuteInSessionRequest,
  Id,
  IdempotentOperation,
  JobFilter,
  JobView,
  OpenSessionReceipt,
  OpenSessionRequest,
  ProfileView,
  ReadArtifactRequest,
  SessionHandle,
  SessionView,
  SetSessionContextRequest,
  StartJobRequest,
  SubmissionToken,
  SubscribeEventsRequest,
} from './types';

/**
 * 传输无关的业务门面。
 *
 * 与 `MethodMap` 一一对应：门面方法把易用参数（`getExecution(executionId)`）收敛成
 * `MethodMap` 的载荷对象（`{ executionId }`），传输适配器只需实现 `call` 一处。
 */
export interface BackendClient {
  // ---- ConnectionClient（12）----
  /** 列出当前 backend 下列出授权配置。本版不传分页参数。 */
  listConnections(): Promise<ProfileView[]>;
  /** 懒建逻辑会话；`attachmentToken` **只返回给 owner**。 */
  openSession(request: OpenSessionRequest): Promise<OpenSessionReceipt>;
  /** 读取当前 owner 的会话状态。 */
  getSession(handle: SessionHandle): Promise<SessionView>;
  /**
   * 会话内执行。返回**接受回执**，不表示 SQL 已成功——成功与否查 `getExecution`。
   * 失败既可能是 `ApiError`，也可能是 `ExecutionState='failed'` + `errorCode`。
   */
  executeInSession(request: ExecuteInSessionRequest): Promise<ExecutionReceipt>;
  /** 目标直执行。**不回填**会话默认值：请求没带的层级就是空的。 */
  executeAtTarget(request: ExecuteAtTargetRequest): Promise<ExecutionReceipt>;
  /** 原地确认上下文，或返回被替换的新 session（见 `ContextChangeReceipt`）。 */
  setSessionContext(request: SetSessionContextRequest): Promise<ContextChangeReceipt>;
  /** 原 owner 身份 + token 校验；**不延长业务空闲期**。 */
  attachSession(request: AttachmentRequest): Promise<SessionView>;
  /** 显式取消 attachment。 */
  detachSession(request: AttachmentRequest): Promise<SessionView>;
  /** 幂等关闭，**不默认提交事务**（`CloseMode` 决定）。 */
  closeSession(request: CloseSessionRequest): Promise<CloseReceipt>;
  /** 查询执行终态。 */
  getExecution(executionId: Id): Promise<ExecutionView>;
  /** 请求精确取消。返回**处置回执**（`requested` / `unsupported` / `alreadyFinished`）。 */
  cancelExecution(executionId: Id): Promise<CancelReceipt>;
  /**
   * 订阅连接事件流。断开订阅**不直接取消 SQL**（概要 §6.2）。
   *
   * 调用方结束迭代（`break` / `return()`）只结束订阅，**不触发 `cancelExecution`**（§7.3）。
   */
  subscribeEvents(request: SubscribeEventsRequest): ConnectionEventStream;

  // ---- ProfileClient（3）----
  /** 创建配置；凭据只交 SecretProvider，不回显。 */
  createConnection(request: CreateConnectionRequest): Promise<ProfileView>;
  /** CAS 更新（`expectedRevision` 不符回 `ConfigRevisionMismatch`）；**不改变已建会话**。 */
  updateConnection(request: UpdateConnectionRequest): Promise<ProfileView>;
  /** 禁止新执行；已有资源按 drain/force 政策处理。 */
  disableConnection(request: DisableConnectionRequest): Promise<ProfileView>;

  // ---- JobClient（4）----
  /** 持久化接受一个 Job。Job 生命周期独立于窗口。 */
  startJob(request: StartJobRequest): Promise<JobView>;
  /** 只返回当前 principal 授权的 Job。 */
  listJobs(filter?: JobFilter | null): Promise<JobView[]>;
  /** 任务状态。 */
  getJob(jobId: Id): Promise<JobView>;
  /** 取消 Job。返回处置回执。 */
  cancelJob(jobId: Id, idempotencyKey: string): Promise<CancelReceipt>;

  // ---- ArtifactClient（1）----
  /** 读产物块。每次授权、限大小、服从产物 TTL；**越界报错，不返回服务器路径**。 */
  readArtifact(request: ReadArtifactRequest): Promise<ArtifactChunk>;

  // ---- SubmissionTokenClient（1）----
  /** 签发限定调用身份、操作与有效期的幂等令牌。 */
  issueSubmissionToken(
    operation: IdempotentOperation,
    sessionHandle?: SessionHandle | null,
  ): Promise<SubmissionToken>;
}

/**
 * 把任意失败载荷整理成 `BackendError`。
 *
 * 能解析成 `ApiError` 的（业务拒绝）保留 `apiError`；不能的（传输崩溃、宿主未绑定）
 * 归为 `transport`/`unbound`。**未知 code 不会被折叠**：`parseApiError` 返回 `null`，
 * 于是原始值挂在 `cause` 上，调用方仍能读到服务端到底返回了什么。
 */
export function normalizeBackendError(error: unknown): BackendError {
  if (typeof error === 'object' && error !== null && 'kind' in error && 'apiError' in error) {
    return error as BackendError;
  }
  const apiError = parseApiError(error);
  if (apiError !== null) {
    return backendError(
      'transport',
      `Backend rejected the call with ${apiError.code}: ${apiError.message}`,
      apiError,
      error,
    );
  }
  const detail = error instanceof Error ? error.message : String(error);
  return backendError('transport', `Backend transport failed: ${detail}`, null, error);
}

/**
 * 用一个 `BackendTransport` 造出门面。桌面适配器与 Web 适配器各调一次。
 *
 * @param backendId 仅用于日志与错误信息定位，**不进入任何载荷**。
 */
export function createBackendClient(backendId: string, transport: BackendTransport): BackendClient {
  const call = async <K extends MethodName>(
    method: K,
    payload: MethodMap[K]['request'],
  ): Promise<MethodMap[K]['response']> => {
    try {
      return await transport.call(method, payload);
    } catch (error) {
      throw normalizeBackendError(error);
    }
  };

  const subscribeEvents = (request: SubscribeEventsRequest): ConnectionEventStream => {
    const subscribe = transport.subscribe;
    if (subscribe === undefined) {
      throw backendError(
        'transport',
        `Backend transport "${backendId}" does not implement subscribe(); event streaming is unavailable.`,
      );
    }
    let source: AsyncIterable<EventEnvelope<ConnectionEvent>>;
    try {
      source = subscribe.call(transport, 'subscribeEvents', request);
    } catch (error) {
      throw normalizeBackendError(error);
    }
    return bridgeSubscription(source);
  };

  return {
    listConnections: () => call('listConnections', null),
    openSession: (request) => call('openSession', request),
    getSession: (handle) => call('getSession', { handle } satisfies SessionRef),
    executeInSession: (request) => call('executeInSession', request),
    executeAtTarget: (request) => call('executeAtTarget', request),
    setSessionContext: (request) => call('setSessionContext', request),
    attachSession: (request) => call('attachSession', request),
    detachSession: (request) => call('detachSession', request),
    closeSession: (request) => call('closeSession', request),
    getExecution: (executionId) => call('getExecution', { executionId } satisfies ExecutionRef),
    cancelExecution: (executionId) =>
      call('cancelExecution', { executionId } satisfies ExecutionRef),
    subscribeEvents,

    createConnection: (request) => call('createConnection', request),
    updateConnection: (request) => call('updateConnection', request),
    disableConnection: (request) => call('disableConnection', request),

    startJob: (request) => call('startJob', request),
    listJobs: (filter = null) => call('listJobs', { filter } satisfies ListJobsRequest),
    getJob: (jobId) => call('getJob', { jobId }),
    cancelJob: (jobId, idempotencyKey) => call('cancelJob', { jobId, idempotencyKey }),

    readArtifact: (request) => call('readArtifact', request),

    issueSubmissionToken: (operation, sessionHandle = null) =>
      call('issueSubmissionToken', { operation, sessionHandle }),
  };
}

/**
 * 把传输的 `AsyncIterable` 桥接成可 `for await` 的推送队列。
 *
 * 收尾纪律（§7.3）：`onDetach` 只对源迭代器调 `return()`——**结束迭代只取消订阅，
 * 不触发 `cancelExecution`**。取消是一次业务动作，必须由调用方显式发起。
 */
function bridgeSubscription(
  source: AsyncIterable<EventEnvelope<ConnectionEvent>>,
): ConnectionEventStream {
  const iterator = source[Symbol.asyncIterator]();
  const stream = createPushEventStream<EventEnvelope<ConnectionEvent>>({
    onDetach: () => {
      const pending = iterator.return?.();
      if (pending !== undefined) void pending.then(undefined, () => undefined);
    },
  });
  void (async (): Promise<void> => {
    try {
      for (;;) {
        const next = await iterator.next();
        if (next.done === true) {
          stream.end();
          return;
        }
        // `push` 返回 false 表示流已结束（例如调用方已 `return()`），此时源迭代器已由
        // `onDetach` 释放，这里直接收工。
        if (!stream.push(next.value)) return;
      }
    } catch (error) {
      stream.fail(normalizeBackendError(error));
    }
  })();
  return stream;
}

// ---------------------------------------------------------------------------
// backendId 注入登记
// ---------------------------------------------------------------------------

/** 已注入的门面。键是 backendId。 */
const facades = new Map<string, BackendClient>();

/** 当前选中的 backendId。入口先逐个注入，再选一个。 */
let activeBackendId: string | null = null;

/**
 * 注入某个 backend 的门面。**必须在首次渲染前调用**（§7.3）。
 * 同时把它设为当前选中门面；之后切换 backend 只调 `selectBackend`，不重建注入。
 */
export function setBackendClient(backendId: string, client: BackendClient): void {
  facades.set(backendId, client);
  activeBackendId = backendId;
}

/** 切换当前门面。目标未注入时抛可定位错误，不静默回落。 */
export function selectBackend(backendId: string): BackendClient {
  const client = facades.get(backendId);
  if (client === undefined) {
    throw unboundError(backendId);
  }
  activeBackendId = backendId;
  return client;
}

/** 读某个已注入的门面；未注入返回 `null`。不抛错，用于「是否已就绪」的探测。 */
export function getBackendClient(backendId?: string): BackendClient | null {
  const id = backendId ?? activeBackendId;
  if (id === null) return null;
  return facades.get(id) ?? null;
}

/**
 * 取门面；未注入则抛 §7.4 的固定错误。
 *
 * 不传 `backendId` 时取当前选中门面。前端层的 React 包装（`useBackendClient()`）
 * 属于 `src/`，本包零 React 依赖（门禁 F-05）。
 */
export function requireBackendClient(backendId?: string): BackendClient {
  const id = backendId ?? activeBackendId;
  if (id === null) throw unboundError(backendId);
  const client = facades.get(id);
  if (client === undefined) throw unboundError(id);
  return client;
}

/** 当前选中的 backendId；未注入返回 `null`。 */
export function currentBackendId(): string | null {
  return activeBackendId;
}

/** 已注入的 backendId 列表。 */
export function boundBackendIds(): readonly string[] {
  return [...facades.keys()];
}

/** 清空登记。测试用；生产路径不会调它。 */
export function clearBackendClients(): void {
  facades.clear();
  activeBackendId = null;
}

/**
 * 未绑定错误。**文案固定**（§7.4 的样板），只在末尾追加 backendId 便于定位——
 * 追加不改变那句可搜索的样板语。
 */
function unboundError(backendId: string | null | undefined): BackendError {
  const suffix =
    backendId === null || backendId === undefined ? '' : ` (backendId: "${backendId}")`;
  return backendError('unbound', `${UNBOUND_MESSAGE}${suffix}`);
}
