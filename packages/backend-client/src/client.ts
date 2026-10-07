/**
 * `BackendClient` — the typed facade every frontend consumer should hold.
 *
 * The method list is the authoritative one from
 * `docs/architecture/platform/system-overview.md` §6.2. Each method is a thin,
 * typed delegation to the bound `BackendTransport`, so this file contains
 * routing and no logic: no SQL, no database-type branching, no transport
 * mechanism. That is the point of the facade — a consumer that needs a new
 * capability asks the backend for it and never learns which backend it is
 * talking to.
 *
 * Backend selection happens *here*, by binding a different instance under a
 * different `backendId` (§7.3). It never happens by passing an id to a method.
 */

import { ApiError, deserializeApiError } from './errors';
import { createEventStream, toEventStream, type ConnectionEventStream } from './streams';
import type { BackendTransport } from './transport';
import { emptyJobProgress, toCounter, toId } from './types';
import type {
  ArtifactChunk,
  ArtifactReadRequest,
  AttachmentRequest,
  CancelReceipt,
  CloseReceipt,
  CloseSessionRequest,
  ContextChangeReceipt,
  Counter,
  EffectOutcome,
  ExecuteAtTargetRequest,
  ExecuteInSessionRequest,
  ExecutionReceipt,
  ExecutionView,
  Id,
  JobProgress,
  JobState,
  JobView,
  OpenSessionReceipt,
  OpenSessionRequest,
  ProfileDraft,
  ProfilePatch,
  ProfileView,
  SessionHandle,
  SessionView,
  SetSessionContextRequest,
  SubmissionToken,
  SubscribeEventsRequest,
  OwnerRef,
  Timestamp,
} from './types';

export interface BackendClient {
  /** Which backend this facade is bound to. Never a business-method argument. */
  readonly backendId: string;
  getPlatformIdentity(): Promise<{ clientInstanceId: string; organizationId: string; principalId: string }>;

  /**
   * The binding this facade delegates to.
   *
   * Exposed only for the transitional `packages/driver-sdk` re-exports, which
   * still speak the legacy driver-command gateway. Consumers call the methods
   * below; this goes away with that gateway.
   */
  readonly transport: BackendTransport;

  listConnections(): Promise<readonly ProfileView[]>;
  createConnection(draft: ProfileDraft, submissionToken: SubmissionToken): Promise<ProfileView>;
  updateConnection(
    connectionId: Id,
    expectedRevision: number,
    patch: ProfilePatch,
  ): Promise<ProfileView>;
  disableConnection(connectionId: Id, expectedRevision: number): Promise<ProfileView>;

  openSession(
    request: OpenSessionRequest,
    submissionToken: SubmissionToken,
  ): Promise<OpenSessionReceipt>;
  getSession(handle: SessionHandle): Promise<SessionView>;
  executeInSession(
    request: ExecuteInSessionRequest,
    token: SubmissionToken,
  ): Promise<ExecutionReceipt>;
  executeAtTarget(
    request: ExecuteAtTargetRequest,
    token: SubmissionToken,
  ): Promise<ExecutionReceipt>;
  setSessionContext(
    request: SetSessionContextRequest,
    token: SubmissionToken,
  ): Promise<ContextChangeReceipt>;
  attachSession(request: AttachmentRequest): Promise<SessionView>;
  detachSession(request: AttachmentRequest): Promise<SessionView>;
  closeSession(request: CloseSessionRequest): Promise<CloseReceipt>;

  getExecution(executionId: Id): Promise<ExecutionView>;
  /**
   * Ask the backend to cancel an execution. This is the *only* path that may
   * cancel SQL: ending the event subscription is not cancelling, and keeping
   * them in separate types is what keeps that true.
   */
  cancelExecution(executionId: Id, token: SubmissionToken): Promise<CancelReceipt>;

