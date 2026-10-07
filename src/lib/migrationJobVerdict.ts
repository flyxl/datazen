/**
 * Presentation model for a P5 Job verdict — deliberately domain-neutral.
 *
 * Data Transfer, Schema Diff and Data Sync all end in the same question: "did
 * my write land, and how much of it?". The backend already answers it in the
 * only terms that can survive a lost response: durable `CommitBoundary`
 * records plus a derived `EffectOutcome`, never a boolean. This module turns
 * that raw verdict into the small number of states a user can be *shown*, and
 * it is the seam the other two tools reuse instead of re-deriving their own.
 *
 * Two rules are load-bearing and must not be relaxed:
 *
 * 1. "Cancel requested" is not "cancelled". Cancellation is a request that races
 *    four ways against the write; only the boundary list and the effect outcome
 *    say where the work actually stopped. `deriveCancelDisposition` keeps those
 *    two facts separate so a UI can never render a cancel click as a completed
 *    rollback.
 * 2. An uncertain verdict stays uncertain. Recovery fails closed: when no
 *    checkpoint was recorded, or evidence is missing, it returns
 *    `requireManualReview` / `reject`. Those map to `severity: 'uncertain'`
 *    and never to `'ok'` — beautifying "we don't know" into "success" is the
 *    defect this type exists to prevent.
 */

import type { CommitBoundary, Counter, EffectOutcome, JobState } from '@datazen/backend-client';

/** The cancel races, as the *user* must see them. */
export type CancelDisposition =
  /** No cancel was ever requested. */
  | 'none'
  /** Cancel reached a queued job: it never started a single write. */
  | 'notStarted'
  /** Cancel reached an in-flight job; the verdict has not landed yet. */
  | 'requestedInFlight'
  /** Cancel reached an in-flight job and work had already committed. */
  | 'settledPartially'
  /** Cancel reached an in-flight job and the transaction still rolled back. */
  | 'settledRolledBack'
  /** Cancel lost the race: the run had already completed on its own. */
  | 'settledCompleted'
  /** Cancel reached an in-flight job and the outcome is not decidable. */
  | 'settledUnknown';

/** How strongly the UI must phrase the result. `uncertain` outranks `partial`. */
export type VerdictSeverity = 'ok' | 'partial' | 'uncertain' | 'failed';

/** The three values `apply_*_job` can put in `recoveryVerdict`. */
export type RecoveryVerdict = 'resumeAfterVerify' | 'reject' | 'requireManualReview';

/** Why the verdict is uncertain, so the UI can explain it instead of guessing. */
export type UncertaintyReason =
  | 'none'
  /** No checkpoint existed, so nothing about the write is known. */
  | 'noCheckpoint'
  /** Recovery explicitly refused to resume. */
  | 'recoveryRejected'
  /** The backend could not derive an effect outcome. */
  | 'effectOutcomeUnknown'
  /** A committed boundary reached the target but carries no verification evidence. */
  | 'missingEvidence'
  /** Recovery asks for a human before anything is resumed. */
  | 'manualReviewRequired';

export interface MigrationJobVerdictInput {
  state: JobState | string;
  effectOutcome: EffectOutcome | string | null;
  /** Cancel intent, as reported by the Job view / cancel receipt. */
  cancelRequested: boolean;
  /** Durable write markers. Empty means "provably wrote nothing". */
  commitBoundaries?: readonly Pick<CommitBoundary, 'evidence'>[] | null;
  recoveryVerdict?: string | null;
  recoveryReason?: string | null;
  /** Terminal error text, when the Job failed rather than completed. */
  error?: string | null;
  /**
   * The committed / unknown progress counters.
   *
   * Needed because commit boundaries alone cannot tell "wrote nothing" from
   * "wrote something nobody recorded". The data-transfer handler never calls
   * `record_commit_boundary`, so its boundary list is structurally empty while
   * `committed` still counts real rows; without this the UI would show a clean
   * "Completed" over an unverifiable write.
   */
  committedRows?: number | null;
  /** Rows the backend itself could not account for. */
  unknownRows?: number | null;
}

