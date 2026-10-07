/**
 * Rendering contract for the shared verdict surface.
 *
 * The panel is what all three migration tools show, so the rules pinned here
 * are the rendering rules: uncertainty is stated as uncertainty, a boundary
 * with no `EVIDENCE_*` marker is shown as unverified, an empty boundary list
 * over committed rows is announced as an evidence gap, and an artifact id
 * is displayed rather than swallowed.
 *
 * These tests deliberately run against the *real* locale packs: a verdict panel
 * that renders raw `migration.*` keys to a user is itself a defect.
 */

import { describe, expect, it, beforeAll, afterEach } from 'vitest';
import { cleanup, render, screen } from '@testing-library/react';
import type { ComponentProps } from 'react';
import {
  toCounter,
  toId,
  toTimestamp,
  type CommitBoundary,
  type Counter,
  type JobProgress,
  type Timestamp,
} from '@datazen/backend-client';
import { MigrationJobVerdictPanel } from '../MigrationJobVerdictPanel';
import {
  deriveMigrationJobVerdict,
  type MigrationJobVerdictInput,
} from '../../../lib/migrationJobVerdict';
import { ensureAllLazyDomains } from '../../../locales/lazyPacks';

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

function progress(overrides: Partial<Record<keyof JobProgress, number>> = {}): JobProgress {
  return {
    read: counter(overrides.read ?? 0),
    converted: counter(overrides.converted ?? 0),
    attempted: counter(overrides.attempted ?? 0),
    committed: counter(overrides.committed ?? 0),
    unknown: counter(overrides.unknown ?? 0),
  };
}

function boundary(stage: string, evidence: readonly string[]): CommitBoundary {
  return {
    stageId: id(stage),
    stableTargetFingerprint: `fingerprint-${stage}`,
    committedAt: timestamp(1_700_000_000_000),
    operationId: id(`op-${stage}`),
    batchId: id(`batch-${stage}`),
    payloadDigest: `digest-${stage}`,
    evidence,
    verifiedAt: timestamp(1_700_000_001_000),
  };
}

/** Render the panel the way a window does: verdict derived, then displayed. */
function renderVerdict(
  input: MigrationJobVerdictInput,
  extra: Partial<ComponentProps<typeof MigrationJobVerdictPanel>> = {},
) {
  const verdict = deriveMigrationJobVerdict(input);
  return render(<MigrationJobVerdictPanel verdict={verdict} {...extra} />);
}

