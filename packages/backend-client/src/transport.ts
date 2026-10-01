/**
 * 传输契约：`MethodMap` / `BackendTransport` / `PlatformServices`。
 *
 * 逐字来自详细设计 §7.1。本文件**只声明方法与载荷形状**：不含任何 Tauri IPC 导入前缀
 * （`@tauri-apps` 命名空间），也不含浏览器取数 API（`fetch` / XHR）字面量。门禁 F-07 由 CI
 * 规则 `backend-client-transport-agnostic` 扫描 `packages/backend-client/src` 强制，
 * 因此**注释里也不得写出这些字面量**——本文件即遵守该纪律。
 *
 * **`backendId` 不出现**：它只用于选择门面，不作为业务方法参数，也不进入
 * `ExecutionTarget`（§7.1）。因此 `MethodMap` 的每个方法只描述业务载荷。
 *
 * 方法清单以概要 §6.2 的表为权威：**21 个**（ConnectionClient 12 + ProfileClient 3 +
 * JobClient 4 + ArtifactClient 1 + SubmissionTokenClient 1）。驱动 Command 与 schema 方法
 * 不在本表内——它们留在 `@datazen/driver-sdk`，由同一 `BackendTransport` 另行承载
 * （§7.2），本包不重复声明。
 */

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
  IssueSubmissionTokenRequest,
  JobFilter,
  JobView,
  OpenSessionReceipt,
  OpenSessionRequest,
  ProfileDraft,
  ProfilePatch,
  ProfileView,
  ReadArtifactRequest,
  SessionHandle,
  SessionView,
  SetSessionContextRequest,
  StartJobRequest,
  SubmissionToken,
  SubscribeEventsRequest,
} from './types';

/** 单个方法的请求/响应对应关系。 */
export interface MethodEntry<TRequest, TResponse> {
  readonly request: TRequest;
  readonly response: TResponse;
}

/** `getSession` 的载荷：按句柄读取。 */
export interface SessionRef {
  readonly handle: SessionHandle;
}

/** `getExecution` / `cancelExecution` 的载荷。 */
export interface ExecutionRef {
  readonly executionId: Id;
}

/** `getJob` 的载荷。 */
export interface JobRef {
  readonly jobId: Id;
}

/** `createConnection` 的载荷：草稿 + 只写凭据 + `createProfile` 签名令牌给出的幂等键。 */
export interface CreateConnectionRequest {
  readonly draft: ProfileDraft;
  /**
   * 只写凭据，**只交 SecretProvider，不回显**。
   * 与 `draft.credentials` 分开列出，是为了契约上明确「凭据不是草稿的可回显部分」。
   */
  readonly credentials: Readonly<Record<string, string>> | null;
  readonly idempotencyKey: string;
}

/** `updateConnection` 的载荷：CAS 更新。**不改变已建会话**。 */
export interface UpdateConnectionRequest {
  readonly connectionId: Id;
  /** 期望的配置版本。不一致回 `ConfigRevisionMismatch`。 */
  readonly expectedRevision: string;
  readonly patch: ProfilePatch;
}

/** `disableConnection` 的载荷：禁止新执行，已有资源按 drain/force 政策处理。 */
export interface DisableConnectionRequest {
  readonly connectionId: Id;
  readonly expectedRevision: string;
}

/** `cancelJob` 的载荷。 */
export interface CancelJobRequest {
  readonly jobId: Id;
  readonly idempotencyKey: string;
}

/** `listJobs` 的载荷。只返回当前 principal 授权的 Job。 */
export interface ListJobsRequest {
  readonly filter: JobFilter | null;
}

/**
 * 方法映射表。键是业务方法名，值是该方法的请求/响应对应。
 *
 * 无参方法的 `request` 是 `null`——契约里不为「没有参数」发明一个空对象类型，
 * 调用方写 `client.listConnections()`。
 */
export interface MethodMap {
  // ---- ConnectionClient（12）----
  listConnections: MethodEntry<null, ProfileView[]>;
  openSession: MethodEntry<OpenSessionRequest, OpenSessionReceipt>;
  getSession: MethodEntry<SessionRef, SessionView>;
  executeInSession: MethodEntry<ExecuteInSessionRequest, ExecutionReceipt>;
  executeAtTarget: MethodEntry<ExecuteAtTargetRequest, ExecutionReceipt>;
  setSessionContext: MethodEntry<SetSessionContextRequest, ContextChangeReceipt>;
  attachSession: MethodEntry<AttachmentRequest, SessionView>;
  detachSession: MethodEntry<AttachmentRequest, SessionView>;
  closeSession: MethodEntry<CloseSessionRequest, CloseReceipt>;
  getExecution: MethodEntry<ExecutionRef, ExecutionView>;
  cancelExecution: MethodEntry<ExecutionRef, CancelReceipt>;
  /**
   * 事件订阅。`response` 是单条事件——流由 `BackendTransport.subscribe` 返回，
   * 不是 `call` 的一次性结果。断开订阅**不直接取消 SQL**（概要 §6.2）。
   */
  subscribeEvents: MethodEntry<SubscribeEventsRequest, EventEnvelope<ConnectionEvent>>;

