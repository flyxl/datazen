/**
 * Data Transfer — Job state-machine tests for `useTransferJobRun`.
 *
 * These drive the hook directly rather than through the wizard, because three
 * of the four plan-lifecycle races are unreachable from the UI: the cancel
 * button is disabled while a plan is merely frozen, an apply cannot be clicked
 * twice, and the lost-receipt retry has no button. Testing them where they are
 * actually possible keeps the assertions about the state machine rather than
 * about button wiring.
 */

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { act, cleanup, renderHook } from '@testing-library/react';
import {
  toCounter,
  toId,
  toTimestamp,
  type CommitBoundary,
  type Counter,
  type Timestamp,
} from '@datazen/backend-client';
import { useTransferJobRun } from '../useTransferJobRun';
import type { TransferApplyJobView, TransferPrepareJobView } from '../../commands/transferJobs';
import type { TransferJob } from '../../commands/transfer';

const prepareMock = vi.hoisted(() => vi.fn());
const applyMock = vi.hoisted(() => vi.fn());
const cancelMock = vi.hoisted(() => vi.fn());

vi.mock('../../commands/transferJobs', () => ({
  localBackendScope: () => 'localBackendScope',
  transferJobCommands: {
    prepare: (request: unknown) => prepareMock(request),
    apply: (request: unknown) => applyMock(request),
    cancel: (jobId: unknown) => cancelMock(jobId),
  },
}));

const JOB: TransferJob = {
  source: { dbSessionId: 'session-source', database: 'app', schema: 'public' },
  target: { dbSessionId: 'session-target', database: 'app_copy', schema: 'public' },
  mode: 'data',
  writeMode: 'insert',
  tables: [
    { sourceTable: 'users', targetTable: 'users', enabled: true },
    { sourceTable: 'orders', targetTable: 'orders', enabled: true },
  ],
  options: { batchSize: 500 },
};

function prepareView(
  overrides: Partial<TransferPrepareJobView> = {},
): TransferPrepareJobView {
  return {
    jobId: 'transfer-data-prepare-1',
    kind: 'transfer-data',
    state: 'succeeded',
    effectOutcome: 'completed',
    planId: 'plan-abc',
    planDigest: 'digest-abc',
    planVersion: 1,
    handlerVersion: 1,
    checkpointVersion: 1,
    selectionRevision: 3,
    expiresAt: '2030-01-01T00:00:00Z',
    canExecute: true,
    blockReason: null,
    review: {
      planId: 'plan-abc',
      pairingPath: 'postgres -> postgres',
      mode: 'data',
      writeMode: 'insert',
      ddl: [],
      writePlans: [],
      warnings: [],
      canExecute: true,
      blockReason: null,
    },
    ...overrides,
  };
}

/**
 * Progress counters and commit-boundary ids are branded types on the wire. The
 * fixtures go through the package's own narrowing helpers rather than casting,
 * so a renamed field or a changed brand fails the build instead of quietly
 * becoming `any`. (The Rust `Counter` arrives as a decimal *string* at runtime;
 * that parsing is covered directly in `src/lib/__tests__/migrationJobVerdict.test.ts`.)
 */
function counter(value: number): Counter {
  const read = toCounter(value);
  if (read === undefined) throw new Error(`toCounter rejected ${value}`);
  return read;
}

function id(value: string): CommitBoundary['stageId'] {
  const read = toId(value);
  if (read === undefined) throw new Error(`toId rejected ${value}`);
  return read;
}

function timestamp(value: number): Timestamp {
  const read = toTimestamp(value);
  if (read === undefined) throw new Error(`toTimestamp rejected ${value}`);
  return read;
}

function verifiedBoundary(stage: string): CommitBoundary {
  return {
    stageId: id(stage),
    stableTargetFingerprint: `fingerprint-${stage}`,
    committedAt: timestamp(1_700_000_000_000),
    operationId: id(`op-${stage}`),
    batchId: id(`batch-${stage}`),
    payloadDigest: `digest-${stage}`,
    evidence: ['EVIDENCE_ROWS_COMMITTED'],
    verifiedAt: timestamp(1_700_000_001_000),
  };
}

