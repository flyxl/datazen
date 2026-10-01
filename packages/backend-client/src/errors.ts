/**
 * `ApiError` — the one error type the application layer rejects requests with.
 *
 * Mirrors `docs/architecture/platform/connection-management.md` §13 (the
 * authoritative code table) and `docs/architecture/platform/system-overview.md`
 * §6.3 (`code`, sanitized `message`, `requestId`, `retryDisposition`).
 *
 * Two facts this module exists to keep true:
 *
 * 1. **The code set is closed and does not grow here.** It is registered in
 *    the design documents, not invented per frontend feature. `ApiErrorCode` is
 *    the union of the values in §13; adding one requires a spec update, not an
 *    edit to this file's union.
 * 2. **Deserialization never trusts its input.** Adapters receive JSON across a
 *    process boundary, so `deserializeApiError` validates field by field and
 *    substitutes a known code instead of handing a half-typed object to
 *    business code.
 */

/**
 * Whether the caller may retry, and how.
 *
 * - `never` — permission, argument, capability or context conflict: fix or
 *   refresh first.
 * - `safeRead` — a read that is known to have had no side effects.
 * - `checkExecution` — an accepted submission whose outcome is unknown: query
 *   the execution, never blindly resend.
 */
export type RetryDisposition = 'never' | 'safeRead' | 'checkExecution';

const RETRY_DISPOSITIONS = ['never', 'safeRead', 'checkExecution'] as const;

export function isRetryDisposition(value: unknown): value is RetryDisposition {
  return typeof value === 'string' && (RETRY_DISPOSITIONS as readonly string[]).includes(value);
}

/**
 * The §13 code table, in the order the document lists it.
 *
 * Declared as a runtime array (not only a union) because the deserializer has
 * to validate untrusted input against it, and a union alone cannot be queried
 * at runtime.
 */
export const API_ERROR_CODES = [
  'InvalidArgument',
  'TargetRequired',
  'TargetConflict',
  'TargetUnsupported',
  'SessionNotFound',
  'SessionLost',
  'RuntimeEpochMismatch',
  'ContextConflict',
  'PermissionDenied',
  'ResourceBusy',
  'QueueFull',
  'TransactionResolutionRequired',
  'CapabilityUnsupported',
  'UnsupportedPlan',
  'EndpointOverlap',
  'SessionQuotaExceeded',
  'IdempotencyExpired',
  'RollbackFailed',
  'CleanupFailed',
  'OutcomeUnknown',
  'PlanStale',
  'SourceChanged',
  'TargetConflictRows',
  'IdempotencyConflict',
  'Unauthenticated',
  'NotFound',
  'PayloadTooLarge',
  'QuotaExceeded',
  'RateLimited',
  'ServiceUnavailable',
  'ConfigRevisionMismatch',
] as const;

/** Codes from the §13 table, in document order. */
export type ApiErrorCode = (typeof API_ERROR_CODES)[number];

function isApiErrorCode(value: string): value is ApiErrorCode {
  return (API_ERROR_CODES as readonly string[]).includes(value);
}

/** Wire shape of a rejected request, exactly as §6.3 defines it. */
export interface ApiErrorPayload {
  code: ApiErrorCode;
  message: string;
  requestId: string;
  retryDisposition: RetryDisposition;
}

export interface ApiErrorOptions {
  requestId?: string;
  retryDisposition?: RetryDisposition;
  cause?: unknown;
}

/**
 * A request was rejected.
 *
 * Only *rejected requests* become an `ApiError`. Once an execution has been
 * accepted, failures arrive as `ExecutionView` with an `ExecutionErrorCode`,
 * which is a separate namespace and must not be squeezed into `code` here.
 */
export class ApiError extends Error {
  readonly code: ApiErrorCode;
  readonly requestId: string;
  readonly retryDisposition: RetryDisposition;

