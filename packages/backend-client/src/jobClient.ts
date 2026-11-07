/**
 * The Job domain client (form-split F3).
 *
 * The one *stateful* domain: `submitJobIdempotent` keeps a per-client receipt
 * map (submission token idempotency key → job id) so an unknown-outcome
 * submission can be recovered instead of re-applied, and `watchJob` runs a
 * local polling loop with explicit re-subscribe semantics. The factory
 * therefore closes over that state; every other domain client is stateless.
 *
 * Also owns the JobView / CancelReceipt wire decoders — the schema alignment
 * point between a raw JSON payload and the documented DTO — which the
 * Execution domain reuses for cancel receipts.
 */

import { ApiError } from './errors';
import type { BackendGuard } from './guard';
import type { BackendTransport } from './transport';
import { emptyJobProgress, toCounter, toId } from './types';
import type {
  CancelReceipt,
  Counter,
  EffectOutcome,
  Id,
  JobProgress,
  JobState,
  JobView,
  SubmissionToken,
  Timestamp,
} from './types';

/** The Job slice of `MethodMap`. */
export interface JobMethodMap {
  startJob: {
    request: Record<string, unknown> & import('./types').SubmitRequest;
    response: JobView;
  };
  listJobs: { request: Record<string, unknown>; response: readonly JobView[] };
  getJob: { request: { jobId: string }; response: JobView };
  cancelJob: {
    request: import('./types').SubmitRequest & { jobId: string };
    response: CancelReceipt;
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

/** The Job domain client surface. */
export interface JobClient {
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
}

/**
 * Build the Job domain client over a transport.
 *
 * The receipt map lives inside this closure by design: two independently
 * created Job clients do not share idempotency receipts, because a receipt is
 * the property of the submitting client, not of the process.
 */
export function createJobMethods(transport: BackendTransport, guard: BackendGuard): JobClient {
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
    startJob,
    getJob,
    listJobs,
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
  };
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
    error: typeof raw['error'] === 'string' ? raw['error'] : null,
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

/** Idempotent submission behind `submitJobIdempotent`; see the interface docs. */
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

/** Compile-time: every Job map key is served by this domain client. */
type Assert<T extends true> = T;
export type JobClientCoversMap = Assert<keyof JobMethodMap extends keyof JobClient ? true : false>;
