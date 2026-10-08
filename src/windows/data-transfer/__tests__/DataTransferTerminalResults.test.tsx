import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import {
  toCounter,
  toId,
  toTimestamp,
  type CommitBoundary,
  type Counter,
  type JobDetails,
  type JobView,
} from '@datazen/backend-client';

const getDetailsMock = vi.hoisted(() => vi.fn());
type JobViewFixture = JobView & { error: string | null };

vi.mock('../../../commands/transferJobs', () => ({
  transferJobCommands: { getDetails: getDetailsMock },
}));

vi.mock('../../../hooks/useLocaleDomains', () => ({ useLocaleDomains: () => undefined }));
vi.mock('../../../hooks/useI18n', () => ({ useI18n: () => ({ t: (key: string) => key }) }));

function counter(value: number): Counter {
  const parsed = toCounter(value);
  if (parsed === undefined) throw new Error(`toCounter rejected ${value}`);
  return parsed;
}

function id(value: string): CommitBoundary['stageId'] {
  const parsed = toId(value);
  if (parsed === undefined) throw new Error(`toId rejected ${value}`);
  return parsed;
}

function timestamp(value: number): CommitBoundary['committedAt'] {
  const parsed = toTimestamp(value);
  if (parsed === undefined) throw new Error(`toTimestamp rejected ${value}`);
  return parsed;
}

function jobView(overrides: Partial<JobViewFixture> = {}): JobViewFixture {
  return {
    jobId: 'transfer-dataTransferApply-1' as JobView['jobId'],
    kind: 'dataTransferApply',
    state: 'succeeded',
    stage: 'data',
    executionIds: [],
    artifactIds: [],
    createdAt: 1 as JobView['createdAt'],
    updatedAt: 2 as JobView['updatedAt'],
    effectOutcome: 'completed',
    cancelRequested: false,
    pendingVerificationReason: null,
    error: null,
    progress: {
      read: counter(3),
      converted: counter(3),
      attempted: counter(3),
      committed: counter(3),
      unknown: counter(0),
    },
    ...overrides,
  };
}

function boundary(): CommitBoundary {
  return {
    stageId: id('stage-1'),
    stableTargetFingerprint: 'target-fingerprint',
    committedAt: timestamp(1_700_000_000_000),
    operationId: id('operation-1'),
    batchId: id('batch-1'),
    payloadDigest: 'sha256:payload',
    evidence: ['rows=3', 'confirmed=checkpoint'],
    verifiedAt: timestamp(1_700_000_001_000),
  };
}

function details(job = jobView()): JobDetails {
  return {
    job,
    planId: 'plan-1',
    planDigest: 'sha256:plan',
    selectionRevision: 4,
    commitBoundaries: [boundary()],
    recovery: { verdict: 'resumeAfterVerify', resumeThrough: 1 },
    stateVersion: counter(1),
    domainResults: [],
    recoveryTargets: [],
  };
}

describe('DataTransferTerminalResults', () => {
  beforeEach(() => {
    getDetailsMock.mockReset();
  });

  afterEach(cleanup);

  it('restores the exact plan, progress and committed boundaries from JobDetails', async () => {
    const settled = jobView();
    getDetailsMock.mockResolvedValue(details(settled));
    const { DataTransferTerminalResults } = await import('../DataTransferTerminalResults');
    render(<DataTransferTerminalResults jobs={[settled]} />);

    const result = screen.getByTestId('data-transfer-result');
    expect(result).toHaveAttribute('data-completed', 'false');
    expect(result).toHaveAttribute('data-verdict-severity', 'uncertain');

    await waitFor(() => expect(getDetailsMock).toHaveBeenCalledWith(settled.jobId));
    await waitFor(() => expect(screen.getByTestId('data-transfer-result')).toHaveAttribute('data-completed', 'true'));
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute('data-plan-id', 'plan-1');
    const verdict = screen.getByTestId('data-transfer-job-verdict');
    expect(verdict).toHaveAttribute('data-plan-digest', 'sha256:plan');
    expect(verdict).toHaveAttribute('data-selection-revision', '4');
    expect(verdict).toHaveAttribute('data-verified-boundaries', '1');
    expect(screen.getByTestId('data-transfer-job-boundaries')).toBeInTheDocument();
  });

  it('keeps an older terminal result hidden while a newer apply Job is active', async () => {
    const older = jobView({ jobId: 'transfer-dataTransferApply-old' as JobView['jobId'] });
    const newer = jobView({
      jobId: 'transfer-dataTransferApply-new' as JobView['jobId'],
      state: 'running',
      createdAt: 3 as JobView['createdAt'],
    });
    const { DataTransferTerminalResults } = await import('../DataTransferTerminalResults');
    const { container } = render(<DataTransferTerminalResults jobs={[older, newer]} />);

    expect(container.firstChild).toBeNull();
    expect(getDetailsMock).not.toHaveBeenCalled();
  });

  it('keeps a pending recovery verdict uncertain after reopening', async () => {
    const settled = jobView();
    getDetailsMock.mockResolvedValue({
      ...details(settled),
      recovery: { verdict: 'pendingVerification', reasonCode: 'restartNeedsVerification' },
    } satisfies JobDetails);
    const { DataTransferTerminalResults } = await import('../DataTransferTerminalResults');
    render(<DataTransferTerminalResults jobs={[settled]} />);

    await waitFor(() => expect(getDetailsMock).toHaveBeenCalledWith(settled.jobId));
    await waitFor(() =>
      expect(screen.getByTestId('data-transfer-result')).toHaveAttribute('data-completed', 'false'),
    );
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute(
      'data-verdict-severity',
      'uncertain',
    );
    expect(screen.getByTestId('data-transfer-job-recovery-verdict')).toHaveAttribute(
      'data-verdict',
      'pendingVerification',
    );
  });

  it('keeps the report uncertain when persisted details cannot be read', async () => {
    const settled = jobView();
    getDetailsMock.mockRejectedValue(new Error('private backend detail'));
    const { DataTransferTerminalResults } = await import('../DataTransferTerminalResults');
    render(<DataTransferTerminalResults jobs={[settled]} />);

    await waitFor(() => expect(screen.getByRole('status')).toHaveTextContent('transfer.job.detailsUnavailable'));
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute('data-completed', 'false');
    expect(screen.getByTestId('data-transfer-result')).toHaveAttribute('data-verdict-severity', 'uncertain');
    expect(screen.queryByText('private backend detail')).toBeNull();
  });
});
