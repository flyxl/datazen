/**
 * Data Transfer — the Job execution state machine.
 *
 * The window used to own a single `execute` call and a `cancelled` boolean.
 * That shape cannot say *where* a run stopped, so this hook owns the whole
 * prepare → apply → verdict lifecycle and exposes exactly two truths:
 *
 * - `cancelRequested` — *intent*. Latched the instant the user clicks.
 * - `verdict` — what actually happened, folded from the Job's
 *   `effectOutcome` + `commitBoundaries` by the shared engine in
 *   `src/lib/migrationJobVerdict.ts`. Schema Diff and Data Sync reuse that
 *   engine unchanged; only their command layer differs.
 *
 * Four invariants this hook exists to protect:
 *
 * - A `planId` is spent once. A rejected second apply is surfaced as
 *   `planConsumed` with a re-prepare action, never as a retry button that
 *   would look safe but is not.
 * - A cancel click is not a cancellation. `cancelDisposition` stays
 *   `requestedInFlight` until the backend returns a verdict.
 * - A cancel is addressed to a Job id we actually know. Guessing the apply
 *   id from the prepare id would report a cancellation that never happened.
 * - Prepare and apply never share an `idempotencyKey`. The backend receipt map
 *   is keyed by it, so one shared key would make the apply *replay the prepare
 *   Job* instead of writing anything.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import type { JobDetails, JobView } from '@datazen/backend-client';
import {
  localBackendScope,
  transferJobCommands,
  type TransferApplyJobRequest,
  type TransferApplyJobView,
  type TransferPrepareJobView,
} from '../commands/transferJobs';
import type { TransferJob } from '../commands/transfer';
import {
  deriveMigrationJobVerdict,
  isBackendScopeRejection,
  isPipelineBudgetInterception,
  isPlanConsumedRejection,
  isMigrationJobInFlight,
  readJobCounter,
  type MigrationJobVerdict,
} from '../lib/migrationJobVerdict';
import { isStalePlanError } from '../lib/migrationJobHydration';

export type TransferRunPhase =
  | 'idle'
  | 'preparing'
  | 'prepared'
  /** Backend-scope / prepare-time refusal: the plan was not admitted for execution. */
  | 'blocked'
  | 'applying'
  /** Terminal verdict available — render it, do not offer a blind retry. */
  | 'settled';

/**
 * How a command refusal must be phrased. Each arm is fail-closed on purpose:
 * backend-scope and pipeline-budget are *refusals*, not warnings, and degrading
 * them into a soft failure would hide a rule the backend enforced.
 */
export type TransferRunFailureKind =
  /** Backend scope: endpoints could not prove they are the local desktop backend. */
  | 'backendScope'
  /** The 8 MiB pipeline budget refused the write on purpose. */
  | 'pipelineBudget'
  /** This planId was already spent by an earlier apply Job. */
  | 'planConsumed'
  /** Plan expired / source drifted / permission changed — re-prepare. */
  | 'stalePlan'
  | 'other';

export interface TransferRunFailure {
  kind: TransferRunFailureKind;
  message: string;
}

export interface TransferApplyInput {
  /** Empty / absent for a SQL-file run with no explicit table list. */
  sourceTables?: string[];
  confirmedDestructive: boolean;
  /**
   * Stable across a retry of *this* apply so the backend replays the
   * recorded receipt instead of committing a second time. Never share it with
   * the prepare call. Omit to let the backend mint a fresh key.
   */
  idempotencyKey?: string;
}