function applyView(overrides: Partial<TransferApplyJobView> = {}): TransferApplyJobView {
  return {
    jobId: 'transfer-data-apply-1',
    kind: 'transfer-data',
    state: 'succeeded',
    effectOutcome: 'completed',
    progress: {
      read: counter(4),
      converted: counter(4),
      attempted: counter(4),
      committed: counter(4),
      unknown: counter(0),
    },
    planId: 'plan-abc',
    planDigest: 'digest-abc',
    selectionRevision: 3,
    commitBoundaries: [],
    artifactIds: [],
    cancelled: false,
    partial: false,
    replayed: false,
    error: null,
    recoveryVerdict: 'resumeAfterVerify',
    recoveryResumeThrough: null,
    recoveryReason: null,
    ...overrides,
  };
}

function renderRun() {
  return renderHook(() => useTransferJobRun());
}

async function prepareAndApply(run: ReturnType<typeof renderRun>) {
  await act(async () => {
    await run.result.current.prepare(JOB);
  });
  await act(async () => {
    await run.result.current.apply({ sourceTables: ['users'], confirmedDestructive: false });
  });
}

describe('useTransferJobRun', () => {
  beforeEach(() => {
    prepareMock.mockReset();
    prepareMock.mockResolvedValue(prepareView());
    applyMock.mockReset();
    applyMock.mockResolvedValue(applyView());
    cancelMock.mockReset();
    cancelMock.mockResolvedValue(true);
  });

  afterEach(() => {
    cleanup();
  });

  it('mints a plan on prepare and parks the run in `prepared`, with no verdict yet', async () => {
    const run = renderRun();
    await act(async () => {
      await run.result.current.prepare(JOB);
    });

    expect(run.result.current.phase).toBe('prepared');
    expect(run.result.current.prepareView?.planId).toBe('plan-abc');
    // No apply has happened, so there is nothing to certify yet.
    expect(run.result.current.verdict).toBeNull();
    expect(run.result.current.applyView).toBeNull();
    expect(run.result.current.cancelTargetJobId).toBe('transfer-data-prepare-1');
    expect(run.result.current.isInFlight).toBe(false);

    const request = prepareMock.mock.calls[0]?.[0] as {
      job: TransferJob;
      backendScope: string;
      idempotencyKey?: string;
    };
    expect(request.job).toBe(JOB);
    expect(request.backendScope).toBe('localBackendScope');
    // A prepare without an explicit key must not invent one.
    expect(request.idempotencyKey).toBeUndefined();
  });

  it('stops at `blocked` when the backend admits the plan but refuses to execute it', async () => {
    prepareMock.mockResolvedValue(prepareView({ canExecute: false, blockReason: 'target is read-only' }));

    const run = renderRun();
    await act(async () => {
      await run.result.current.prepare(JOB);
    });

    expect(run.result.current.phase).toBe('blocked');
    expect(run.result.current.prepareView?.blockReason).toBe('target is read-only');
  });

  it('reports a backend-scope refusal without leaving a plan behind', async () => {
    prepareMock.mockRejectedValueOnce(
      new Error('backend scope mismatch: expected local-desktop-backend'),
    );

    const run = renderRun();
    let returned: TransferPrepareJobView | null = null;
    await act(async () => {
      returned = await run.result.current.prepare(JOB);
    });

    expect(returned).toBeNull();
    expect(run.result.current.phase).toBe('blocked');
    expect(run.result.current.prepareView).toBeNull();
    expect(run.result.current.failure?.kind).toBe('backendScope');
    // Readable in the same tick the wizard's error panel renders.
    expect(run.result.current.lastFailure()?.kind).toBe('backendScope');
  });

  it('applies the frozen plan and settles on a verdict', async () => {
    applyMock.mockResolvedValue(
      applyView({ commitBoundaries: [verifiedBoundary('users')] }),
    );

    const run = renderRun();
    await prepareAndApply(run);

    expect(run.result.current.phase).toBe('settled');
    expect(run.result.current.applyView?.jobId).toBe('transfer-data-apply-1');
    expect(run.result.current.verdict?.severity).toBe('ok');
    expect(run.result.current.verdict?.completed).toBe(true);

    const request = applyMock.mock.calls[0]?.[0] as {
      planId: string;
      planDigest: string;
      selectionRevision: number;
      selection: { sourceTables: string[] | null };
      confirmedDestructive: boolean;
      backendScope: string;
      idempotencyKey?: string;
    };
    expect(request.planId).toBe('plan-abc');
    expect(request.planDigest).toBe('digest-abc');
    expect(request.selectionRevision).toBe(3);
    expect(request.selection.sourceTables).toEqual(['users']);
    expect(request.confirmedDestructive).toBe(false);
    expect(request.backendScope).toBe('localBackendScope');
  });

  it('sends an absent table list for a SQL-file run rather than an empty one', async () => {
    const run = renderRun();
    await act(async () => {
      await run.result.current.prepare(JOB);
    });
    await act(async () => {
      // A SQL-file run has no explicit selection: `null` means "the whole frozen
      // plan", whereas `[]` would apply nothing and look like a silent success.
      await run.result.current.apply({ confirmedDestructive: true });
    });

    const request = applyMock.mock.calls[0]?.[0] as { selection: { sourceTables: string[] | null } };
    expect(request.selection.sourceTables).toBeNull();
    expect(applyMock).toHaveBeenCalledTimes(1);
  });

  it('refuses a second apply on a spent planId without calling the backend', async () => {
    const run = renderRun();
    await prepareAndApply(run);

    let second: TransferApplyJobView | null = null;
    await act(async () => {
      second = await run.result.current.apply({ confirmedDestructive: false });
    });

    expect(second).toBeNull();
    // One write attempt only: the local guard is what keeps a double click from
    // becoming a second commit.
    expect(applyMock).toHaveBeenCalledTimes(1);
    expect(run.result.current.failure?.kind).toBe('planConsumed');
    expect(run.result.current.lastFailure()?.message).toMatch(/already applied/i);
  });

  it('lets a retry reach the backend with the same key when the receipt was lost', async () => {
    // Lost receipt: the write may well have happened, but the reply never came
    // back, so the hook holds no verdict and must not treat the plan as spent.
    applyMock.mockRejectedValueOnce(new Error('commit ack lost: transport closed'));
    const run = renderRun();
    await act(async () => {
      await run.result.current.prepare(JOB);
    });
    await act(async () => {
      await run.result.current.apply({ confirmedDestructive: false, idempotencyKey: 'apply-key-1' });
    });

    expect(run.result.current.phase).toBe('blocked');
    expect(run.result.current.applyView).toBeNull();
    expect(run.result.current.failure?.kind).toBe('other');

    // The retry repeats the attempt with the *same* key, so the backend's
    // receipt map can answer with the recorded receipt instead of committing
    // a second time.
    applyMock.mockResolvedValueOnce(applyView({ replayed: true }));
    await act(async () => {
      await run.result.current.apply({ confirmedDestructive: false, idempotencyKey: 'apply-key-1' });
    });

    expect(applyMock).toHaveBeenCalledTimes(2);
    const keys = applyMock.mock.calls.map((call) => (call[0] as { idempotencyKey?: string }).idempotencyKey);
    expect(keys).toEqual(['apply-key-1', 'apply-key-1']);
    expect(run.result.current.phase).toBe('settled');
    expect(run.result.current.applyView?.replayed).toBe(true);
  });

  it('never reuses the prepare idempotency key for the apply', async () => {
    const run = renderRun();
    await act(async () => {
      await run.result.current.prepare(JOB, { idempotencyKey: 'prepare-key-1' });
    });
    await act(async () => {
      await run.result.current.apply({ confirmedDestructive: false, idempotencyKey: 'apply-key-1' });
    });

    const prepareRequest = prepareMock.mock.calls[0]?.[0] as { idempotencyKey?: string };
    const applyRequest = applyMock.mock.calls[0]?.[0] as { idempotencyKey?: string };
    expect(prepareRequest.idempotencyKey).toBe('prepare-key-1');
    expect(applyRequest.idempotencyKey).toBe('apply-key-1');
    // One shared key would make the apply replay the *prepare* Job and write nothing.
    expect(applyRequest.idempotencyKey).not.toBe(prepareRequest.idempotencyKey);
  });

  it('treats a pipeline-budget refusal as a rejection, not a run failure', async () => {
    applyMock.mockRejectedValueOnce(
      new Error('pipeline budget exceeded for stage users: 10485760 of 8388608 bytes'),
    );

    const run = renderRun();
    await act(async () => {
      await run.result.current.prepare(JOB);
    });
    let applied: TransferApplyJobView | null = null;
    await act(async () => {
      applied = await run.result.current.apply({ confirmedDestructive: false });
    });

    expect(applied).toBeNull();
    expect(run.result.current.phase).toBe('blocked');
    expect(run.result.current.failure?.kind).toBe('pipelineBudget');
    // Nothing was written and nothing was verified, so no boundary list and no
    // verdict may be drawn.
    expect(run.result.current.applyView).toBeNull();
    expect(run.result.current.verdict).toBeNull();
  });

  it('classifies an apply-time backend-scope refusal', async () => {
    applyMock.mockRejectedValueOnce(new Error('backend scope rejected: target is not local'));

    const run = renderRun();
    await act(async () => {
      await run.result.current.prepare(JOB);
    });
    await act(async () => {
      await run.result.current.apply({ confirmedDestructive: false });
    });

    expect(run.result.current.failure?.kind).toBe('backendScope');
  });

  it('addresses a cancel to the apply Job id once one exists', async () => {
    const run = renderRun();
    await prepareAndApply(run);

    expect(run.result.current.cancelTargetJobId).toBe('transfer-data-apply-1');
    let acknowledged: boolean | null = null;
    await act(async () => {
      acknowledged = await run.result.current.requestCancel();
    });

    expect(acknowledged).toBe(true);
    expect(cancelMock).toHaveBeenCalledWith('transfer-data-apply-1');
    expect(run.result.current.cancelRequested).toBe(true);
    expect(run.result.current.cancelAcknowledged).toBe(true);
    expect(run.result.current.cancelUnknownJob).toBe(false);
  });

  it('reports a cancel the backend could not deliver instead of claiming it', async () => {
    cancelMock.mockResolvedValueOnce(false);

    const run = renderRun();
    await act(async () => {
      await run.result.current.prepare(JOB);
    });
    let acknowledged: boolean | null = null;
    await act(async () => {
      acknowledged = await run.result.current.requestCancel();
    });

    expect(acknowledged).toBe(false);
    expect(cancelMock).toHaveBeenCalledWith('transfer-data-prepare-1');
    expect(run.result.current.cancelRequested).toBe(true);
    expect(run.result.current.cancelAcknowledged).toBe(false);
    expect(run.result.current.cancelUnknownJob).toBe(true);
  });

  it('does not cancel the prepare Job while an apply is in flight', async () => {
    let releaseApply: (view: TransferApplyJobView) => void = () => {};
    applyMock.mockImplementationOnce(
      () =>
        new Promise<TransferApplyJobView>((resolve) => {
          releaseApply = resolve;
        }),
    );

    const run = renderRun();
    await act(async () => {
      await run.result.current.prepare(JOB);
    });

    let pending: Promise<TransferApplyJobView | null> = Promise.resolve(null);
    act(() => {
      pending = run.result.current.apply({ confirmedDestructive: false });
    });
    expect(run.result.current.phase).toBe('applying');
    expect(run.result.current.isInFlight).toBe(true);
    // The backend mints the apply Job id internally and only returns it once the
    // run is terminal, so there is no id to address a cancel to yet.
    expect(run.result.current.cancelTargetJobId).toBeNull();

    let acknowledged: boolean | null = null;
    await act(async () => {
      acknowledged = await run.result.current.requestCancel();
    });

    expect(acknowledged).toBe(false);
    expect(cancelMock).not.toHaveBeenCalled();
    // The *request* is latched; nothing claims a cancellation happened.
    expect(run.result.current.cancelRequested).toBe(true);
    expect(run.result.current.cancelUnknownJob).toBe(true);

    await act(async () => {
      releaseApply(applyView());
      await pending;
    });
    expect(run.result.current.phase).toBe('settled');
  });

  it('refuses to apply without an admitted plan', async () => {
    const run = renderRun();
    let applied: TransferApplyJobView | null = null;
    await act(async () => {
      applied = await run.result.current.apply({ confirmedDestructive: false });
    });

    expect(applied).toBeNull();
    expect(applyMock).not.toHaveBeenCalled();
    expect(run.result.current.failure?.message).toBe('apply requires an admitted plan');
  });

  it('clears every trace of a run on reset, including the cancel latches', async () => {
    const run = renderRun();
    await prepareAndApply(run);
    await act(async () => {
      await run.result.current.requestCancel();
    });

    act(() => {
      run.result.current.reset();
    });

    expect(run.result.current.phase).toBe('idle');
    expect(run.result.current.prepareView).toBeNull();
    expect(run.result.current.applyView).toBeNull();
    expect(run.result.current.verdict).toBeNull();
    expect(run.result.current.cancelRequested).toBe(false);
    expect(run.result.current.cancelAcknowledged).toBe(false);
    expect(run.result.current.cancelUnknownJob).toBe(false);
    expect(run.result.current.cancelTargetJobId).toBeNull();
  });
});