  /**
   * Subscribe to connection events.
   *
   * The handle is owned by this facade. Breaking out of the iteration ends the
   * subscription locally (§7.3) and leaves any running execution alone.
   */
  subscribeEvents(request: SubscribeEventsRequest): ConnectionEventStream;

  startJob(definition: Record<string, unknown>, token: SubmissionToken): Promise<JobView>;
  listJobs(filter: Record<string, unknown>): Promise<readonly JobView[]>;
  getJob(jobId: Id): Promise<JobView>;
  cancelJob(jobId: Id, token: SubmissionToken): Promise<CancelReceipt>;

  /**
   * Poll `getJob` and deliver each fresh view, with explicit re-subscribe
   * semantics: after any delivery error the next successful fetch is reported
   * as `resubscribed`, so a consumer may treat it as a fresh attachment to the
   * job rather than a continuation of the old stream.
   *
   * The returned handle's `stop()` only ends the local polling: it never
   * cancels or releases the backend job. A window unmount is an unsubscribe,
   * not a resource release.
   */
  watchJob(
    jobId: Id,
    onUpdate: (view: JobView, meta: { resubscribed: boolean }) => void,
    options?: JobWatchOptions,
  ): JobWatchHandle;

  /**
   * Submit an apply job with idempotent-timeout recovery.
   *
   * If the submission does not settle within `timeoutMs`, or fails with a
   * transient availability/outcome-unknown error, the client first consults
   * the original idempotent receipt (the token→jobId map populated by
   * `startJob`, then a `listJobs` scan by kind + creation time) instead of
   * immediately creating a new job. An unknown commit therefore never spawns
   * a second apply Job; when no receipt can be found the error is
   * `OutcomeUnknown`, which is check-then-retry, not blind retry.
   */
  submitJobIdempotent(
    definition: Record<string, unknown>,
    token: SubmissionToken,
    options?: SubmitJobOptions,
  ): Promise<JobView>;

  readArtifact(request: ArtifactReadRequest): Promise<ArtifactChunk>;
  issueSubmissionToken(operation: string, handle: SessionHandle | null, scope?: { connectionId: string; owner: OwnerRef }): Promise<SubmissionToken>;
}

/**
 * Build a facade over a transport.
 *
 * Every method delegates to `transport.call` with its `MethodMap` key, and
 * every rejection passes through `asApiError` so an adapter that throws a raw
 * object cannot leak an unrecognized error type to consumers.
 */
