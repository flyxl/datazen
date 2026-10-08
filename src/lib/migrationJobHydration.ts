/**
 * Migration job hydration — the client-side half of the client/interface
 * adaptation:
 *
 * 1. 窗口重新打开先查 Job，再读计划/结果。Window mount must consult the job
 *    center first; only then may it adopt a plan/result (artifact ids on the
 *    job view point at the private plan artifact).
 * 2. UI unmount 只退订（不释放资源）。Local subscriptions stop here; nothing
 *    in this module ever cancels, removes or releases a backend job.
 * 3. planId 过期/目标漂移/权限变化提示重新准备，不自动用旧 SQL 应用。
 *    A failed apply whose error code is in {@link STALE_PLAN_ERROR_CODES}
 *    must prompt the user to re-prepare; the stale plan is discarded, never
 *    silently re-applied.
 */

import {
  ApiError,
  getBackendClient,
  type BackendClient,
  type JobView,
} from '@datazen/backend-client';

/** Per-window job kind prefixes. Prepare and apply are separate Job kinds. */
export const MIGRATION_JOB_KINDS = {
  schemaDiff: ['schemaDiffPrepare', 'schemaDiffApply'],
  dataSync: ['dataSyncPrepare', 'dataSyncApply'],
  dataTransfer: ['dataTransferPrepare', 'dataTransferApply'],
} as const;

export type MigrationWindow = keyof typeof MIGRATION_JOB_KINDS;

export interface MigrationJobHydration {
  /** Jobs of this window's kinds that are still queued/running. */
  activeJobs: JobView[];
  /** Terminal or paused jobs whose effect outcome still needs a human decision. */
  verificationJobs: JobView[];
}

/**
 * Query the job center first. The callers read plans/results only after this
 * has run, so a window never hydrates from stale local state.
 */
export async function hydrateMigrationJobs(
  client: BackendClient,
  window: MigrationWindow,
): Promise<MigrationJobHydration> {
  const kinds = MIGRATION_JOB_KINDS[window];
  const isOurKind = (job: JobView): boolean =>
    (kinds as readonly string[]).includes(job.kind);
  const summaries = (await client.listJobs({ states: ['queued', 'running'] })).filter(isOurKind);
  const activeJobs: JobView[] = [];
  for (const summary of summaries) {
    try {
      activeJobs.push(await client.getJob(summary.jobId));
    } catch {
      // A job that vanished between list/get is simply not reported.
    }
  }
  const verificationJobs = (await client.listJobs({})).filter(
    (job) => isOurKind(job) && job.pendingVerificationReason !== null,
  );
  return { activeJobs, verificationJobs };
}

/** Classify a single job for a window banner. */
export function classifyJobView(job: JobView): 'active' | 'pendingVerification' | 'terminal' {
  if (job.pendingVerificationReason !== null) return 'pendingVerification';
  if (job.state === 'queued' || job.state === 'running') return 'active';
  return 'terminal';
}

/** Error codes that mean the plan must be discarded and re-prepared. */
export const STALE_PLAN_ERROR_CODES = ['PlanStale', 'SourceChanged', 'PermissionDenied'] as const;

/**
 * True when an apply failure was caused by plan expiry, target drift or a
 * permission change — the user must be told to re-prepare, and the old plan
 * (hence any old SQL) must not be reused.
 *
 * Accepts `ApiError`, legacy invoke errors carrying a `code` string field,
 * and errors whose message mentions one of the codes (the desktop invoke
 * path does not produce `ApiError` instances yet).
 */
export function isStalePlanError(error: unknown): boolean {
  if (error instanceof ApiError) {
    return (STALE_PLAN_ERROR_CODES as readonly string[]).includes(error.code);
  }
  if (error instanceof Error) {
    const code = (error as { code?: unknown }).code;
    if (typeof code === 'string' && (STALE_PLAN_ERROR_CODES as readonly string[]).includes(code)) {
      return true;
    }
    return (STALE_PLAN_ERROR_CODES as readonly string[]).some((c) =>
      error.message.includes(c),
    );
  }
  if (typeof error === 'object' && error !== null) {
    const code = (error as { code?: unknown }).code;
    return typeof code === 'string' && (STALE_PLAN_ERROR_CODES as readonly string[]).includes(code);
  }
  return false;
}

/** The job center view of this window's own most recent apply job, if any. */
export function latestApplyJob(jobs: readonly JobView[], window: MigrationWindow): JobView | null {
  const applyKinds = MIGRATION_JOB_KINDS[window].filter((k) => k.endsWith('Apply'));
  const candidates = jobs.filter((job) =>
    (applyKinds as readonly string[]).includes(job.kind),
  );
  if (candidates.length === 0) return null;
  return candidates.reduce((latest, job) =>
    job.createdAt > latest.createdAt ? job : latest,
  );
}

/** Resolve the bound client, or `null` when nothing is bound (e.g. tests). */
export function boundBackendClient(): BackendClient | null {
  return getBackendClient();
}