export interface MigrationJobVerdict {
  severity: VerdictSeverity;
  cancelDisposition: CancelDisposition;
  /** True only when the write finished and nothing is left to verify. */
  completed: boolean;
  /** Boundaries carrying `EVIDENCE_*` markers. */
  verifiedBoundaries: number;
  /** Boundaries present but unverified — fail-closed evidence gap. */
  unverifiedBoundaries: number;
  uncertainty: UncertaintyReason;
  /**
   * True when the run must not be retried blindly: either the outcome is
   * undecidable or the cancel has not settled, and a second apply attempt
   * could double-write.
   */
  requiresReconcile: boolean;
}

/**
 * Read a progress counter.
 *
 * The Rust `Counter` serializes as a **decimal string** (`visit_str` /
 * `visit_u64` / `visit_i64` in `platform-api/src/id.rs`), while
 * `toCounter` from `@datazen/backend-client` only accepts a non-negative
 * `number`. Routing a real backend payload through that helper silently
 * yields `undefined`, so this reader accepts both wire forms. The
 * package-level gap is reported upstream; until it is fixed, none of the three
 * tools may lose every counter to it.
 */
export function readJobCounter(
  value: Counter | number | string | null | undefined,
): number | null {
  if (typeof value === 'number') {
    return Number.isFinite(value) && value >= 0 ? Math.trunc(value) : null;
  }
  if (typeof value === 'string') {
    const trimmed = value.trim();
    if (!/^\d+$/.test(trimmed)) return null;
    const parsed = Number.parseInt(trimmed, 10);
    return Number.isSafeInteger(parsed) ? parsed : null;
  }
  return null;
}

/** Sum progress buckets, or `null` when any bucket is unreadable. */
export function sumJobCounters(
  ...values: readonly (Counter | number | string | null | undefined)[]
): number | null {
  let total = 0;
  for (const value of values) {
    const read = readJobCounter(value);
    if (read === null) return null;
    total += read;
  }
  return total;
}

/** True while the Job is still queued or running. */
export function isMigrationJobInFlight(state: JobState | string): boolean {
  return state === 'queued' || state === 'running';
}

/**
 * Derive the cancel disposition.
 *
 * The `cancelled` / `partial` booleans the old command returned are not inputs
 * on purpose: they cannot distinguish "cancelled before writing" from
 * "cancelled after committing", which is the entire point of the flag.
 */
export function deriveCancelDisposition(
  effectOutcome: EffectOutcome | string | null,
  cancelRequested: boolean,
  boundaries: readonly Pick<CommitBoundary, 'evidence'>[] | null | undefined,
  inFlight: boolean,
): CancelDisposition {
  if (!cancelRequested) return 'none';
  if (inFlight) return 'requestedInFlight';
  const committed = (boundaries?.length ?? 0) > 0;
  switch (effectOutcome) {
    case 'completed':
      // The write finished before the cancel landed; reporting this as
      // "cancelled" would be the exact lie the cancel model forbids.
      return 'settledCompleted';
    case 'rolledBack':
      return 'settledRolledBack';
    case 'partiallyApplied':
      return 'settledPartially';
    case 'unknown':
      return 'settledUnknown';
    default:
      // `notStarted`, an absent outcome, or an unknown string. A committed
      // boundary still contradicts "nothing happened".
      return committed ? 'settledPartially' : 'notStarted';
  }
}

/**
 * Recovery adjudication. Anything other than `resumeAfterVerify` means the
 * backend refuses to certify the write, so the caller must not call it success.
 */
export function deriveUncertainty(
  effectOutcome: EffectOutcome | string | null,
  recoveryVerdict: string | null | undefined,
  recoveryReason: string | null | undefined,
  unverifiedBoundaries: number,
  /**
   * True when the backend reported committed or unknown rows but attached no
   * boundary to prove them.
   */
  hasUnbackedCommit = false,
): UncertaintyReason {
  if (effectOutcome === 'unknown') return 'effectOutcomeUnknown';
  if (recoveryVerdict === 'requireManualReview') return 'manualReviewRequired';
  if (recoveryVerdict === 'reject') return 'recoveryRejected';
  if (recoveryVerdict && recoveryVerdict !== 'resumeAfterVerify') return 'recoveryRejected';
  // A recorded reason is the fail-closed evidence even when the verdict
  // itself stays optimistic; "no checkpoint was recorded" is the canonical one.
  if (recoveryReason && /no checkpoint/i.test(recoveryReason)) return 'noCheckpoint';
  // Rows without a boundary behind them are an evidence gap, checked before the
  // per-boundary count because a structurally empty list would otherwise make
  // this unreachable.
  if (hasUnbackedCommit) return 'missingEvidence';
  if (unverifiedBoundaries > 0) return 'missingEvidence';
  return 'none';
}

