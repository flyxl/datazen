import { cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SchemaDiffJobDetails } from '../../../commands/schemaDiff';
import { SchemaDiffJobStatusPanel } from '../SchemaDiffJobStatusPanel';
import type { SchemaDiffTrackedJob } from '../useSchemaDiffJobLifecycle';

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

afterEach(cleanup);

function trackedJob(
  recovery: { verdict: 'notExecuted' | 'pendingVerification'; reasonCode: string },
  pendingVerificationReason: string | null,
): SchemaDiffTrackedJob {
  const jobId = 'schemaDiffApply-test';
  const details = {
    details: {
      job: {
        jobId,
        kind: 'schemaDiffApply',
        state: 'failed',
        stage: null,
        executionIds: [],
        artifactIds: [],
        createdAt: 0,
        updatedAt: 0,
        effectOutcome: 'notStarted',
        cancelRequested: false,
        pendingVerificationReason,
        error: recovery.reasonCode,
        progress: { read: 0, converted: 0, attempted: 0, committed: 0, unknown: 0 },
      },
      stateVersion: 2,
      commitBoundaries: [],
      recovery,
      domainResults: [],
      recoveryTargets: [{ connectionId: 'target-connection', objectIds: ['obj-746172676574'] }],
    },
    prepared: null,
    planUnavailableAfterRestart: false,
    deployResult: null,
  } as unknown as SchemaDiffJobDetails;
  return {
    jobId,
    kind: 'schemaDiffApply',
    state: 'failed',
    progress: { read: 0, converted: 0, attempted: 0, committed: 0, unknown: 0 },
    details,
  } as unknown as SchemaDiffTrackedJob;
}

function renderPanel(currentJob: SchemaDiffTrackedJob, onVerify = vi.fn()) {
  return render(
    <SchemaDiffJobStatusPanel
      currentJob={currentJob}
      latestApply={null}
      cancelOutcome="idle"
      verifying={false}
      targetReady={true}
      onCancel={vi.fn()}
      onVerify={onVerify}
    />,
  );
}

describe('SchemaDiffJobStatusPanel recovery actions', () => {
  it('explains a not-dispatched job without offering verification or replay', () => {
    renderPanel(trackedJob({ verdict: 'notExecuted', reasonCode: 'notDispatchedAfterRestart' }, null));

    expect(screen.getByText('schemaDiff.notDispatchedAfterRestart')).toBeInTheDocument();
    expect(screen.queryByRole('button', { name: 'schemaDiff.verifyRecovery' })).not.toBeInTheDocument();
  });

  it('does not describe a live dispatch failure as a restart', () => {
    renderPanel(trackedJob({ verdict: 'notExecuted', reasonCode: 'dispatchFailed' }, null));

    expect(screen.getByText('schemaDiff.notExecuted')).toBeInTheDocument();
    expect(screen.queryByText('schemaDiff.notDispatchedAfterRestart')).not.toBeInTheDocument();
  });

  it('offers explicit read-only verification only for pending jobs with target identity', () => {
    const onVerify = vi.fn();
    renderPanel(
      trackedJob({ verdict: 'pendingVerification', reasonCode: 'restartNeedsVerification' }, 'restartNeedsVerification'),
      onVerify,
    );

    fireEvent.click(screen.getByRole('button', { name: 'schemaDiff.verifyRecovery' }));
    expect(onVerify).toHaveBeenCalledExactlyOnceWith('schemaDiffApply-test');
  });
});
