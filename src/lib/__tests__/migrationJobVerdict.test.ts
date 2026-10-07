/**
 * State-machine tests for the shared P5 verdict engine (T4).
 *
 * The four §2.3 cancel races, the §7 uncertainty ladder and the §9 refusal
 * matchers are the rules the other two migration tools will inherit, so they
 * are pinned here as pure functions rather than through any one window.
 */

import { describe, expect, it } from 'vitest';
import {
  PIPELINE_BUDGET_BYTES,
  deriveCancelDisposition,
  deriveMigrationJobVerdict,
  deriveUncertainty,
  isBackendScopeRejection,
  isMigrationJobInFlight,
  isPipelineBudgetInterception,
  isPlanConsumedRejection,
  readJobCounter,
  sumJobCounters,
  type MigrationJobVerdictInput,
} from '../migrationJobVerdict';

/** A §7 boundary that carries its `EVIDENCE_*` markers. */
const verifiedBoundary = { evidence: ['EVIDENCE_ROWS_COMMITTED'] };
/** A §7 boundary that committed but recorded no evidence. */
const unverifiedBoundary = { evidence: [] };

function input(overrides: Partial<MigrationJobVerdictInput> = {}): MigrationJobVerdictInput {
  return {
    state: 'succeeded',
    effectOutcome: 'completed',
    cancelRequested: false,
    ...overrides,
  };
}

describe('readJobCounter', () => {
  it('reads the decimal string the Rust Counter actually serializes as', () => {
    // `platform-api/src/id.rs` visits `visit_str`; `toCounter` in
    // @datazen/backend-client only accepts a number, so the string form is the
    // one a live Job view really carries.
    expect(readJobCounter('0')).toBe(0);
    expect(readJobCounter('4096')).toBe(4096);
    expect(readJobCounter(' 12 ')).toBe(12);
  });

  it('accepts a plain number but rejects every unreadable shape', () => {
    expect(readJobCounter(7)).toBe(7);
    expect(readJobCounter(-1)).toBeNull();
    expect(readJobCounter(Number.NaN)).toBeNull();
    expect(readJobCounter('')).toBeNull();
    expect(readJobCounter('12.5')).toBeNull();
    expect(readJobCounter('abc')).toBeNull();
    expect(readJobCounter(null)).toBeNull();
    expect(readJobCounter(undefined)).toBeNull();
  });
});

describe('sumJobCounters', () => {
  it('sums mixed wire forms', () => {
    expect(sumJobCounters('2', 3, '4')).toBe(9);
  });

  it('refuses to total an unreadable bucket rather than reporting a wrong sum', () => {
    // A silently-dropped bucket would read as "fewer rows written than the
    // backend reported", which is the one direction that hides a write.
    expect(sumJobCounters('2', null)).toBeNull();
    expect(sumJobCounters('2', 'oops')).toBeNull();
    expect(sumJobCounters()).toBe(0);
  });
});

describe('isMigrationJobInFlight', () => {
  it('treats only queued and running as incomplete states', () => {
    expect(isMigrationJobInFlight('queued')).toBe(true);
    expect(isMigrationJobInFlight('running')).toBe(true);
    expect(isMigrationJobInFlight('succeeded')).toBe(false);
    expect(isMigrationJobInFlight('failed')).toBe(false);
    expect(isMigrationJobInFlight('cancelled')).toBe(false);
  });
});

describe('deriveCancelDisposition — the four §2.3 races', () => {
  it('reports no cancel at all as an intent-free run', () => {
    expect(deriveCancelDisposition('completed', false, [verifiedBoundary], false)).toBe('none');
  });

  it('keeps an unsettled cancel separate from any cancellation', () => {
    // The request raced the write and nothing has been decided yet.
    expect(deriveCancelDisposition(null, true, [], true)).toBe('requestedInFlight');
    expect(deriveCancelDisposition('notStarted', true, [], true)).toBe('requestedInFlight');
  });

  it('separates "cancelled before writing" from "cancelled after committing"', () => {
    expect(deriveCancelDisposition('notStarted', true, [], false)).toBe('notStarted');
    expect(deriveCancelDisposition('notStarted', true, [verifiedBoundary], false)).toBe(
      'settledPartially',
    );
    expect(deriveCancelDisposition('partiallyApplied', true, [verifiedBoundary], false)).toBe(
      'settledPartially',
    );
  });

  it('does not call a lost race a cancellation', () => {
    expect(deriveCancelDisposition('completed', true, [verifiedBoundary], false)).toBe(
      'settledCompleted',
    );
  });

  it('keeps rollback and undecidable apart', () => {
    expect(deriveCancelDisposition('rolledBack', true, [], false)).toBe('settledRolledBack');
    expect(deriveCancelDisposition('unknown', true, [], false)).toBe('settledUnknown');
  });

  it('fails toward "something committed" on an outcome it cannot parse', () => {
    expect(deriveCancelDisposition('partiallyApplied_v2', true, [verifiedBoundary], false)).toBe(
      'settledPartially',
    );
    expect(deriveCancelDisposition('partiallyApplied_v2', true, [], false)).toBe('notStarted');
  });
});