export interface TransferJobRun {
  phase: TransferRunPhase;
  prepareView: TransferPrepareJobView | null;
  applyView: TransferApplyJobView | null;
  failure: TransferRunFailure | null;
  /** The click happened. Never rendered as "cancelled". */
  cancelRequested: boolean;
  /** The backend recognised the Job id and recorded the intent. */
  cancelAcknowledged: boolean;
  /**
   * The cancel could not be delivered — either the backend has no such Job, or
   * no cancellable id existed yet. Never folded into a successful cancellation.
   */
  cancelUnknownJob: boolean;
  /** Non-null only once a verdict can be stated. */
  verdict: MigrationJobVerdict | null;
  /** The addressable active apply Job id, or the prepare Job id before apply. */
  cancelTargetJobId: string | null;
  /** True while the apply Job is queued/running — disables "apply" but not "cancel". */
  isInFlight: boolean;
  prepare: (job: TransferJob, options?: { idempotencyKey?: string }) => Promise<TransferPrepareJobView | null>;
  apply: (input: TransferApplyInput) => Promise<TransferApplyJobView | null>;
  /**
   * `true` only when a cancel was actually sent *and* the backend answered
   * `true`. `false` means "nothing was cancelled" — either no id was available
   * or the backend has no such Job — and the caller must not treat it as one.
   */
  requestCancel: () => Promise<boolean>;
  /** Return to `idle` for a fresh run; keeps nothing a recovery needs. */
  reset: () => void;
  /** Re-review after a consumed plan. Mints a new plan, never re-applies. */
  reprepare: () => void;
  /**
   * The most recent classified failure, readable synchronously. `prepare` and
   * `apply` resolve to `null` instead of throwing, so a caller that needs the
   * backend's own words in the same tick (the wizard's error panel) reads them
   * here rather than from state that has not been committed yet.
   */
  lastFailure: () => TransferRunFailure | null;
}

/** Classify a refused command so the UI can phrase it correctly. */
export function classifyTransferRunFailure(error: unknown): TransferRunFailure {
  const message = error instanceof Error ? error.message : String(error);
  if (isBackendScopeRejection(message)) return { kind: 'backendScope', message };
  if (isPipelineBudgetInterception(message)) return { kind: 'pipelineBudget', message };
  if (isPlanConsumedRejection(message)) return { kind: 'planConsumed', message };
  if (isStalePlanError(error)) return { kind: 'stalePlan', message };
  return { kind: 'other', message };
}

function isAmbiguousApplySubmissionError(error: unknown): boolean {
  if (typeof error === 'object' && error !== null && 'code' in error) {
    const code = (error as { code?: unknown }).code;
    if (code === 'OutcomeUnknown' || code === 'ServiceUnavailable' || code === 'Timeout') {
      return true;
    }
  }
  const message = error instanceof Error ? error.message : String(error);
  return /timed?\s*out|timeout|transport|connection\s+(?:closed|lost)|ack(?:nowledg(?:e)?ment)?\s+lost|outcome\s+unknown/i.test(
    message,
  );
}

function isActiveJobState(state: string): boolean {
  return state === 'queued' || state === 'running';
}

/** Merge the generic Job read model into the transfer admission receipt. */
function projectApplyJobView(current: TransferApplyJobView, latest: JobView): TransferApplyJobView {
  const active = isActiveJobState(latest.state);
  return {
    ...current,
    state: latest.state,
    // A terminal summary is not the durable report. Keep it unknown until the
    // details query returns boundaries and the exact recovery disposition.
    effectOutcome: active ? latest.effectOutcome ?? current.effectOutcome : 'unknown',
    progress: latest.progress,
    artifactIds: [...latest.artifactIds],
    cancelled: latest.state === 'cancelled',
    partial:
      latest.effectOutcome === 'partiallyApplied' || latest.effectOutcome === 'unknown',
  };
}

/** Merge the durable terminal read model into the live apply receipt. */
function projectApplyJobDetails(
  current: TransferApplyJobView,
  details: JobDetails,
): TransferApplyJobView {
  const { job } = details;
  return {
    ...current,
    jobId: job.jobId,
    kind: job.kind,
    state: job.state,
    effectOutcome: job.effectOutcome ?? 'unknown',
    progress: job.progress,
    planId: details.planId ?? current.planId,
    planDigest: details.planDigest ?? current.planDigest,
    selectionRevision: details.selectionRevision ?? current.selectionRevision,
    commitBoundaries: [...details.commitBoundaries],
    artifactIds: [...job.artifactIds],
    cancelled: job.state === 'cancelled',
    partial: job.effectOutcome === 'partiallyApplied' || job.effectOutcome === 'unknown',
    error: job.error ?? null,
    recoveryVerdict: details.recovery?.verdict ?? null,
    recoveryResumeThrough: details.recovery?.resumeThrough ?? null,
    recoveryReason: details.recovery?.reasonCode ?? null,
  };
}

