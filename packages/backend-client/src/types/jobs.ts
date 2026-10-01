/**
 * Profile / job / artifact DTO mirrors.
 *
 * Mirrors of `docs/architecture/platform/system-overview.md` §6.2. These are
 * the write models and job-owned views that sit next to the connection
 * contracts in `./session`; they live in their own module because they are
 * owned by different backends (`ProfileRepository` and `JobRuntime` /
 * `ArtifactStore`) and carry no connection authorization of their own.
 */

import type { Counter, Id, Timestamp } from './identity';
import type { NamespaceTarget } from './session';

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
  offset: Counter;
  bytes: Uint8Array;
  resultCompleteness: ResultCompleteness;
}

/** Either an explicit chunk index or a byte range, never both. */
export type ArtifactReadRequest =
  | { artifactId: Id; chunkIndex: Counter }
  | { artifactId: Id; offset: Counter; limit: Counter };