describe('deriveUncertainty — the §7 fail-closed ladder', () => {
  it('calls an undecided outcome uncertain first of all', () => {
    expect(deriveUncertainty('unknown', 'resumeAfterVerify', null, 0)).toBe('effectOutcomeUnknown');
  });

  it('trusts a refusal to resume over an optimistic outcome', () => {
    expect(deriveUncertainty('completed', 'requireManualReview', null, 0)).toBe(
      'manualReviewRequired',
    );
    expect(deriveUncertainty('completed', 'reject', null, 0)).toBe('recoveryRejected');
    expect(deriveUncertainty('completed', 'someFutureVerdict', null, 0)).toBe('recoveryRejected');
  });

  it('takes the recorded reason as evidence even under resumeAfterVerify', () => {
    expect(
      deriveUncertainty('completed', 'resumeAfterVerify', 'no checkpoint was recorded', 0),
    ).toBe('noCheckpoint');
    expect(deriveUncertainty('completed', 'resumeAfterVerify', 'unrelated note', 0)).toBe('none');
  });

  it('never clears a missing-evidence gap', () => {
    expect(deriveUncertainty('completed', null, null, 1)).toBe('missingEvidence');
    expect(deriveUncertainty('completed', null, null, 0, true)).toBe('missingEvidence');
    expect(deriveUncertainty('completed', null, null, 0, false)).toBe('none');
  });
});