describe('MigrationJobVerdictPanel', () => {
  beforeAll(async () => {
    // The `migration.*` keys live in the lazily loaded `sync` domain pack; a
    // mocked `useLocaleDomains` would render raw keys instead of English.
    await ensureAllLazyDomains('en');
  });

  afterEach(() => {
    // This project registers matchers manually rather than through
    // `@testing-library/react/vitest`, so the automatic cleanup hook is absent
    // and each render has to be unmounted before the next assertion.
    cleanup();
  });

  it('states an indeterminate outcome as uncertain, never as a completed run', () => {
    renderVerdict(
      {
        state: 'cancelled',
        effectOutcome: 'unknown',
        cancelRequested: true,
        recoveryVerdict: 'requireManualReview',
        recoveryReason: 'the target never confirmed the batch',
        unknownRows: 7,
      },
      { cancelRequested: true, progress: progress({ read: 10, unknown: 7 }) },
    );

    const root = screen.getByTestId('migration-job-verdict');
    expect(root).toHaveAttribute('data-severity', 'uncertain');
    expect(root).toHaveAttribute('data-cancel-disposition', 'settledUnknown');
    expect(root).toHaveAttribute('data-uncertainty', 'effectOutcomeUnknown');
    expect(screen.getByTestId('migration-job-verdict-status')).toHaveTextContent(
      'Outcome uncertain',
    );
    // The words "Completed" / "Partially applied" must not appear anywhere.
    expect(document.body.textContent).not.toContain('Completed');
    expect(document.body.textContent).not.toContain('Partially applied');
    expect(screen.getByTestId('migration-job-uncertainty')).toHaveTextContent(
      'The database did not confirm whether the write committed or rolled back.',
    );
    expect(screen.getByTestId('migration-job-reconcile')).toHaveTextContent(
      'Verify the target against the recorded boundaries before applying anything else.',
    );
  });

  it('honours a backend that refuses recovery even when the outcome says completed', () => {
    // The outcome flag is the backend's progress signal; the recovery
    // adjudication decides whether the write may be certified.
    renderVerdict(
      {
        state: 'succeeded',
        effectOutcome: 'completed',
        cancelRequested: false,
        recoveryVerdict: 'requireManualReview',
        recoveryReason: 'target fingerprint drifted',
      },
      { recoveryVerdict: 'requireManualReview' },
    );

    expect(screen.getByTestId('migration-job-verdict')).toHaveAttribute(
      'data-uncertainty',
      'manualReviewRequired',
    );
    expect(screen.getByTestId('migration-job-uncertainty')).toHaveTextContent(
      'The backend requires manual review before anything else is applied.',
    );
    expect(screen.getByTestId('migration-job-verdict-status')).not.toHaveTextContent('Completed');
  });

  it('explains a missing evidence gap instead of showing an empty list', () => {
    // The real data-transfer shape: rows were committed, and the handler never
    // recorded a boundary for them.
    renderVerdict(
      { state: 'succeeded', effectOutcome: 'completed', cancelRequested: false, committedRows: 30 },
      { progress: progress({ read: 30, attempted: 30, committed: 30 }) },
    );

    const empty = screen.getByTestId('migration-job-boundaries-empty');
    expect(empty).toHaveAttribute('data-evidence-gap', 'true');
    expect(empty).toHaveTextContent(
      'No write boundary was recorded, so nothing can be reported as committed.',
    );
    expect(screen.getByTestId('migration-job-uncertainty')).toHaveTextContent(
      'At least one committed boundary has no evidence, so it is reported as unverified.',
    );
    expect(screen.getByTestId('migration-job-verdict')).toHaveAttribute('data-severity', 'uncertain');
  });

  it('does not call an empty boundary list a gap when nothing was written', () => {
    renderVerdict(
      { state: 'cancelled', effectOutcome: 'notStarted', cancelRequested: true, committedRows: 0 },
      { progress: progress() },
    );

    expect(screen.getByTestId('migration-job-boundaries-empty')).toHaveAttribute(
      'data-evidence-gap',
      'false',
    );
    expect(screen.queryByTestId('migration-job-uncertainty')).toBeNull();
    expect(screen.queryByTestId('migration-job-reconcile')).toBeNull();
  });

  it('shows the recovery verdict, reason and resume boundary', () => {
    renderVerdict(
      {
        state: 'succeeded',
        effectOutcome: 'partiallyApplied',
        cancelRequested: false,
        commitBoundaries: [boundary('users', ['EVIDENCE_ROWS_COMMITTED']), boundary('orders', [])],
        recoveryVerdict: 'reject',
        recoveryReason: 'target fingerprint drifted',
      },
      {
        recoveryVerdict: 'reject',
        recoveryReason: 'target fingerprint drifted',
        commitBoundaries: [
          boundary('users', ['EVIDENCE_ROWS_COMMITTED']),
          boundary('orders', []),
        ],
      },
    );

    expect(screen.getByTestId('migration-job-recovery-verdict')).toHaveTextContent('reject');
    expect(screen.getByTestId('migration-job-recovery-reason')).toHaveTextContent(
      'target fingerprint drifted',
    );
    expect(screen.getByTestId('migration-job-verdict')).toHaveAttribute('data-severity', 'uncertain');
  });

  it('reports how far recovery is allowed to resume', () => {
    renderVerdict(
      {
        state: 'succeeded',
        effectOutcome: 'completed',
        cancelRequested: false,
        commitBoundaries: [boundary('users', ['EVIDENCE_ROWS_COMMITTED'])],
        recoveryVerdict: 'resumeAfterVerify',
      },
      { recoveryVerdict: 'resumeAfterVerify', recoveryResumeThrough: 2 },
    );

    expect(screen.getByTestId('migration-job-recovery-verdict')).toHaveTextContent(
      'Recovery verdict',
    );
    expect(screen.getByTestId('migration-job-recovery-resume-through')).toHaveTextContent('2');
  });

  it('marks each boundary verified or unverified from its own evidence', () => {
    renderVerdict(
      {
        state: 'succeeded',
        effectOutcome: 'completed',
        cancelRequested: false,
        commitBoundaries: [boundary('users', ['EVIDENCE_ROWS_COMMITTED']), boundary('orders', [])],
      },
      {
        commitBoundaries: [boundary('users', ['EVIDENCE_ROWS_COMMITTED']), boundary('orders', [])],
      },
    );

    expect(screen.getByTestId('migration-job-boundary-users')).toHaveAttribute(
      'data-boundary-verified',
      'true',
    );
    expect(screen.getByTestId('migration-job-boundary-orders')).toHaveAttribute(
      'data-boundary-verified',
      'false',
    );
    expect(screen.getByTestId('migration-job-boundary-orders')).toHaveTextContent(
      'Committed without evidence — unverified',
    );
    expect(screen.getByTestId('migration-job-verdict')).toHaveAttribute(
      'data-verified-boundaries',
      '1',
    );
    expect(screen.getByTestId('migration-job-verdict')).toHaveAttribute(
      'data-unverified-boundaries',
      '1',
    );
  });

  it('keeps a cancel request apart from where the run stopped', () => {
    renderVerdict(
      { state: 'running', effectOutcome: null, cancelRequested: true },
      { cancelRequested: true },
    );

    expect(screen.getByTestId('migration-job-cancel')).toHaveTextContent(
      'Cancel was requested while the run was in flight. The stopping point is not known yet',
    );
    expect(screen.getByTestId('migration-job-verdict')).toHaveAttribute(
      'data-cancel-disposition',
      'requestedInFlight',
    );
    expect(screen.queryByTestId('migration-job-cancel-unknown-job')).toBeNull();
  });

  it('says a cancel had no target when the backend had no such Job', () => {
    // `cancel_data_transfer` returns false for a Job it does not know; that is
    // not a cancellation and must not be reported as one.
    renderVerdict(
      { state: 'running', effectOutcome: null, cancelRequested: true },
      { cancelRequested: true, cancelAcknowledged: false },
    );

    expect(screen.getByTestId('migration-job-cancel-unknown-job')).toHaveTextContent(
      'The backend has no Job for this run, so the cancel request had no target.',
    );
  });

  it('shows a replayed receipt instead of implying a second write', () => {
    renderVerdict(
      {
        state: 'succeeded',
        effectOutcome: 'completed',
        cancelRequested: false,
        commitBoundaries: [boundary('users', ['EVIDENCE_ROWS_COMMITTED'])],
        committedRows: 5,
      },
      { replayed: true },
    );

    expect(screen.getByTestId('migration-job-replayed')).toHaveTextContent(
      'This reply came from the recorded receipt for a run that already happened.',
    );
  });

  it('surfaces a SQL-file artifact id, the only proof that run produced', () => {
    renderVerdict(
      { state: 'succeeded', effectOutcome: 'completed', cancelRequested: false, committedRows: 0 },
      { progress: progress(), artifactIds: ['transfer-sql-a1b2c3d4'] },
    );

    const artifact = screen.getByTestId('migration-job-artifact');
    expect(artifact).toHaveAttribute('data-artifact-id', 'transfer-sql-a1b2c3d4');
    expect(artifact).toHaveTextContent('transfer-sql-a1b2c3d4');
    expect(screen.getByTestId('migration-job-verdict')).toHaveAttribute(
      'data-verified-boundaries',
      '0',
    );
  });

  it('exposes the plan identity as data-* so a spec can name the spent plan', () => {
    // The replayed reply carries exactly these three values over IPC; if they
    // were only rendered as prose the spec could not assert "this plan, refused".
    renderVerdict(
      {
        state: 'succeeded',
        effectOutcome: 'completed',
        cancelRequested: false,
        commitBoundaries: [boundary('users', ['EVIDENCE_ROWS_COMMITTED'])],
      },
      {
        plan: { planId: 'plan-42', planDigest: 'sha256:abc123', selectionRevision: 3 },
      },
    );

    const root = screen.getByTestId('migration-job-verdict');
    expect(root).toHaveAttribute('data-plan-id', 'plan-42');
    expect(root).toHaveAttribute('data-plan-digest', 'sha256:abc123');
    expect(root).toHaveAttribute('data-selection-revision', '3');
  });

  it('omits the plan attributes when the backend sent no plan identity', () => {
    // A refusal reaches this panel without an admitted plan. Emitting
    // `data-plan-id=""` would let a spec pass on a plan that never existed.
    renderVerdict(
      { state: 'failed', effectOutcome: 'notStarted', cancelRequested: false, committedRows: 0 },
      {},
    );

    const root = screen.getByTestId('migration-job-verdict');
    expect(root.hasAttribute('data-plan-id')).toBe(false);
    expect(root.hasAttribute('data-plan-digest')).toBe(false);
    expect(root.hasAttribute('data-selection-revision')).toBe(false);
  });

  it('binds its data-* attributes to the caller prefix for WDIO specs', () => {
    renderVerdict(
      { state: 'succeeded', effectOutcome: 'completed', cancelRequested: false, committedRows: 0 },
      { testIdPrefix: 'data-transfer-job' },
    );

    expect(screen.getByTestId('data-transfer-job-verdict')).toBeTruthy();
    expect(screen.queryByTestId('migration-job-verdict')).toBeNull();
  });
});
