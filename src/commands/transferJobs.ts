/**
 * Job command layer for Data Transfer — the caller for the
 * prepare/apply split introduced by the backend Job chain.
 *
 * This file is the **seam** the other two migration tools copy rather than
 * reinvent: `prepare` admits and freezes a plan, `apply` spends it exactly
 * once, and the reply is a Job verdict (`CommitBoundary` + derived
 * `EffectOutcome`) instead of the old flat success/failure boolean. Schema
 * Diff and Data Sync only need their own `*Jobs.ts` with the same four
 * members — `backendScope`, `prepare`, `apply`, `cancel` — to adopt the shared
 * presentation engine in `src/lib/migrationJobVerdict.ts`.
 *
 * Every request here goes through exactly one manager end to end. The
 * legacy `transferCommands.execute` is still exported for the rollback path
 * but is no longer called by the window.
 */

import { invoke } from '@tauri-apps/api/core';
import { parseJobView } from '@datazen/backend-client';
import type {
  CommitBoundary,
  EffectOutcome,
  JobDetails,
  JobProgress,
  JobRecoveryVerdict,
  JobView,
  JobState,
  Timestamp,
} from '@datazen/backend-client';
import type { TransferJob, TransferPreview } from './transfer';

/**
 * The scope string the local desktop backend declares. Both endpoints must
 * present it; there is no default, because "I forgot to declare a scope" is
 * precisely the input the rule exists to reject. Mirrors
 * `LOCAL_BACKEND_SCOPE` in `src-tauri/src/commands/data_transfer/job_api/scope.rs`.
 */
export const LOCAL_BACKEND_SCOPE = 'local-desktop-backend';

export interface TransferBackendScope {
  sourceBackendScope: string;
  targetBackendScope: string;
  /**
   * Profile scopes, if any. The backend rejects a foreign entry here, so the
   * local window sends an empty list rather than inventing one.
   */
  profileBackendScopes: string[];
}

/**
 * The scope every migration tool in this window declares.
 *
 * A SQL-file run carries no target connection, and the backend skips the
 * target check for it — but the field is mandatory on the wire, so the window
 * sends the same object on both branches instead of building a variant. One
 * code path means the SQL-file branch cannot drift into an under-specified
 * request.
 */
export function localBackendScope(): TransferBackendScope {
  return {
    sourceBackendScope: LOCAL_BACKEND_SCOPE,
    targetBackendScope: LOCAL_BACKEND_SCOPE,
    profileBackendScopes: [],
  };
}

export interface TransferPrepareJobRequest {
  job: TransferJob;
  backendScope: TransferBackendScope;
  /** Replaying the same key returns the recorded prepare Job. */
  idempotencyKey?: string;
}

export interface TransferApplyJobRequest {
  planId: string;
  planDigest: string;
  selectionRevision: number;
  selection: { sourceTables?: string[] | null };
  confirmedDestructive: boolean;
  backendScope: TransferBackendScope;
  idempotencyKey?: string;
}

export interface TransferPrepareJobView {
  jobId: string;
  kind: string;
  state: JobState;
  effectOutcome: EffectOutcome;
  planId: string;
  planDigest: string;
  planVersion: number;
  handlerVersion: number;
  checkpointVersion: number;
  selectionRevision: number;
  expiresAt: string;
  canExecute: boolean;
  blockReason: string | null;
  review: TransferPreview;
}

export interface TransferApplyJobView {
  jobId: string;
  kind: string;
  state: JobState;
  effectOutcome: EffectOutcome;
  progress: JobProgress;
  planId: string;
  planDigest: string;
  selectionRevision: number;
  /** Durable write markers; the only trustworthy record of what landed. */
  commitBoundaries: CommitBoundary[];
  /** SQL-file runs return `transfer-sql-<sha256hex>` here. */
  artifactIds: string[];
  /** Cancel intent. Never equivalent to "cancelled". */
  cancelled: boolean;
  partial: boolean;
  /** True when this reply came from the idempotent receipt, not a fresh write. */
  replayed: boolean;
  error: string | null;
  recoveryVerdict: JobRecoveryVerdict | null;
  recoveryResumeThrough: number | null;
  recoveryReason: string | null;
  createdAt?: Timestamp;
  updatedAt?: Timestamp;
}

type DataTransferJobE2eCall = {
  command:
    | 'prepare_data_transfer_job'
    | 'apply_data_transfer_job'
    | 'get_job'
    | 'get_transfer_job_details'
    | 'list_jobs'
    | 'cancel_data_transfer';
  args: unknown;
  response?: unknown;
  error?: string;
};

function e2eCaptures(): DataTransferJobE2eCall[] | undefined {
  return import.meta.env.VITE_E2E
    ? (globalThis as typeof globalThis & {
        __dataTransferJobCalls?: DataTransferJobE2eCall[];
      }).__dataTransferJobCalls
    : undefined;
}

/**
 * Capture one Job-path call for WDIO, mirroring the legacy `execute` seam so
 * specs can assert that the *old* command was never reached from the window.
 */
async function captureJobCall<T>(
  command: DataTransferJobE2eCall['command'],
  args: unknown,
  invokeCall: () => Promise<T>,
): Promise<T> {
  const captures = e2eCaptures();
  if (!captures) return invokeCall();
  const capture: DataTransferJobE2eCall = { command, args };
  captures.push(capture);
  try {
    const response = await invokeCall();
    capture.response = response;
    return response;
  } catch (error) {
    capture.error = String(error);
    throw error;
  }
}

export const transferJobCommands = {
  /** Prepare: admits the plan, freezes evidence, returns the plan digest to spend. */
  prepare: (request: TransferPrepareJobRequest) =>
    captureJobCall('prepare_data_transfer_job', request, () =>
      invoke<TransferPrepareJobView>('prepare_data_transfer_job', { request }),
    ),

  /** Apply: one planId, one Job, one idempotent write attempt. */
  apply: (request: TransferApplyJobRequest) =>
    captureJobCall('apply_data_transfer_job', request, () =>
      invoke<TransferApplyJobView>('apply_data_transfer_job', { request }),
    ),

  /** Read the live P5 Job projection after an accepted apply receipt. */
  getJob: (jobId: string) =>
    captureJobCall('get_job', { jobId }, () => invoke<JobView>('get_job', { jobId })),

  /** Read durable terminal evidence after a window reopens. */
  getDetails: (jobId: string) =>
    captureJobCall('get_transfer_job_details', { jobId }, async () => {
      const details = await invoke<JobDetails>('get_transfer_job_details', { jobId });
      return { ...details, job: parseJobView(details.job) };
    }),

  /** List Job projections for window hydration and lost-receipt recovery. */
  listJobs: (filter: Record<string, unknown> = {}) =>
    captureJobCall('list_jobs', filter, () => invoke<JobView[]>('list_jobs', filter)),

  /**
   * Cancel **request**. The same command the legacy path used — the Job
   * chain routes it to the Job repository first, so no second cancel command
   * is introduced here. `false` means "no such Job", which the caller must
   * surface rather than treat as a successful cancel.
   */
  cancel: (jobId: string) =>
    captureJobCall('cancel_data_transfer', { jobId }, () =>
      invoke<boolean>('cancel_data_transfer', { jobId }),
    ),
};