describe('deriveMigrationJobVerdict', () => {
  it('calls a fully evidenced run completed', () => {
    const verdict = deriveMigrationJobVerdict(
      input({ commitBoundaries: [verifiedBoundary], committedRows: 12 }),
    );
    expect(verdict).toMatchObject({
      severity: 'ok',
      completed: true,
      verifiedBoundaries: 1,
      unverifiedBoundaries: 0,
      uncertainty: 'none',
      requiresReconcile: false,
      cancelDisposition: 'none',
    });
  });

  it('refuses "completed" when rows were written with no boundary behind them', () => {
    // The real data-transfer shape: `committed` counts rows while the handler
    // never records a §7 boundary.
    const verdict = deriveMigrationJobVerdict(input({ commitBoundaries: [], committedRows: 30 }));
    expect(verdict.completed).toBe(false);
    expect(verdict.severity).toBe('uncertain');
    expect(verdict.uncertainty).toBe('missingEvidence');
    expect(verdict.requiresReconcile).toBe(true);
  });

  it('treats a boundary without EVIDENCE_* as unverified, not committed', () => {
    const verdict = deriveMigrationJobVerdict(
      input({ commitBoundaries: [verifiedBoundary, unverifiedBoundary] }),
    );
    expect(verdict).toMatchObject({
      verifiedBoundaries: 1,
      unverifiedBoundaries: 1,
      severity: 'uncertain',
      completed: false,
    });
  });

  it('keeps uncertainty above a rolled-back run rather than calling it clean', () => {
    const verdict = deriveMigrationJobVerdict(
      input({
        state: 'cancelled',
        effectOutcome: 'rolledBack',
        cancelRequested: true,
        recoveryVerdict: 'requireManualReview',
        recoveryReason: 'no checkpoint was recorded',
      }),
    );
    expect(verdict.uncertainty).toBe('manualReviewRequired');
    expect(verdict.severity).toBe('uncertain');
    expect(verdict.cancelDisposition).toBe('settledRolledBack');
    expect(verdict.requiresReconcile).toBe(true);
  });

  it('shows an in-flight run as partial and reconcileable', () => {
    const verdict = deriveMigrationJobVerdict(
      input({ state: 'running', effectOutcome: null, cancelRequested: false }),
    );
    expect(verdict).toMatchObject({
      severity: 'partial',
      completed: false,
      requiresReconcile: true,
      cancelDisposition: 'none',
    });
  });

  it('shows a queued run with nothing written as not started', () => {
    const verdict = deriveMigrationJobVerdict(
      input({ state: 'cancelled', effectOutcome: 'notStarted', cancelRequested: true }),
    );
    expect(verdict).toMatchObject({
      severity: 'failed',
      cancelDisposition: 'notStarted',
      completed: false,
      requiresReconcile: false,
    });
  });

  it('reports an undecidable cancel as uncertain and reconcileable', () => {
    const verdict = deriveMigrationJobVerdict(
      input({
        state: 'cancelled',
        effectOutcome: 'unknown',
        cancelRequested: true,
        unknownRows: 4,
      }),
    );
    expect(verdict.cancelDisposition).toBe('settledUnknown');
    expect(verdict.severity).toBe('uncertain');
    expect(verdict.requiresReconcile).toBe(true);
  });

  it('reports a lost cancel race as completed on its own merits', () => {
    const verdict = deriveMigrationJobVerdict(
      input({
        cancelRequested: true,
        commitBoundaries: [verifiedBoundary],
        committedRows: 9,
      }),
    );
    expect(verdict).toMatchObject({
      cancelDisposition: 'settledCompleted',
      severity: 'ok',
      completed: true,
    });
  });

  it('reports a terminal error as failed', () => {
    const verdict = deriveMigrationJobVerdict(
      input({ state: 'failed', effectOutcome: 'notStarted', error: 'target refused the DDL' }),
    );
    expect(verdict.severity).toBe('failed');
    expect(verdict.completed).toBe(false);
  });

  it('flags unknown rows as an evidence gap even when the outcome claims completion', () => {
    const verdict = deriveMigrationJobVerdict(input({ commitBoundaries: [], unknownRows: 2 }));
    expect(verdict.uncertainty).toBe('missingEvidence');
    expect(verdict.completed).toBe(false);
  });

  it('has no branch that turns an uncertain verdict into a success', () => {
    const states = ['queued', 'running', 'succeeded', 'failed', 'cancelled'] as const;
    const outcomes = ['notStarted', 'completed', 'rolledBack', 'partiallyApplied', 'unknown'];
    for (const state of states) {
      for (const effectOutcome of outcomes) {
        for (const recoveryVerdict of [null, 'resumeAfterVerify', 'reject', 'requireManualReview']) {
          for (const boundaries of [[], [unverifiedBoundary]] as const) {
            const verdict = deriveMigrationJobVerdict(
              input({ state, effectOutcome, recoveryVerdict, commitBoundaries: boundaries }),
            );
            if (verdict.uncertainty !== 'none') {
              expect(verdict.severity).toBe('uncertain');
              expect(verdict.completed).toBe(false);
              expect(verdict.requiresReconcile).toBe(true);
            }
          }
        }
      }
    }
  });
});

describe('refusal matchers', () => {
  it('names the §6.2 budget cap as 8 MiB', () => {
    expect(PIPELINE_BUDGET_BYTES).toBe(8388608);
    expect(isPipelineBudgetInterception('pipeline budget exceeded for stage users')).toBe(true);
    expect(isPipelineBudgetInterception('PipelineBudget: refusing 12 MiB payload')).toBe(true);
    expect(isPipelineBudgetInterception('budget exceed: 8388608 bytes')).toBe(true);
    expect(isPipelineBudgetInterception('target connection refused the batch')).toBe(false);
    expect(isPipelineBudgetInterception(null)).toBe(false);
  });

  it('recognises a spent plan so the UI offers a re-review, not a retry', () => {
    expect(isPlanConsumedRejection('plan plan-1 is already consumed')).toBe(true);
    expect(isPlanConsumedRejection('PlanAlreadyConsumed')).toBe(true);
    expect(isPlanConsumedRejection('re-review the migration to mint a new plan')).toBe(true);
    expect(isPlanConsumedRejection('connection closed')).toBe(false);
    expect(isPlanConsumedRejection(undefined)).toBe(false);
  });

  it('recognises a §8 scope refusal', () => {
    expect(isBackendScopeRejection('backend scope mismatch')).toBe(true);
    expect(isBackendScopeRejection('expected local-desktop-backend')).toBe(true);
    expect(isBackendScopeRejection('nothing to see')).toBe(false);
  });

  it('keeps the three refusals distinguishable', () => {
    // A single over-broad matcher would label a spent plan as a scope problem
    // and offer the wrong recovery for it.
    const message = 'pipeline budget exceeded for stage users';
    expect(isPipelineBudgetInterception(message)).toBe(true);
    expect(isPlanConsumedRejection(message)).toBe(false);
    expect(isBackendScopeRejection(message)).toBe(false);
  });
});