export function createBackendClient(backendId: string, transport: BackendTransport): BackendClient {
  /** Delegation plus error normalization, applied to every kernel method. */
  const guard = async <T>(method: string, run: () => Promise<T>): Promise<T> => {
    try {
      return await run();
    } catch (error) {
      throw asApiError(error, method);
    }
  };

  /** Per-client receipt map: submission token idempotency key → job id. */
  const jobReceiptByKey = new Map<string, Id>();

  const startJob = async (
    definition: Record<string, unknown>,
    submissionToken: SubmissionToken,
  ): Promise<JobView> => {
    const raw = await guard('startJob', () =>
      transport.call('startJob', { ...definition, submissionToken }),
    );
    const job = parseJobView(raw);
    jobReceiptByKey.set(submissionToken.idempotencyKey, job.jobId);
    return job;
  };

  const getJob = async (jobId: Id): Promise<JobView> => {
    const raw = await guard('getJob', () => transport.call('getJob', { jobId }));
    return parseJobView(raw);
  };

  const listJobs = async (filter: Record<string, unknown>): Promise<readonly JobView[]> => {
    const raw = await guard('listJobs', () => transport.call('listJobs', filter));
    if (!Array.isArray(raw)) {
      throw new ApiError('ServiceUnavailable', 'listJobs returned a malformed payload.');
    }
    return raw.map((entry) => parseJobView(entry));
  };

  const cancelJob = async (jobId: Id, submissionToken: SubmissionToken) => {
    const raw = await guard('cancelJob', () =>
      transport.call('cancelJob', { jobId, submissionToken }),
    );
    return parseCancelReceipt(raw);
  };

  return {
    backendId,
    getPlatformIdentity: () => guard('getPlatformIdentity', () => transport.call('getPlatformIdentity', undefined)),
    transport,

    listConnections: () =>
      guard('listConnections', () => transport.call('listConnections', undefined)),

    createConnection: (draft, submissionToken) =>
      guard('createConnection', () =>
        transport.call('createConnection', { ...draft, submissionToken }),
      ),

    updateConnection: (connectionId, expectedRevision, patch) =>
      guard('updateConnection', () =>
        transport.call('updateConnection', { connectionId, expectedRevision, patch }),
      ),

    disableConnection: (connectionId, expectedRevision) =>
      guard('disableConnection', () =>
        transport.call('disableConnection', { connectionId, expectedRevision }),
      ),

    openSession: (request, submissionToken) =>
      guard('openSession', () => transport.call('openSession', { ...request, submissionToken })),

    getSession: (handle) => guard('getSession', () => transport.call('getSession', { handle })),

    executeInSession: (request, submissionToken) =>
      guard('executeInSession', () =>
        transport.call('executeInSession', { ...request, submissionToken }),
      ),

    executeAtTarget: (request, submissionToken) =>
      guard('executeAtTarget', () =>
        transport.call('executeAtTarget', { ...request, submissionToken }),
      ),

    setSessionContext: (request, submissionToken) =>
      guard('setSessionContext', () =>
        transport.call('setSessionContext', { ...request, submissionToken }),
      ),

    attachSession: (request) =>
      guard('attachSession', () => transport.call('attachSession', request)),
    detachSession: (request) =>
      guard('detachSession', () => transport.call('detachSession', request)),

    closeSession: (request) => guard('closeSession', () => transport.call('closeSession', request)),

    getExecution: (executionId) =>
      guard('getExecution', () => transport.call('getExecution', { executionId })),

    cancelExecution: (executionId, submissionToken) =>
      guard('cancelExecution', () =>
        transport.call('cancelExecution', { executionId, submissionToken }),
      ),

    subscribeEvents: (request) => {
      const { subscribe } = transport;
      if (!subscribe) {
        // An adapter without stream support fails on *use*, not on bind, and
        // with a code from the §13 table rather than a bare TypeError.
        return createEventStream<never>(
          {
            next: () =>
              Promise.reject(
                new ApiError(
                  'CapabilityUnsupported',
                  'Event subscription is not supported by this backend.',
                ),
              ),
            release: () => undefined,
          },
          request.streamId,
        );
      }
      return toEventStream(subscribe.call(transport, 'subscribeEvents', request), request.streamId);
    },

    startJob,

    listJobs,

    getJob,

    cancelJob,

    watchJob: (jobId, onUpdate, options) => watchJobLoop(getJob, jobId, onUpdate, options),

    submitJobIdempotent: (definition, submissionToken, options) =>
      submitJobIdempotentImpl(
        startJob,
        getJob,
        listJobs,
        jobReceiptByKey,
        definition,
        submissionToken,
        options,
      ),

    readArtifact: (request) => guard('readArtifact', () => transport.call('readArtifact', request)),

    issueSubmissionToken: (operation, handle, scope) =>
      guard('issueSubmissionToken', () =>
        transport.call('issueSubmissionToken', { operation, handle, ...scope }),
      ),
  };
}

/** How a `watchJob` loop tunes its polling. */
export interface JobWatchOptions {
  /** Poll interval; defaults to 1500ms. */
  intervalMs?: number;
  /** Surface delivery errors; the loop then waits and re-subscribes. */
  onError?: (error: unknown) => void;
}

export interface JobWatchHandle {
  /** End the local polling loop. Never cancels or releases the job. */
  stop(): void;
}

export interface SubmitJobOptions {
  /** Milliseconds before the submission is considered timed out. */
  timeoutMs?: number;
}