/** Fold the raw Job verdict into the state a user is shown. */
export function deriveMigrationJobVerdict(
  input: MigrationJobVerdictInput,
): MigrationJobVerdict {
  const boundaries = input.commitBoundaries ?? [];
  const verifiedBoundaries = boundaries.filter(
    (boundary) => Array.isArray(boundary.evidence) && boundary.evidence.length > 0,
  ).length;
  const unverifiedBoundaries = boundaries.length - verifiedBoundaries;
  const inFlight = isMigrationJobInFlight(input.state);
  const cancelDisposition = deriveCancelDisposition(
    input.effectOutcome,
    input.cancelRequested,
    boundaries,
    inFlight,
  );
  const uncertainty = deriveUncertainty(
    input.effectOutcome,
    input.recoveryVerdict ?? null,
    input.recoveryReason ?? null,
    unverifiedBoundaries,
    // A commit is only trustworthy when a boundary backs it. Rows with
    // no boundary at all are strictly worse than a boundary missing its
    // `EVIDENCE_*` markers, and both must fail closed.
    boundaries.length === 0 &&
      ((input.committedRows ?? 0) > 0 || (input.unknownRows ?? 0) > 0),
  );
  // "Completed" means finished *and* certifiable. An `effectOutcome` of
  // `completed` alone is the backend's progress flag, not proof — letting it
  // pass while uncertainty is open is how an unverifiable write gets presented
  // as a clean success.
  const completed =
    input.effectOutcome === 'completed' &&
    unverifiedBoundaries === 0 &&
    !inFlight &&
    uncertainty === 'none';

  const severity = pickSeverity({
    inFlight,
    uncertainty,
    completed,
    state: input.state,
    cancelDisposition,
    hasError: Boolean(input.error),
  });

  return {
    severity,
    cancelDisposition,
    completed,
    verifiedBoundaries,
    unverifiedBoundaries,
    uncertainty,
    requiresReconcile:
      inFlight || uncertainty !== 'none' || cancelDisposition === 'settledUnknown',
  };
}

function pickSeverity(input: {
  inFlight: boolean;
  uncertainty: UncertaintyReason;
  completed: boolean;
  state: string;
  cancelDisposition: CancelDisposition;
  hasError: boolean;
}): VerdictSeverity {
  // Uncertainty outranks everything: an undecided verdict must never
  // be presented as a clean one.
  if (input.uncertainty !== 'none') return 'uncertain';
  if (input.completed) return 'ok';
  if (input.inFlight) return 'partial';
  if (input.hasError || input.state === 'failed') return 'failed';
  switch (input.cancelDisposition) {
    case 'notStarted':
      return 'failed';
    case 'settledRolledBack':
      return 'failed';
    case 'settledCompleted':
      // Completed on its own merits; `completed` above already returned 'ok'.
      return 'ok';
    case 'settledPartially':
      return 'partial';
    case 'settledUnknown':
      return 'uncertain';
    case 'requestedInFlight':
      return 'partial';
    case 'none':
    default:
      return input.state === 'succeeded' ? 'partial' : 'partial';
  }
}

/** The 8 MiB pipeline budget is a deliberate fail-closed interception. */
export const PIPELINE_BUDGET_BYTES = 8 * 1024 * 1024;

/** `true` for the backend's `PipelineBudget` interception message. */
export function isPipelineBudgetInterception(message: string | null | undefined): boolean {
  if (!message) return false;
  return /pipeline\s*budget|pipelinebudget|budget\s*exceed|8\s*MiB|8388608/i.test(message);
}

/**
 * planId single-consumption. A second apply of the same plan is refused by
 * the backend; the UI must surface that refusal instead of offering a retry
 * that looks safe but is not.
 */
export function isPlanConsumedRejection(message: string | null | undefined): boolean {
  if (!message) return false;
  return /already\s*consumed|PlanAlreadyConsumed|re-review the migration to mint a new plan/i.test(
    message,
  );
}

/**
 * backendScope fail-closed. When the endpoints cannot prove they are the
 * local desktop backend, the backend refuses with a `Validation` error; the UI
 * has to say so rather than degrade into a soft failure.
 */
export function isBackendScopeRejection(message: string | null | undefined): boolean {
  if (!message) return false;
  return /backend\s*scope|local-desktop-backend|scope/i.test(message);
}
