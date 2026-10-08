import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { toCounter, type Counter, type JobView } from '@datazen/backend-client';

const cancelMock = vi.hoisted(() => vi.fn());

function counter(value: number): Counter {
  const parsed = toCounter(value);
  if (parsed === undefined) throw new Error(`toCounter rejected ${value}`);
  return parsed;
}

vi.mock('../../../commands/transferJobs', () => ({
  transferJobCommands: { cancel: (jobId: string) => cancelMock(jobId) },
}));

vi.mock('../../../hooks/useI18n', () => ({
  useI18n: () => ({ t: (key: string) => key }),
}));

function jobView(overrides: Partial<JobView> = {}): JobView {
  return {
    jobId: 'transfer-dataTransferApply-1' as JobView['jobId'],
    kind: 'dataTransferApply',
    state: 'running',
    stage: 'data',
    executionIds: [],
    artifactIds: [],
    createdAt: 1 as JobView['createdAt'],
    updatedAt: 1 as JobView['updatedAt'],
    effectOutcome: null,
    cancelRequested: false,
    pendingVerificationReason: null,
    progress: {
      read: counter(8),
      converted: counter(6),
      attempted: counter(4),
      committed: counter(2),
      unknown: counter(0),
    },
    ...overrides,
  };
}

describe('DataTransferAttachedJobs', () => {
  beforeEach(() => {
    cancelMock.mockReset();
    cancelMock.mockResolvedValue(true);
  });

  afterEach(cleanup);

  it('shows reattached progress and keeps a cancel receipt separate from completion', async () => {
    const { DataTransferAttachedJobs } = await import('../DataTransferAttachedJobs');
    render(<DataTransferAttachedJobs jobs={[jobView()]} />);

    const attached = screen.getByTestId('data-transfer-attached-job');
    expect(attached).toHaveAttribute('data-state', 'running');
    expect(screen.getByTestId('data-transfer-attached-job-state')).toHaveTextContent(
      'transfer.job.running',
    );
    expect(screen.getByTestId('data-transfer-attached-job-progress')).toHaveTextContent(
      '8 / 6 / 4 / 2 / 0',
    );

    fireEvent.click(screen.getByTestId('data-transfer-attached-cancel'));

    await waitFor(() => expect(cancelMock).toHaveBeenCalledWith('transfer-dataTransferApply-1'));
    expect(screen.getByTestId('data-transfer-attached-job')).toHaveAttribute(
      'data-cancel-requested',
      'true',
    );
    expect(screen.getByTestId('data-transfer-attached-job-cancel-requested')).toHaveTextContent(
      'migration.cancel.requestedInFlight',
    );
    expect(screen.getByTestId('data-transfer-attached-job')).toHaveAttribute('data-state', 'running');
    expect(screen.queryByText('migration.verdict.ok')).toBeNull();
  });

  it('does not present terminal jobs as still cancellable', async () => {
    const { DataTransferAttachedJobs } = await import('../DataTransferAttachedJobs');
    const { container } = render(
      <DataTransferAttachedJobs jobs={[jobView({ state: 'cancelled', cancelRequested: true })]} />,
    );

    expect(container.firstChild).toBeNull();
    expect(cancelMock).not.toHaveBeenCalled();
  });
});
