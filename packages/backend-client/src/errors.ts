/**
 * 错误镜像：`ApiErrorCode` / `RetryDisposition` / `ApiError` / `BackendError`。
 *
 * 与 `packages/application/src/error.rs` 逐字对应（camelCase wire）。Rust 侧是权威定义，
 * 本文件只做**反序列化与分类**，不重新发明 code。
 *
 * 三条纪律：
 * 1. **code 是封闭枚举**：31 个取值逐字来自连接 §13 的错误表。新增取值等同协议版本升级，
 *    必须同时更新 driver、宿主与前端——前端**不得**把未知 code 折叠成 `InternalError`
 *    再丢掉原始值（`BackendError.code` 永远保留原字符串）。
 * 2. **执行终态失败不是 `ApiError`**：`ExecutionState='failed'` + `errorCode` 才是执行失败，
 *    两套枚举互不包含（CM §13.1）。
 * 3. **HTTP 映射是宿主的活**：`ApiError` 上没有 status 字段。Tauri 适配器映射成 IPC
 *    错误、HTTP 适配器映射成状态码，本包不参与。
 */

/**
 * 机器可读的错误 code。取值逐字来自连接 §13 的错误表。
 *
 * **封闭枚举**：这 31 个字面量与 Rust `ApiErrorCode` 一一对应，任何一侧新增都必须同时
 * 更新另一侧与宿主。
 */
export type ApiErrorCode =
  | 'invalidArgument'
  | 'targetRequired'
  | 'targetConflict'
  | 'targetUnsupported'
  | 'sessionNotFound'
  | 'sessionLost'
  | 'runtimeEpochMismatch'
  | 'contextConflict'
  | 'permissionDenied'
  | 'resourceBusy'
  | 'queueFull'
  | 'transactionResolutionRequired'
  | 'capabilityUnsupported'
  | 'unsupportedPlan'
  | 'endpointOverlap'
  | 'sessionQuotaExceeded'
  | 'idempotencyExpired'
  | 'rollbackFailed'
  | 'cleanupFailed'
  | 'outcomeUnknown'
  | 'planStale'
  | 'sourceChanged'
  | 'targetConflictRows'
  | 'idempotencyConflict'
  | 'unauthenticated'
  | 'notFound'
  | 'payloadTooLarge'
  | 'quotaExceeded'
  | 'rateLimited'
  | 'serviceUnavailable'
  | 'configRevisionMismatch';

/** 全部 code，按 Rust 侧 `ApiErrorCode::ALL` 的声明顺序。用于校验往返完整性。 */
export const API_ERROR_CODES: readonly ApiErrorCode[] = [
  'invalidArgument',
  'targetRequired',
  'targetConflict',
  'targetUnsupported',
  'sessionNotFound',
  'sessionLost',
  'runtimeEpochMismatch',
  'contextConflict',
  'permissionDenied',
  'resourceBusy',
  'queueFull',
  'transactionResolutionRequired',
  'capabilityUnsupported',
  'unsupportedPlan',
  'endpointOverlap',
  'sessionQuotaExceeded',
  'idempotencyExpired',
  'rollbackFailed',
  'cleanupFailed',
  'outcomeUnknown',
  'planStale',
  'sourceChanged',
  'targetConflictRows',
  'idempotencyConflict',
  'unauthenticated',
  'notFound',
  'payloadTooLarge',
  'quotaExceeded',
  'rateLimited',
  'serviceUnavailable',
  'configRevisionMismatch',
];

/**
 * 机器可读的重试政策，取值与 wire 字面值一致。
 *
 * **已知规格缺口**：连接 §13 说 `ResourceBusy` / `QueueFull` / `RateLimited` /
 * `ServiceUnavailable` / `QuotaExceeded` / `SessionQuotaExceeded` 可以「等待/退避后重发」，
 * 但概要 §6.3 的三个取值表达不了「退避重发」。Rust 侧因此保守地映射成 `never`，
 * 前端**不得**据此对这些 code 做自动重试——必须自己读 code 决定退避策略。
 */
export type RetryDisposition = 'never' | 'safeRead' | 'checkExecution';

/**
 * 用例层的失败载荷。跨边界传输时 `code` / `requestId` / `retryDisposition` 全部必填，
 * `message` 是**脱敏**后的给调用者文案。
 */
export interface ApiError {
  readonly code: ApiErrorCode;
  readonly message: string;
  readonly requestId: string | null;
  readonly retryDisposition: RetryDisposition;
}

