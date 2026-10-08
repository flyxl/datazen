import { describe, expect, it } from 'vitest';

import {
  classifyJobView,
  hydrateMigrationJobs,
  isStalePlanError,
  latestApplyJob,
  MIGRATION_JOB_KINDS,
} from '../migrationJobHydration';
import { ApiError, type BackendClient, type JobView } from '@datazen/backend-client';

function jobView(overrides: Partial<JobView> = {}): JobView {
  return {
    jobId: 'job-1' as JobView['jobId'],
    kind: 'dataSyncApply',
    state: 'running',
    stage: null,
    executionIds: [],
    artifactIds: [],
    createdAt: 1700000000000 as JobView['createdAt'],
    updatedAt: 1700000000000 as JobView['updatedAt'],
    effectOutcome: null,
    cancelRequested: false,
    pendingVerificationReason: null,
    error: null,
    progress: {
      read: 0 as JobView['progress']['read'],
      converted: 0 as JobView['progress']['converted'],
      attempted: 0 as JobView['progress']['attempted'],
      committed: 0 as JobView['progress']['committed'],
      unknown: 0 as JobView['progress']['unknown'],
    },
    ...overrides,
  };
}

describe('MIGRATION_JOB_KINDS', () => {
  it('covers all three windows with prepare/apply kinds', () => {
    expect(Object.keys(MIGRATION_JOB_KINDS)).toEqual(['schemaDiff', 'dataSync', 'dataTransfer']);
    for (const kinds of Object.values(MIGRATION_JOB_KINDS)) {
      expect(kinds.some((k) => k.endsWith('Prepare'))).toBe(true);
      expect(kinds.some((k) => k.endsWith('Apply'))).toBe(true);
    }
  });
});

describe('classifyJobView', () => {
  it('marks verification-pending jobs separately from active ones', () => {
    expect(classifyJobView(jobView({ pendingVerificationReason: 'outcomeUnknown' }))).toBe(
      'pendingVerification',
    );
    expect(classifyJobView(jobView({ state: 'queued' }))).toBe('active');
    expect(classifyJobView(jobView({ state: 'running' }))).toBe('active');
    expect(classifyJobView(jobView({ state: 'succeeded' }))).toBe('terminal');
  });
});

describe('isStalePlanError', () => {
  it('matches PlanStale / SourceChanged / PermissionDenied ApiErrors', () => {
    expect(isStalePlanError(new ApiError('PlanStale', 'x'))).toBe(true);
    expect(isStalePlanError(new ApiError('SourceChanged', 'x'))).toBe(true);
    expect(isStalePlanError(new ApiError('PermissionDenied', 'x'))).toBe(true);
    expect(isStalePlanError(new ApiError('OutcomeUnknown', 'x'))).toBe(false);
    // Legacy invoke errors do not carry ApiError codes, so the message is
    // consulted too: a thrown Error whose message names the code is stale.
    expect(isStalePlanError(new Error('PlanStale'))).toBe(true);
    expect(isStalePlanError(new Error('some other failure'))).toBe(false);
    expect(isStalePlanError('PlanStale')).toBe(false);
  });
});

describe('latestApplyJob', () => {
  it('picks the newest apply job for the window', () => {
    const older = jobView({
      jobId: 'j1' as JobView['jobId'],
      createdAt: 1000 as JobView['createdAt'],
      kind: 'schemaDiffApply',
    });
    const newer = jobView({
      jobId: 'j2' as JobView['jobId'],
      createdAt: 2000 as JobView['createdAt'],
      kind: 'schemaDiffApply',
    });
    const prepare = jobView({
      jobId: 'j3' as JobView['jobId'],
      createdAt: 3000 as JobView['createdAt'],
      kind: 'schemaDiffPrepare',
    });
    const other = jobView({
      jobId: 'j4' as JobView['jobId'],
      createdAt: 4000 as JobView['createdAt'],
      kind: 'dataSyncApply',
    });
    expect(latestApplyJob([older, newer, prepare, other], 'schemaDiff')?.jobId).toBe('j2');
    expect(latestApplyJob([older], 'dataSync')).toBeNull();
  });
});

describe('hydrateMigrationJobs', () => {
  it('queries the job center first (listJobs before getJob) and filters by window', async () => {
    const order: string[] = [];
    const client = {
      backendId: 'test',
      listJobs: async (filter: Record<string, unknown>) => {
        order.push('listJobs');
        if (filter['states']) {
          return [
            jobView({ jobId: 'j1' as JobView['jobId'], kind: 'dataSyncApply', state: 'running' }),
          ];
        }
        return [
          jobView({
            jobId: 'j1' as JobView['jobId'],
            kind: 'dataSyncApply',
            pendingVerificationReason: 'outcomeUnknown',
          }),
          jobView({
            jobId: 'j2' as JobView['jobId'],
            kind: 'workflow',
            pendingVerificationReason: 'outcomeUnknown',
          }),
        ];
      },
      getJob: async (jobId: JobView['jobId']) => {
        order.push(`getJob:${jobId}`);
        return jobView({ jobId });
      },
    } as unknown as BackendClient;

    const result = await hydrateMigrationJobs(client, 'dataSync');

    expect(order).toEqual(['listJobs', 'getJob:j1', 'listJobs']);
    expect(result.activeJobs).toHaveLength(1);
    expect(result.verificationJobs).toHaveLength(1);
    expect(result.verificationJobs[0]?.kind).toBe('dataSyncApply');
  });
});
