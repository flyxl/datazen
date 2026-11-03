/**
 * Profile / job / artifact DTO mirrors.
 *
 * Mirrors of `docs/architecture/platform/system-overview.md` §6.2. These are
 * the write models and job-owned views that sit next to the connection
 * contracts in `./session`; they live in their own module because they are
 * owned by different backends (`ProfileRepository` and `JobRuntime` /
 * `ArtifactRepository`) and carry no connection authorization of their own.
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
  | 'notExecuted'
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

/** A bounded, stable handler receipt; it has no free-form field for SQL or driver messages. */
export interface JobDomainResult {
  stageId: Id;
  resultCode: string;
  outcomeCode: string;
  counters: readonly { code: string; value: Counter }[];
  items: readonly {
    itemId: Id;
    outcomeCode: string;
    reasonCode?: string;
  }[];
  artifactIds: readonly Id[];
}

/** Stable target identity used by an explicit fresh-session recovery verifier. */
export interface JobRecoveryTarget {
  connectionId: Id;
  objectIds: readonly Id[];
}

/** Durable detail query returned by `get_transfer_job_details`. */
export interface JobDetails {
  job: JobView;
  stateVersion: Counter;
  planId?: string;
  planDigest?: string;
  selectionRevision?: number;
  commitBoundaries: readonly CommitBoundary[];
  recovery?: JobRecoveryResult;
  domainResults: readonly JobDomainResult[];
  recoveryTargets: readonly JobRecoveryTarget[];
  targetBeforeFingerprint?: string;
  recoveryPolicy?: string;
}

export type ResultCompleteness = 'pending' | 'complete' | 'truncated';

/**
 * Lifecycle of an artifact (mirror of the kernel `ArtifactLifecycle`,
 * camelCase on the wire). `revoked` / `expired` are unreadable: the download
 * path answers them with the deliberate §11 404, never with bytes.
 */
export type ArtifactLifecycle = 'writing' | 'complete' | 'truncated' | 'revoked' | 'expired';

/**
 * The metadata read (`GET /api/v1/artifacts/{id}?metadata=1`, §6.5 third read
 * mode) — mirror of the kernel `ArtifactMetadata`, camelCase on the wire.
 *
 * This is the planning half of a restore: `publishedChunkCount` bounds the
 * chunk-mode reads and `publishedByteSize` is the reassembled total a restore
 * reconciles against. Every read authorizes per request on the server side.
 */
export interface ArtifactMetadata {
  /** Immutable target binding supplied at creation; empty for legacy artifacts. */
  targetFingerprint: string;
  artifactId: Id;
  lifecycle: ArtifactLifecycle;
  publishedChunkCount: Counter;
  publishedByteSize: Counter;
  /** `null` only while the write has not been finalized. */
  totalChunks: Counter | null;
  resultCompleteness: ResultCompleteness;
  /** Present only when `resultCompleteness` is `truncated`. */
  truncationReason: string | null;
}

/**
 * The §6.5 uploads endpoint's v1 envelope — the single-request
 * begin+append+finalize the server defines (`server/src/http/router.rs`
 * `handler_artifact_upload`). One HTTP round trip; chunked writers aggregate
 * client-side and land here once (see {@link ArtifactUploadBeginRequest}).
 */
export interface ArtifactUploadEnvelope {
  executionId: Id;
  targetFingerprint: string;
  /** Server-side TTL; the handler defaults to 3600 when absent. */
  ttlSeconds?: number;
  /** The whole payload, standard base64. */
  payloadBase64: string;
  /** Lowercase hex SHA-256 of the decoded payload; mismatch aborts the artifact. */
  expectedSha256?: string;
}

/** 201 receipt of the uploads endpoint: the server-minted artifact id. */
export interface ArtifactUploadReceipt {
  artifactId: Id;
}

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

/**
 * One of the three mutually exclusive §6.5 read modes: the metadata envelope
 * (`?metadata=1`), an explicit chunk index, or a byte range — never more than
 * one per request (the server's `ArtifactReadMode::from_query` and the web
 * route table both reject a mixed or empty selection).
 */
export type ArtifactReadRequest =
  | { artifactId: Id; metadata: true }
  | { artifactId: Id; chunkIndex: Counter }
  | { artifactId: Id; offset: Counter; limit: Counter };

/** The metadata read mode's request, named for callers that plan a restore. */
export interface ArtifactMetadataReadRequest {
  artifactId: Id;
  metadata: true;
}