/** 宿主未绑定 / 传输层自身故障。与业务 `ApiError` 是两件事，调用方必须能分开处理。 */
export type BackendErrorKind = 'unbound' | 'transport' | 'malformedResponse';

/** 传输层 / 绑定层故障。`apiError` 非空表示业务拒绝，优先读它。 */
export interface BackendError extends Error {
  readonly kind: BackendErrorKind;
  readonly apiError: ApiError | null;
}

/** 宿主未绑定时的固定文案。**不得**回退到任何默认传输（Tauri 除外，宿主自己选）。 */
export const UNBOUND_MESSAGE =
  'BackendClient has not been bound; check that the platform adapter ran.';

const CODE_SET: ReadonlySet<string> = new Set<string>(API_ERROR_CODES);
const DISPOSITIONS: readonly RetryDisposition[] = ['never', 'safeRead', 'checkExecution'];

/** 是否为已知的封闭 code。未知 code 返回 `false`（不抛异常）。 */
export function isApiErrorCode(value: unknown): value is ApiErrorCode {
  return typeof value === 'string' && CODE_SET.has(value);
}

/** 是否为合法的重试政策取值。 */
export function isRetryDisposition(value: unknown): value is RetryDisposition {
  return typeof value === 'string' && (DISPOSITIONS as readonly string[]).includes(value);
}

/**
 * 把传输层的失败载荷解析成 `ApiError`。
 *
 * 解析失败（缺 `code`、`code` 不在枚举内、字段类型不对）返回 `null`：
 * 调用方据此判定为 `BackendError`，而不是伪造一个看起来合法的 `ApiError`。
 * 这是「未知 code 不得被静默折叠」的落点。
 */
export function parseApiError(value: unknown): ApiError | null {
  if (typeof value !== 'object' || value === null) return null;
  const raw = value as Record<string, unknown>;
  if (!isApiErrorCode(raw.code)) return null;
  if (typeof raw.message !== 'string') return null;
  const requestId = raw.requestId;
  if (requestId !== null && requestId !== undefined && typeof requestId !== 'string') return null;
  const retryDisposition = raw.retryDisposition;
  if (!isRetryDisposition(retryDisposition)) return null;
  return {
    code: raw.code,
    message: raw.message,
    requestId: (requestId as string | null | undefined) ?? null,
    retryDisposition,
  };
}

/** 构造 `BackendError`。`cause` 只在浏览器支持时附带原始错误，便于定位。 */
export function backendError(
  kind: BackendErrorKind,
  message: string,
  apiError: ApiError | null = null,
  cause?: unknown,
): BackendError {
  const error = new Error(message) as Error & {
    cause?: unknown;
    kind: BackendErrorKind;
    apiError: ApiError | null;
  };
  error.name = 'BackendError';
  error.kind = kind;
  error.apiError = apiError;
  if (cause !== undefined) error.cause = cause;
  return error as BackendError;
}

/** 读取任意 catch 到的东西上的 `code`，没有业务错误时返回 `null`。 */
export function apiErrorCode(error: unknown): ApiErrorCode | null {
  if (typeof error !== 'object' || error === null) return null;
  const candidate = (error as Partial<BackendError>).apiError;
  return candidate !== undefined && candidate !== null ? candidate.code : null;
}

/** 读取 `requestId`，用于上报对账。没有则返回 `null`。 */
export function apiErrorRequestId(error: unknown): string | null {
  if (typeof error !== 'object' || error === null) return null;
  const candidate = (error as Partial<BackendError>).apiError;
  return candidate !== undefined && candidate !== null ? candidate.requestId : null;
}

/**
 * 是否可以**直接重发**同一个请求（同一幂等键）。
 *
 * 只对 `retryDisposition === 'safeRead'` 为真：`checkExecution` 必须先核验已受理的执行，
 * `never` 一律不重发。规格缺口里的退避重发 code 不在此列（见 `RetryDisposition` 注释）。
 */
export function mayRetryWithoutCheckingExecution(error: unknown): boolean {
  if (typeof error !== 'object' || error === null) return false;
  const candidate = (error as Partial<BackendError>).apiError;
  return candidate !== undefined && candidate !== null && candidate.retryDisposition === 'safeRead';
}

/** `OutcomeUnknown` 是**必须核验**的最高优先 code：执行是否发生不得猜测。 */
export function requiresOutcomeVerification(error: unknown): boolean {
  const code = apiErrorCode(error);
  return code === 'outcomeUnknown' || code === 'idempotencyConflict';
}