/**
 * Normalize anything thrown inside a facade method into an `ApiError`.
 *
 * A well-formed `ApiError` passes through untouched — re-wrapping it would
 * discard its `requestId`, which is the one field that makes a user-visible
 * failure traceable to a backend log line.
 */
function asApiError(error: unknown, method: string): ApiError {
  if (error instanceof ApiError) return error;
  const apiError = deserializeApiError(error);
  if (apiError.code === 'ServiceUnavailable' && !apiError.requestId) {
    return new ApiError(apiError.code, `${method} failed: ${apiError.message}`, { cause: error });
  }
  return apiError;
}

/* ------------------------------------------------------------------ */
/* Job DTO decoding and the two submission/watch semantics helpers.    */
/*                                                                     */
/* The desktop adapter currently forwards raw JSON, so these decoders    */
/* are the schema alignment point: a payload that drops the P5 fields  */
/* is normalized to documented defaults instead of leaking `undefined`, */
/* and one that violates the contract fails loudly.                     */
/* ------------------------------------------------------------------ */

const JOB_STATES: readonly string[] = ['queued', 'running', 'succeeded', 'failed', 'cancelled'];
const EFFECT_OUTCOMES: readonly string[] = [
  'notStarted',
  'completed',
  'rolledBack',
  'partiallyApplied',
  'unknown',
];
const EXECUTION_STATES: readonly string[] = [
  'queued',
  'running',
  'cancelRequested',
  'succeeded',
  'failed',
  'cancelled',
];
const CANCEL_DISPOSITIONS: readonly string[] = ['requested', 'unsupported', 'alreadyFinished'];

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function parseEnum<T extends string>(value: unknown, allowed: readonly string[]): T | null {
  return typeof value === 'string' && allowed.includes(value) ? (value as T) : null;
}

/** A wire Timestamp may be a number or an ISO-8601 string; normalize to number. */
function parseTimestamp(value: unknown): Timestamp | undefined {
  if (typeof value === 'number' && Number.isFinite(value)) return value as Timestamp;
  if (typeof value === 'string' && value.length > 0) {
    const ms = Date.parse(value);
    if (!Number.isNaN(ms)) return ms as Timestamp;
  }
  return undefined;
}

function parseIdArray(value: unknown): readonly Id[] {
  if (!Array.isArray(value)) return [];
  const out: Id[] = [];
  for (const entry of value) {
    const id = toId(entry);
    if (id !== undefined) out.push(id);
  }
  return out;
}

function parseEffectOutcome(value: unknown): EffectOutcome | null {
  return parseEnum<EffectOutcome>(value, EFFECT_OUTCOMES);
}

function parseJobProgress(value: unknown): JobProgress {
  if (!isRecord(value)) return emptyJobProgress();
  return {
    read: counterOrAbsent(value['read'], 'read'),
    converted: counterOrAbsent(value['converted'], 'converted'),
    attempted: counterOrAbsent(value['attempted'], 'attempted'),
    committed: counterOrAbsent(value['committed'], 'committed'),
    unknown: counterOrAbsent(value['unknown'], 'unknown'),
  };
}

/**
 * A counter that is **absent** is zero — an older payload simply has no such
 * field yet. A counter that is **present and unreadable** is a different thing:
 * it is a count the client cannot hold faithfully, because it exceeds
 * `Number.MAX_SAFE_INTEGER`, and reporting it as 0 is precisely the silent lie
 * this narrowing is meant to prevent. A migration of 2^53 + 1 rows and a
 * migration of no rows must never look alike, so the second case throws instead
 * of collapsing into the first.
 */
function counterOrAbsent(raw: unknown, field: string): Counter {
  if (raw === undefined || raw === null) return zeroCounter();
  const narrowed = toCounter(raw);
  if (narrowed === undefined) {
    throw new ApiError(
      'ServiceUnavailable',
      `Malformed job progress: progress.${field} is ${JSON.stringify(raw)}, which is not a count this client can read without losing it.`,
    );
  }
  return narrowed;
}

