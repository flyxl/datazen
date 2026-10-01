/**
 * `@datazen/backend-client` 公开入口。
 *
 * 分层：
 * 1. `types/` —— Rust 侧 DTO 的镜像（`platform-api` / `application` 是权威定义）。
 * 2. `transport.ts` —— `MethodMap` / `BackendTransport` / `PlatformServices` 契约。
 * 3. `errors.ts` —— `ApiError` 反序列化与重试分类。
 * 4. `streams.ts` —— 传输无关的推送事件队列。
 * 5. `client.ts` —— `BackendClient` 门面与 backendId 注入登记。
 *
 * 本包**零传输依赖**：不含 Tauri IPC 导入前缀、不含浏览器取数 API（`fetch` / XHR）
 * 字面量（门禁 F-07），也零 React 依赖（F-05）。桌面与 Web 各提供一个 `BackendTransport`
 * 实现。
 */

export type {
  BackendTransport,
  CancelJobRequest,
  CreateConnectionRequest,
  DisableConnectionRequest,
  ExecutionRef,
  JobRef,
  ListJobsRequest,
  MethodEntry,
  MethodMap,
  MethodName,
  OpenDirectoryInput,
  OpenedDirectory,
  OpenedTextFile,
  OpenTextInput,
  PlatformServices,
  SaveBinaryInput,
  SaveTextInput,
  SessionRef,
  UpdateConnectionRequest,
} from './transport';
export { clearPlatformServices, peekPlatformServices, setPlatformServices } from './transport';

export type {
  ApiError,
  ApiErrorCode,
  BackendError,
  BackendErrorKind,
  RetryDisposition,
} from './errors';
export {
  API_ERROR_CODES,
  apiErrorCode,
  apiErrorRequestId,
  backendError,
  isApiErrorCode,
  isRetryDisposition,
  mayRetryWithoutCheckingExecution,
  parseApiError,
  requiresOutcomeVerification,
  UNBOUND_MESSAGE,
} from './errors';

export type { PushEventStream, PushEventStreamOptions } from './streams';
export {
  collectLatest,
  createPushEventStream,
  drainStream,
  EventStreamOverflowError,
} from './streams';
export type { ConnectionEventStream } from './streams';

export type { BackendClient } from './client';
export {
  boundBackendIds,
  clearBackendClients,
  createBackendClient,
  currentBackendId,
  getBackendClient,
  normalizeBackendError,
  requireBackendClient,
  selectBackend,
  setBackendClient,
} from './client';

export * from './types';