  // ---- ProfileClient（3）----
  createConnection: MethodEntry<CreateConnectionRequest, ProfileView>;
  updateConnection: MethodEntry<UpdateConnectionRequest, ProfileView>;
  disableConnection: MethodEntry<DisableConnectionRequest, ProfileView>;

  // ---- JobClient（4）----
  startJob: MethodEntry<StartJobRequest, JobView>;
  listJobs: MethodEntry<ListJobsRequest, JobView[]>;
  getJob: MethodEntry<JobRef, JobView>;
  cancelJob: MethodEntry<CancelJobRequest, CancelReceipt>;

  // ---- ArtifactClient（1）----
  readArtifact: MethodEntry<ReadArtifactRequest, ArtifactChunk>;

  // ---- SubmissionTokenClient（1）----
  issueSubmissionToken: MethodEntry<IssueSubmissionTokenRequest, SubmissionToken>;
}

/** 全部业务方法名的联合类型。`BackendClient` 的方法集必须与它逐字相等。 */
export type MethodName = keyof MethodMap;

/**
 * 传输契约。**任何**桌面 / Web 实现都满足这一个接口：桌面用 Tauri IPC，Web 用 HTTP。
 *
 * 契约层不规定 HTTP 状态码、不规定重试、不规定鉴权头——那是适配器的事。
 */
export interface BackendTransport {
  call<K extends keyof MethodMap>(
    method: K,
    payload: MethodMap[K]['request'],
  ): Promise<MethodMap[K]['response']>;
  /** 流传输适配为 AsyncIterable；结束迭代只取消订阅。 */
  subscribe?(
    method: 'subscribeEvents',
    payload: SubscribeEventsRequest,
  ): AsyncIterable<EventEnvelope<ConnectionEvent>>;
  cancel?(opaque: string, reason: string): Promise<void>;
}

// ---------------------------------------------------------------------------
// PlatformServices
// ---------------------------------------------------------------------------

/** 保存文本对话框的载荷。`extensions` 不含前导点，与驱动侧文件过滤口径一致。 */
export interface SaveTextInput {
  readonly contents: string;
  readonly defaultFileName: string;
  readonly filterName: string;
  readonly extensions: readonly string[];
}

/**
 * 保存二进制对话框的载荷。
 *
 * `bytes` 是**原始字节**，与 `ArtifactChunk.bytes` 同一形态。base64 等线缆编码是适配器的活
 * （桌面实现目前委托 `tauri-plugin-dialog`，内部走 base64 IPC）——契约层不做传输决策。
 */
export interface SaveBinaryInput {
  readonly bytes: Uint8Array;
  readonly defaultFileName: string;
  readonly filterName: string;
  readonly extensions: readonly string[];
}

/** 打开文本文件的载荷。 */
export interface OpenTextInput {
  readonly filterName: string;
  readonly extensions: readonly string[];
}

/** 打开目录的载荷。 */
export interface OpenDirectoryInput {
  readonly title: string;
}

/** 打开到的文本文件。**只含 basename，不含绝对路径**——路径不过 JS 边界。 */
export interface OpenedTextFile {
  readonly fileName: string;
  readonly contents: string;
}

/** 打开到的目录。 */
export interface OpenedDirectory {
  /** 目录标识。不保证是文件系统路径（Web 形态下可能是句柄）。 */
  readonly path: string;
  readonly name: string;
}

/**
 * 平台原生能力：文件对话框与剪贴板。
 *
 * 名称与 §2.1 一致，**不另立 `PlatformBridge` 之类的别名**（§7.1）。
 * 桌面实现委托 `tauri-plugin-dialog`；Web 实现可只实现剪贴板子集，其余必须显式抛错，
 * **不得**静默降级。
 */
export interface PlatformServices {
  saveTextWithDialog(input: SaveTextInput): Promise<boolean>;
  saveBinaryWithDialog(input: SaveBinaryInput): Promise<boolean>;
  openTextWithDialog(input: OpenTextInput): Promise<OpenedTextFile | null>;
  openDirectoryWithDialog(input: OpenDirectoryInput): Promise<OpenedDirectory | null>;
  writeClipboard(text: string): Promise<void>;
  readClipboard(): Promise<string>;
}

// ---------------------------------------------------------------------------
// 注入
// ---------------------------------------------------------------------------

/**
 * 已注入的 `PlatformServices`。模块级单值，与 `BackendClient` 的 `backendId` 选择
 * 无关——原生对话框在所有后端形态下都是同一套宿主能力。
 */
let injectedServices: PlatformServices | null = null;

/**
 * 注入 `PlatformServices`。必须在**首次绘制前**调用（§7.3）。
 * 重复注入是允许的：切换 backendId 只换门面，`PlatformServices` 不换。
 */
export function setPlatformServices(services: PlatformServices): void {
  injectedServices = services;
}

/** 读出已注入的 `PlatformServices`，未注入返回 `null`。 */
export function peekPlatformServices(): PlatformServices | null {
  return injectedServices;
}

/** 清空注入。测试用；生产路径不会调它。 */
export function clearPlatformServices(): void {
  injectedServices = null;
}
