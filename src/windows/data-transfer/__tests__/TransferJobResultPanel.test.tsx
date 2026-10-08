/**
 * Plumbing contract for the transfer-specific result surface.
 *
 * The panel itself holds no verdict logic — that lives in
 * `src/lib/migrationJobVerdict.ts` and the shared
 * `MigrationJobVerdictPanel`. What is pinned here is only the glue a window
 * depends on:
 *
 *  - the plan identity of the apply reply is exposed as `data-*` (the e2e
 *    spec replays exactly those values over IPC), and
 *  - a refusal with no admitted Job renders *no* verdict panel at all, because
 *    drawing one would imply a run happened.
 */

import { describe, expect, it, beforeAll, afterEach } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import {
  toCounter,
  toId,
  toTimestamp,
  type CommitBoundary,
  type Counter,
  type JobProgress,
} from '@datazen/backend-client';
import { TransferJobResultPanel } from '../TransferJobResultPanel';
import { deriveMigrationJobVerdict } from '../../../lib/migrationJobVerdict';
import type { TransferJobRun } from '../../../hooks/useTransferJobRun';
import type { TransferApplyJobView } from '../../../commands/transferJobs';
import { ensureAllLazyDomains } from '../../../locales/lazyPacks';

function counter(value: number): Counter {
  const read = toCounter(value);
  if (read === undefined) throw new Error(`toCounter rejected ${value}`);
  return read;
}

function progress(committed: number): JobProgress {
  return {
    read: counter(committed),
    converted: counter(committed),
    attempted: counter(committed),
    committed: counter(committed),
    unknown: counter(0),
  };
}

function id(value: string): CommitBoundary['stageId'] {
  const read = toId(value);
  if (read === undefined) throw new Error(`toId rejected ${value}`);
  return read;
}

function timestamp(value: number): CommitBoundary['committedAt'] {
  const read = toTimestamp(value);
  if (read === undefined) throw new Error(`toTimestamp rejected ${value}`);
  return read;
}

/** A boundary carrying evidence, i.e. one the verdict may certify. */
function commitBoundary(stage: string): CommitBoundary {
  return {
    stageId: id(stage),
    stableTargetFingerprint: `fingerprint-${stage}`,
    committedAt: timestamp(1_700_000_000_000),
    operationId: id(`op-${stage}`),
    batchId: id(`batch-${stage}`),
    payloadDigest: `digest-${stage}`,
    evidence: ['rows=3', 'confirmed=checkpoint'],
    verifiedAt: timestamp(1_700_000_001_000),
  };
}

const applyView: TransferApplyJobView = {
  jobId: 'job-7',
  kind: 'dataTransfer',
  state: 'succeeded',
  effectOutcome: 'completed',
  progress: progress(3),
  planId: 'plan-42',
  planDigest: 'sha256:abc123',
  selectionRevision: 3,
  commitBoundaries: [commitBoundary('xfer_job_1')],
  artifactIds: [],
  cancelled: false,
  partial: false,
  replayed: false,
  error: null,
  recoveryVerdict: 'resumeAfterVerify',
  recoveryResumeThrough: null,
  recoveryReason: null,
};

/** A settled run with the given apply reply; nothing else is under test. */
function settledRun(overrides: Partial<TransferJobRun> = {}): TransferJobRun {
  return {
    phase: 'settled',
    prepareView: null,
    applyView,
    failure: null,
    cancelRequested: false,
    cancelAcknowledged: false,
    cancelUnknownJob: false,
    verdict: deriveMigrationJobVerdict({
      state: 'succeeded',
      effectOutcome: 'completed',
      cancelRequested: false,
      committedRows: 3,
      commitBoundaries: [commitBoundary('xfer_job_1')],
    }),
    cancelTargetJobId: 'job-7',
    isInFlight: false,
    prepare: async () => null,
    apply: async () => applyView,
    requestCancel: async () => false,
    reset: () => {},
    reprepare: () => {},
    lastFailure: () => null,
    ...overrides,
  };
}

describe('TransferJobResultPanel', () => {
  beforeAll(async () => {
    // `migration.*` lives in the lazily loaded `sync` pack; without this the
    // panel would render raw keys and these assertions would prove nothing.
    await ensureAllLazyDomains('en');
  });

  afterEach(() => {
    cleanup();
  });

  it('exposes the spent plan identity on the result root and the verdict panel', () => {
    render(<TransferJobResultPanel run={settledRun()} onReReview={() => {}} />);

    const root = screen.getByTestId('data-transfer-result');
    expect(root).toHaveAttribute('data-plan-id', 'plan-42');
    expect(root).toHaveAttribute('data-completed', 'true');
    expect(root).toHaveAttribute('data-verdict-severity', 'ok');

    const verdict = screen.getByTestId('migration-job-verdict');
    expect(verdict).toHaveAttribute('data-plan-id', 'plan-42');
    expect(verdict).toHaveAttribute('data-plan-digest', 'sha256:abc123');
    expect(verdict).toHaveAttribute('data-selection-revision', '3');
    // Committed rows with an evidenced boundary behind them certify.
    expect(verdict).toHaveAttribute('data-uncertainty', 'none');
    expect(verdict).toHaveAttribute('data-verified-boundaries', '1');
    expect(verdict).toHaveAttribute('data-unverified-boundaries', '0');
  });

  it('renders no plan and no verdict when the backend refused before admitting a Job', () => {
    // backendScope: nothing ran, so there is no plan and no outcome.
    render(
      <TransferJobResultPanel
        run={settledRun({
          phase: 'blocked',
          applyView: null,
          verdict: null,
          failure: {
            kind: 'backendScope',
            message: 'a remote backend cannot reach a client-local profile',
          },
        })}
        onReReview={() => {}}
      />,
    );

    const root = screen.getByTestId('data-transfer-result');
    expect(root.hasAttribute('data-plan-id')).toBe(false);
    expect(root).toHaveAttribute('data-completed', 'false');
    expect(screen.queryByTestId('migration-job-verdict')).toBeNull();
    expect(screen.getByTestId('migration-job-failure')).toHaveAttribute(
      'data-fail-closed',
      'true',
    );
  });
});