export function useTransferJobRun(): TransferJobRun {
  const [phase, setPhase] = useState<TransferRunPhase>('idle');
  const [prepareView, setPrepareView] = useState<TransferPrepareJobView | null>(null);
  const [applyView, setApplyView] = useState<TransferApplyJobView | null>(null);
  const [failure, setFailure] = useState<TransferRunFailure | null>(null);
  const [cancelRequested, setCancelRequested] = useState(false);
  const [cancelAcknowledged, setCancelAcknowledged] = useState(false);
  const [cancelUnknownJob, setCancelUnknownJob] = useState(false);
  const [watchJobId, setWatchJobId] = useState<string | null>(null);
  const [terminalDetailsJobId, setTerminalDetailsJobId] = useState<string | null>(null);
  /**
   * Refs, not state: `requestCancel` must not re-render the whole wizard just
   * to read ids the previous calls already returned.
   *
   * They are kept apart on purpose. Cancelling the *prepare* Job while a plan
   * is merely frozen is a real cancellation; sending that same id at an
   * in-flight *apply* would make `cancel_data_transfer` return `true` for a Job
   * nobody asked about, and the UI would then claim a cancellation that never
   * touched the write.
   */
  const prepareJobIdRef = useRef<string | null>(null);
  const applyJobIdRef = useRef<string | null>(null);
  /**
   * The classified failure is mirrored into a ref so a caller that swallowed the
   * rejection (`prepare` resolves to `null`, it does not throw) can still read
   * the backend's own words in the same tick. React state would not have been
   * committed yet, so the detail would be lost from the wizard's error panel.
   */
  const failureRef = useRef<TransferRunFailure | null>(null);
  const lastFailure = useCallback((): TransferRunFailure | null => failureRef.current, []);
  const fail = useCallback((next: TransferRunFailure | null) => {
    failureRef.current = next;
    setFailure(next);
  }, []);

  const prepare = useCallback(
    async (job: TransferJob, options?: { idempotencyKey?: string }) => {
      setPhase('preparing');
      fail(null);
      setApplyView(null);
      setCancelRequested(false);
      setCancelAcknowledged(false);
      setCancelUnknownJob(false);
      try {
        const view = await transferJobCommands.prepare({
          job,
          backendScope: localBackendScope(),
          // A stable prepare key makes "preview again" replay the same admitted
          // plan instead of freezing a second one.
          ...(options?.idempotencyKey ? { idempotencyKey: options.idempotencyKey } : {}),
        });
        prepareJobIdRef.current = view.jobId;
        setPrepareView(view);
        setPhase(view.canExecute ? 'prepared' : 'blocked');
        return view;
      } catch (error) {
        prepareJobIdRef.current = null;
        fail(classifyTransferRunFailure(error));
        setPhase('blocked');
        return null;
      }
    },
    [],
  );

  const apply = useCallback(
    async (input: TransferApplyInput) => {
      if (!prepareView) {
        fail({ kind: 'other', message: 'apply requires an admitted plan' });
        return null;
      }
      // The planId was already spent. The backend would refuse this, but a
      // local refusal keeps the window from offering an action that cannot
      // succeed, and keeps `planConsumed` a first-class outcome.
      if (phase === 'settled' && applyView && !applyView.replayed) {
        fail({
          kind: 'planConsumed',
          message: 'this planId was already applied; review a new plan',
        });
        return null;
      }
      setPhase('applying');
      fail(null);
      const request: TransferApplyJobRequest = {
        planId: prepareView.planId,
        planDigest: prepareView.planDigest,
        selectionRevision: prepareView.selectionRevision,
        // An absent list means "every table the plan frozen", which is
        // what a SQL-file run with no explicit selection means.
        selection: { sourceTables: input.sourceTables ?? null },
        confirmedDestructive: input.confirmedDestructive,
        backendScope: localBackendScope(),
        // A plan is spent once, so its id is a stable and naturally scoped
        // receipt key even when a caller did not supply an explicit one.
        idempotencyKey: input.idempotencyKey ?? `data-transfer/apply/${prepareView.planId}`,
      };
      let view: TransferApplyJobView;
      try {
        view = await transferJobCommands.apply(request);
      } catch (error) {
        // The apply may have been accepted even though its receipt was lost.
        // Repeat the exact request once under the same idempotency key: the
        // backend replays the original Job view and cannot create a second
        // write. A new key here would turn an ambiguous response into a second
        // execution attempt.
        if (!isAmbiguousApplySubmissionError(error)) {
          fail(classifyTransferRunFailure(error));
          setPhase('blocked');
          return null;
        }
        try {
          view = await transferJobCommands.apply(request);
        } catch (recoveryError) {
          fail(classifyTransferRunFailure(recoveryError));
          setPhase('blocked');
          return null;
        }
      }

      // `apply_data_transfer_job` answers at admission. Publish the returned id
      // before the next render so cancellation and polling address this Job
      // while it is still queued or running.
      applyJobIdRef.current = view.jobId;
      const active = isActiveJobState(view.state);
      setApplyView(
        active
          ? view
          : {
              ...view,
              effectOutcome: 'unknown',
              recoveryVerdict: 'pendingVerification',
              recoveryReason: 'jobDetailsPending',
            },
      );
      setPhase(active ? 'applying' : 'settled');
      setWatchJobId(active ? view.jobId : null);
      setTerminalDetailsJobId(active ? null : view.jobId);
      return view;
    },
    [prepareView, phase, applyView],
  );

  useEffect(() => {
    if (!terminalDetailsJobId) return undefined;
    let stopped = false;
    const jobId = terminalDetailsJobId;

    void transferJobCommands
      .getDetails(jobId)
      .then((details) => {
        if (stopped || applyJobIdRef.current !== jobId) return;
        if (details.job.jobId !== jobId || details.job.kind !== 'dataTransferApply') {
          throw new Error('jobDetailsIdentityMismatch');
        }
        setApplyView((current) =>
          current?.jobId === jobId ? projectApplyJobDetails(current, details) : current,
        );
        if (details.job.cancelRequested) {
          setCancelRequested(true);
          setCancelAcknowledged(true);
          setCancelUnknownJob(false);
        }
        if (isActiveJobState(details.job.state)) {
          setPhase('applying');
          setWatchJobId(jobId);
        }
        setTerminalDetailsJobId(null);
      })
      .catch(() => {
        if (stopped || applyJobIdRef.current !== jobId) return;
        setApplyView((current) =>
          current?.jobId === jobId
            ? {
                ...current,
                effectOutcome: 'unknown',
                recoveryVerdict: 'pendingVerification',
                recoveryReason: 'jobDetailsUnavailable',
              }
            : current,
        );
        setTerminalDetailsJobId(null);
      });

    return () => {
      stopped = true;
    };
  }, [terminalDetailsJobId]);

  useEffect(() => {
    if (!watchJobId) return undefined;
    let stopped = false;
    let timer: ReturnType<typeof setTimeout> | undefined;

    const poll = async (): Promise<void> => {
      try {
        const latest = await transferJobCommands.getJob(watchJobId);
        if (stopped || applyJobIdRef.current !== watchJobId) return;
        setApplyView((current) =>
          current?.jobId === watchJobId ? projectApplyJobView(current, latest) : current,
        );
        if (latest.cancelRequested) {
          setCancelRequested(true);
          setCancelAcknowledged(true);
          setCancelUnknownJob(false);
        }
        if (!isActiveJobState(latest.state)) {
          let terminalState = latest.state;
          try {
            const details = await transferJobCommands.getDetails(watchJobId);
            if (stopped || applyJobIdRef.current !== watchJobId) return;
            if (
              details.job.jobId !== watchJobId ||
              details.job.kind !== 'dataTransferApply'
            ) {
              throw new Error('jobDetailsIdentityMismatch');
            }
            terminalState = details.job.state;
            setApplyView((current) =>
              current?.jobId === watchJobId ? projectApplyJobDetails(current, details) : current,
            );
            if (details.job.cancelRequested) {
              setCancelRequested(true);
              setCancelAcknowledged(true);
              setCancelUnknownJob(false);
            }
          } catch {
            if (stopped || applyJobIdRef.current !== watchJobId) return;
            // A missing durable report must stay uncertain; the lightweight
            // summary cannot prove terminal boundaries or a recovery verdict.
            setApplyView((current) =>
              current?.jobId === watchJobId
                ? {
                    ...current,
                    effectOutcome: 'unknown',
                    recoveryVerdict: 'pendingVerification',
                    recoveryReason: 'jobDetailsUnavailable',
                  }
                : current,
            );
          }
          if (!isActiveJobState(terminalState)) {
            setPhase('settled');
            setWatchJobId(null);
            return;
          }
        }
      } catch {
        // A failed poll is a temporary loss of observation, not a job failure.
        // Retry against the same id; never resubmit the apply to recover status.
      }
      if (!stopped) timer = setTimeout(() => void poll(), 1500);
    };

    void poll();
    return () => {
      stopped = true;
      if (timer !== undefined) clearTimeout(timer);
    };
  }, [watchJobId]);

  /**
   * Sends `cancel_data_transfer` when — and only when — the frontend holds the
   * Job id the request can be addressed to.
   *
   * @returns `true` only when a cancel was actually sent *and* the backend
   * answered `true` (it knows the Job). `false` means nothing was cancelled:
   * either there was no id to address, or the backend has no such Job. A caller
   * must not treat `false` as "cancelled".
   */
  const requestCancel = useCallback(async (): Promise<boolean> => {
    setCancelRequested(true);
    // Apply admission returns its Job id before the worker settles. Use that id
    // while active; if admission has not returned yet, there is no apply id to
    // guess and cancelling the prepare Job would not stop the write.
    const applyJobId = applyJobIdRef.current;
    const jobId = applyJobId
      ? applyView && isActiveJobState(applyView.state)
        ? applyJobId
        : null
      : phase === 'applying'
        ? null
        : prepareJobIdRef.current;
    if (!jobId) {
      setCancelAcknowledged(false);
      setCancelUnknownJob(true);
      return false;
    }
    try {
      const acknowledged = await transferJobCommands.cancel(jobId);
      setCancelAcknowledged(acknowledged);
      setCancelUnknownJob(!acknowledged);
      return acknowledged;
    } catch (error) {
      setCancelUnknownJob(true);
      fail(classifyTransferRunFailure(error));
      return false;
    }
  }, [phase, applyView]);

  const reset = useCallback(() => {
    setPhase('idle');
    setPrepareView(null);
    setApplyView(null);
    fail(null);
    setCancelRequested(false);
    setCancelAcknowledged(false);
    setCancelUnknownJob(false);
    prepareJobIdRef.current = null;
    applyJobIdRef.current = null;
    setWatchJobId(null);
    setTerminalDetailsJobId(null);
  }, []);

  const verdict = useMemo<MigrationJobVerdict | null>(() => {
    if (!applyView) return null;
    return deriveMigrationJobVerdict({
      state: applyView.state,
      effectOutcome: applyView.effectOutcome,
      cancelRequested,
      commitBoundaries: applyView.commitBoundaries,
      recoveryVerdict: applyView.recoveryVerdict,
      recoveryReason: applyView.recoveryReason,
      error: applyView.error,
      // A commit is only trustworthy when a boundary backs it. The
      // data-transfer handler records no boundaries, so these two counters are
      // the only way the verdict can tell "wrote nothing" from "wrote
      // something nobody recorded" — without them every such run would render
      // as a clean success over an unverifiable write.
      committedRows: readJobCounter(applyView.progress?.committed),
      unknownRows: readJobCounter(applyView.progress?.unknown),
    });
  }, [applyView, cancelRequested]);

  const cancelTargetJobId = applyJobIdRef.current
    ? applyView && isActiveJobState(applyView.state)
      ? applyJobIdRef.current
      : null
    : phase === 'applying'
      ? null
      : prepareView?.jobId ?? null;
  const isInFlight = applyView ? isMigrationJobInFlight(applyView.state) : phase === 'applying';

  return {
    phase,
    prepareView,
    applyView,
    failure,
    cancelRequested,
    cancelAcknowledged,
    cancelUnknownJob,
    verdict,
    cancelTargetJobId,
    isInFlight,
    prepare,
    apply,
    requestCancel,
    reset,
    reprepare: reset,
    lastFailure,
  };
}