  constructor(code: ApiErrorCode, message: string, options: ApiErrorOptions = {}) {
    super(message, options.cause === undefined ? undefined : { cause: options.cause });
    this.name = 'ApiError';
    this.code = code;
    this.requestId = options.requestId ?? '';
    this.retryDisposition = options.retryDisposition ?? defaultRetryDisposition(code);
  }

  /** Structured form, safe to log and to put in a report. */
  toPayload(): ApiErrorPayload {
    return {
      code: this.code,
      message: this.message,
      requestId: this.requestId,
      retryDisposition: this.retryDisposition,
    };
  }

  /**
   * True when plain retry of the *same* idempotency key is the right response.
   *
   * A `checkExecution` code is deliberately excluded: the request was
   * accepted, so resending it is exactly the thing §13 forbids.
   */
  isTransient(): boolean {
    return TRANSIENT_CODES.has(this.code);
  }
}

const TRANSIENT_CODES: ReadonlySet<ApiErrorCode> = new Set<ApiErrorCode>([
  'ResourceBusy',
  'QueueFull',
  'RateLimited',
  'ServiceUnavailable',
]);

/**
 * Default disposition per code, so a decoder that omits the field still
 * yields the policy §13 describes instead of a blanket "never retry".
 *
 * Only the cases where the default is not `never` need an entry; everything
 * else falls through to `never`.
 */
const RETRY_BY_CODE: Readonly<Partial<Record<ApiErrorCode, RetryDisposition>>> = {
  NotFound: 'safeRead',
  ResourceBusy: 'safeRead',
  QueueFull: 'safeRead',
  OutcomeUnknown: 'checkExecution',
  IdempotencyExpired: 'checkExecution',
  RollbackFailed: 'checkExecution',
  CleanupFailed: 'checkExecution',
  ServiceUnavailable: 'safeRead',
  RateLimited: 'safeRead',
};

/** Exported so an adapter can fill in a missing `retryDisposition` the same way. */
export function defaultRetryDisposition(code: ApiErrorCode): RetryDisposition {
  return RETRY_BY_CODE[code] ?? 'never';
}

/** The JSON type an adapter may hand us, before any validation. */
type RawErrorPayload = Record<string, unknown>;

function isRecord(value: unknown): value is RawErrorPayload {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

/**
 * Convert an arbitrary thrown value into an `ApiError`.
 *
 * Anything that is not a recognizable error payload becomes
 * `ServiceUnavailable` with the original kept as `cause`. That is the honest
 * reading: we know something went wrong at the boundary and nothing about
 * which code it was, and dropping the original would discard the only trace.
 */
export function deserializeApiError(raw: unknown): ApiError {
  if (raw instanceof ApiError) return raw;
  if (raw instanceof Error) {
    return new ApiError('ServiceUnavailable', raw.message, { cause: raw });
  }
  if (!isRecord(raw)) {
    return new ApiError('ServiceUnavailable', 'Backend rejected the request with an unreadable payload.');
  }

  const code = toApiErrorCode(raw['code']);
  const message = typeof raw['message'] === 'string' ? raw['message'] : 'Backend rejected the request.';
  const requestId = typeof raw['requestId'] === 'string' ? raw['requestId'] : '';
  const disposition = raw['retryDisposition'];
  const retryDisposition = isRetryDisposition(disposition) ? disposition : defaultRetryDisposition(code);

  return new ApiError(code, message, { requestId, retryDisposition, cause: raw });
}

/**
 * Closed-set code check.
 *
 * A code this build does not know about is reported as `ServiceUnavailable`
 * rather than passed through as a string: a frontend `switch` on an
 * unregistered code cannot handle it correctly, and "we cannot classify this"
 * has the same operational meaning as "the backend is unavailable".
 */
function toApiErrorCode(value: unknown): ApiErrorCode {
  if (typeof value === 'string' && isApiErrorCode(value)) return value;
  return 'ServiceUnavailable';
}