function zeroCounter(): Counter {
  const value = toCounter(0);
  if (value === undefined) throw new Error('toCounter rejected 0');
  return value;
}

/** Schema-align a raw payload with the backend JobView DTO (§8). */
export function parseJobView(raw: unknown): JobView {
  if (!isRecord(raw)) {
    throw new ApiError('ServiceUnavailable', 'Malformed job view: not an object.');
  }
  const jobId = toId(raw['jobId']);
  const kind = typeof raw['kind'] === 'string' ? raw['kind'] : undefined;
  const state = parseEnum<JobState>(raw['state'], JOB_STATES);
  const createdAt = parseTimestamp(raw['createdAt']);
  const updatedAt = parseTimestamp(raw['updatedAt']);
  if (
    jobId === undefined ||
    kind === undefined ||
    state === null ||
    createdAt === undefined ||
    updatedAt === undefined
  ) {
    throw new ApiError(
      'ServiceUnavailable',
      'Malformed job view: required field missing or mistyped.',
    );
  }
  const stage = typeof raw['stage'] === 'string' ? raw['stage'] : null;
  return {
    jobId,
    kind,
    state,
    stage,
    executionIds: parseIdArray(raw['executionIds']),
    artifactIds: parseIdArray(raw['artifactIds']),
    createdAt,
    updatedAt,
    effectOutcome: parseEffectOutcome(raw['effectOutcome']),
    cancelRequested: raw['cancelRequested'] === true,
    pendingVerificationReason:
      typeof raw['pendingVerificationReason'] === 'string'
        ? raw['pendingVerificationReason']
        : null,
    progress: parseJobProgress(raw['progress']),
  };
}

/** Schema-align a raw payload with the backend CancelReceipt DTO. */
export function parseCancelReceipt(raw: unknown): CancelReceipt {
  if (!isRecord(raw)) {
    throw new ApiError('ServiceUnavailable', 'Malformed cancel receipt: not an object.');
  }
  const executionId = toId(raw['executionId']);
  const disposition = parseEnum<'requested' | 'unsupported' | 'alreadyFinished'>(
    raw['disposition'],
    CANCEL_DISPOSITIONS,
  );
  const state = parseEnum<CancelReceipt['state']>(raw['state'], EXECUTION_STATES);
  if (executionId === undefined || disposition === null || state === null) {
    throw new ApiError(
      'ServiceUnavailable',
      'Malformed cancel receipt: required field missing or mistyped.',
    );
  }
  return { executionId, disposition, state };
}

/** Poll loop behind `watchJob`; see the interface docs for the semantics. */
function watchJobLoop(
  getJob: (jobId: Id) => Promise<JobView>,
  jobId: Id,
  onUpdate: (view: JobView, meta: { resubscribed: boolean }) => void,
  options?: JobWatchOptions,
): JobWatchHandle {
  let stopped = false;
  let disconnected = false;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const intervalMs = options?.intervalMs ?? 1500;

  const tick = async (): Promise<void> => {
    if (stopped) return;
    try {
      const view = await getJob(jobId);
      const resubscribed = disconnected;
      disconnected = false;
      if (!stopped) onUpdate(view, { resubscribed });
    } catch (error) {
      disconnected = true;
      options?.onError?.(error);
    } finally {
      if (!stopped) {
        timer = setTimeout(() => {
          void tick();
        }, intervalMs);
      }
    }
  };

  void tick();
  return {
    stop: () => {
      stopped = true;
      if (timer !== undefined) clearTimeout(timer);
    },
  };
}

