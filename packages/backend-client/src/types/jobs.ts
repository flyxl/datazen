/**
 * Profile / job / artifact DTO mirrors.
 *
 * Mirrors of `docs/architecture/platform/system-overview.md` §6.2. These are
 * the write models and job-owned views that sit next to the connection
 * contracts in `./session`; they live in their own module because they are
 * owned by different backends (`ProfileRepository` and `JobRuntime` /
 * `ArtifactStore`) and carry no connection authorization of their own.
 */

import { toCounter } from './identity';
import type { Counter, Id, Timestamp } from './identity';
import type { EffectOutcome, NamespaceTarget } from './session';

/**
 * Credentials are write-only: they go to the secret provider and never come
 * back. The optional marker is "not supplied", which the backend treats as
 * "leave the stored secret alone", not as "clear it".
 */
export interface ProfileDraft {
  name: string;
  driverId: string;
  initialNamespace: NamespaceTarget;
  publicOptions: Readonly<Record<string, unknown>>;
  credentials?: Readonly<Record<string, string>>;
}

/**
 * A `ProfileDraft` patch may never change `driverId`.
 *
 * The driver is part of the target's identity; switching it means creating a
 * new profile, so `driverId` is excluded at the type level rather than by a
 * runtime check.
 */
export type ProfilePatch = Partial<Omit<ProfileDraft, 'driverId'>>;

/**
 * Read model of a connection profile.
 *
 * `credentialConfigured` is a boolean on purpose: the API reports whether a
 * secret exists and never echoes any part of it.
 */
export interface ProfileView {
  connectionId: Id;
  name: string;
  driverId: string;
  configRevision: Counter;
  credentialRevision: Counter;
  initialNamespace: NamespaceTarget;
  publicOptions: Readonly<Record<string, unknown>>;
  credentialConfigured: boolean;
  enabled: boolean;
}

export type JobState = 'queued' | 'running' | 'succeeded' | 'failed' | 'cancelled';

/**
 * P5 five-bucket progress counters (§2.3).
 *
 * The buckets are independent on purpose: a row whose commit has not been
 * confirmed must never be counted inside `committed`.
 */
export interface JobProgress {
  read: Counter;
  converted: Counter;
  attempted: Counter;
  committed: Counter;
  unknown: Counter;
}

const zeroCounter = (): Counter => {
  const value = toCounter(0);
  if (value === undefined) throw new Error('toCounter rejected 0');
  return value;
};

/** Zero-ed progress; the DTO decoder materializes the same shape for a missing field. */
export function emptyJobProgress(): JobProgress {
  return {
    read: zeroCounter(),
    converted: zeroCounter(),
    attempted: zeroCounter(),
    committed: zeroCounter(),
    unknown: zeroCounter(),
  };
}

/**
 * Job lifecycle is independent of any window: a job survives the window that
 * started it, which is why `JobView` carries ids rather than handles.
 */
export interface JobView {
  jobId: Id;
  kind: string;
  state: JobState;
  stage: string | null;
  executionIds: readonly Id[];
  artifactIds: readonly Id[];
  createdAt: Timestamp;
  updatedAt: Timestamp;
  /**
   * P5 derived effect outcome (§10.1.1). Independent of `state`: a failed or
   * cancelled job still reports its confirmed committed scope. `null` when the
   * backend has not derived one yet.
   */
  effectOutcome: EffectOutcome | null;
  /** Cancellation is a separate fact; it never rewrites `state` to cancelled. */
  cancelRequested: boolean;
  /** Why the job is pending verification (e.g. `outcomeUnknown`), else null. */
  pendingVerificationReason: string | null;
  /** Safe result detail written by the Job runtime, else null. */
  error: string | null;
  /** P5 five-bucket progress; zero when nothing has been reported yet. */
  progress: JobProgress;
}

/**
 * Committed boundary recorded in a checkpoint (§7).
 *
 * The only fact about "how far the write actually got"; a fingerprint, not a
 * live lease. Restart recovery is driven by this plus the payload digest, not
 * by re-running the old session.
 */
export interface CommitBoundary {
  stageId: Id;
  stableTargetFingerprint: string;
  committedAt: Timestamp;
  operationId: Id | null;
  batchId: Id | null;
  payloadDigest: string | null;
  evidence: readonly string[];
  verifiedAt: Timestamp | null;
}

export type JobRecoveryVerdict =
  | 'pendingVerification'
  | 'resumeAfterVerify'
  | 'reject'
  | 'requireManualReview';

/** Safe recovery receipt; reasonCode is a stable code, never handler text. */
export interface JobRecoveryResult {
  verdict: JobRecoveryVerdict;
  resumeThrough?: number;
  reasonCode?: string;
}

/** Durable detail query returned by `get_transfer_job_details`. */
export interface JobDetails {
  job: JobView;
  planId?: string;
  planDigest?: string;
  selectionRevision?: number;
  commitBoundaries: readonly CommitBoundary[];
  recovery?: JobRecoveryResult;
}

export type ResultCompleteness = 'pending' | 'complete' | 'truncated';

/**
 * One authorized slice of a result artifact.
 *
 * `totalChunks` is `null` while a streamed result is still arriving, and
 * `bytes` carries result bytes only — never a server path and never a
 * credential.
 */
export interface ArtifactChunk {
  artifactId: Id;
  chunkIndex: Counter;
  totalChunks: Counter | null;
  publishedChunkCount?: Counter;
  publishedByteLength?: Counter;
  offset: Counter;
  bytes: Uint8Array;
  resultCompleteness: ResultCompleteness;
}

/** Either an explicit chunk index or a byte range, never both. */
export type ArtifactReadRequest =
  | { artifactId: Id; chunkIndex: Counter }
  | { artifactId: Id; offset: Counter; limit: Counter };