/** Idempotent submission behind `submitJobIdempotent`; see interface docs. */
async function submitJobIdempotentImpl(
  startJob: (definition: Record<string, unknown>, token: SubmissionToken) => Promise<JobView>,
  getJob: (jobId: Id) => Promise<JobView>,
  listJobs: (filter: Record<string, unknown>) => Promise<readonly JobView[]>,
  jobReceiptByKey: Map<string, Id>,
  definition: Record<string, unknown>,
  token: SubmissionToken,
  options?: SubmitJobOptions,
): Promise<JobView> {
  const startedAt = Date.now();
  let applied: JobView | undefined;
  let submissionError: unknown;
  try {
    if (options?.timeoutMs === undefined) {
      applied = await startJob(definition, token);
    } else {
      applied = await Promise.race([
        startJob(definition, token),
        new Promise<never>((_, reject) =>
          setTimeout(
            () => reject(new ApiError('OutcomeUnknown', 'Job submission timed out.')),
            options.timeoutMs,
          ),
        ),
      ]);
    }
  } catch (error) {
    submissionError = error;
  }

  if (applied !== undefined) return applied;

  // Timeout or transient failure: consult the original idempotent receipt
  // before considering any new submission.
  const receiptId = jobReceiptByKey.get(token.idempotencyKey);
  if (receiptId !== undefined) {
    try {
      return await getJob(receiptId);
    } catch {
      // Fall through to the listJobs scan.
    }
  }

  try {
    const kind = typeof definition['kind'] === 'string' ? definition['kind'] : undefined;
    const recent = (await listJobs({})).filter(
      (job) =>
        (kind === undefined || job.kind === kind) &&
        job.createdAt >= startedAt - 5000 &&
        job.createdAt <= startedAt + 60_000,
    );
    if (recent.length === 1) return recent[0];
  } catch {
    // Consulting the receipt index failed; report the original outcome.
  }

  if (submissionError instanceof ApiError) throw submissionError;
  throw new ApiError(
    'OutcomeUnknown',
    'Job submission outcome is unknown; check the job center rather than re-applying.',
  );
}

/**
 * The binding registry (§7.3, §7.4).
 *
 * `backendId` selects a facade; it is never a business-method argument. The app
 * entry binds before first paint and afterwards only swaps the selection.
 */
const clientsByBackend = new Map<string, BackendClient>();
let selectedBackendId: string | null = null;

/**
 * Bind (or rebind) one backend's facade.
 *
 * The *first* binding also selects it, because the single-backend app entry
 * calls this once and then expects lookups to resolve. Later bindings only
 * register: switching between already-bound backends is `selectBackend`'s job,
 * so binding a second facade must not silently steal the current selection.
 */
export function setBackendClient(backendId: string, client: BackendClient): void {
  clientsByBackend.set(backendId, client);
  if (selectedBackendId === null) selectedBackendId = backendId;
}

/** Choose which bound facade subsequent lookups resolve to. */
export function selectBackend(backendId: string): void {
  selectedBackendId = backendId;
}

/** The selected facade, or `null` when nothing resolves. */
export function getBackendClient(): BackendClient | null {
  if (selectedBackendId === null) return null;
  return clientsByBackend.get(selectedBackendId) ?? null;
}

/** Which backend is selected, or `null`. */
export function getSelectedBackendId(): string | null {
  return selectedBackendId;
}

/** Whether a facade is currently resolvable. */
export function isBackendClientBound(): boolean {
  return getBackendClient() !== null;
}

/** Drop every binding. Exists so tests can exercise the unbound path. */
export function clearBackendClients(): void {
  clientsByBackend.clear();
  selectedBackendId = null;
}

/**
 * Resolve the selected facade, or throw a locatable error.
 *
 * §7.4's rule: never fall back to a direct connection when nothing is bound. A
 * silent fallback is exactly how a desktop-only path survives a failed
 * injection and fails later, somewhere unrelated. Throwing names the missing
 * binding instead.
 */
export function useBackendClient(): BackendClient {
  const client = getBackendClient();
  if (!client) {
    throw new Error('BackendClient has not been bound; check that the platform adapter ran.');
  }
  return client;
}
