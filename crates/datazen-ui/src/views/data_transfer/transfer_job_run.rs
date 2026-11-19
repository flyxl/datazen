//! T147-S2: the Data Transfer Job run machine.
//!
//! Ported from `packages/frontend/src/hooks/useTransferJobRun.ts` (564
//! lines) — the prepare → apply → verdict lifecycle the window mounts via
//! `jobRun`. The Vm owns the state machine; every async hop is the host's
//! (the Vm never performs IPC), and each host outcome lands through a
//! dedicated method so the semantics cannot drift into two places.
//!
//! ## The four invariants the hook exists to protect (React 7-26)
//!
//! * A `planId` is spent once. A rejected second apply surfaces as
//!   `planConsumed` with a re-prepare action ([`apply_local_refusal`]),
//!   never as a retry button that would look safe but is not
//!   ([`TransferJobRunVm::begin_apply`]'s guard).
//! * A cancel click is not a cancellation. `cancel_requested` latches the
//!   intent the instant the user clicks and stays until the backend
//!   returns a verdict ([`begin_cancel`], [`finish_cancel`]).
//! * A cancel is addressed to a Job id we actually know
//!   ([`cancel_target_job_id`]) — guessing the apply id from the prepare
//!   id would report a cancellation that never happened.
//! * Prepare and apply never share an `idempotencyKey`. The apply key is
//!   derived from the admitted `planId` (React 296-299) when the caller
//!   supplies none, so a retry replays the recorded receipt instead of
//!   committing a second time.
//!
//! ## Host contract
//!
//! * **Prepare.** The host calls the window's `runPreview`, which reads
//!   [`begin_prepare`] (sets `preparing`, clears the failure and the
//! previous apply), performs `transferJobCommands.prepare`, and applies
//! the outcome with [`finish_prepare`] (a `canExecute` view → `prepared`;
//! otherwise `blocked`) or [`fail_prepare`] (classified, → `blocked`).
//! * **Apply.** The host calls [`begin_apply`] (the local guards: no
//!   admitted plan; the plan already spent), builds the request with
//!   [`apply_request`], performs the command, and applies the outcome:
//!   an accepted active view with [`finish_apply_active`], an accepted
//!   terminal view with [`finish_apply_terminal`], a ambiguous-reply
//!   retry with [`begin_apply_retry`] (the exact request under the same
//!   key), or a refusal with [`fail_apply`] (classified, → `blocked`).
//! * **Terminal details.** The host queries the durable report for a
//!   settled apply and applies it with [`apply_terminal_details`]; an
//!   identity mismatch or a missing report degrades through
//!   [`apply_details_unavailable`].
//! * **Polling.** The host polls the lightweight Job summary while the
//!   apply is active and applies each with [`apply_job_view`] — a poll
//!   failure is a temporary loss of observation, not a job failure, and
//!   the host retries against the same id.
//! * **Cancel.** [`begin_cancel`] decides whether a cancellable id exists
//!   (the apply id while it is active; the prepare id before apply;
//!   nothing during admission), the host performs
//!   `cancel_data_transfer`, [`finish_cancel`] records the answer —
//!   `true` only when the cancel was actually sent *and* the backend
//!   answered `true`.
//! * **Reset.** [`reset`] / [`reprepare`] (the same method, as in React
//!   561-562) returns to `idle` keeping nothing a recovery needs.
//!
//! ## Documented deviations from React
//!
//! * **The refs are fields.** React splits state (re-renders) from refs
//!   (`prepareJobIdRef` / `applyJobIdRef` / `failureRef`, read inside
//!   stable callbacks); the Vm is the single owner, and the mirror
//!   `failure_slot` keeps the same-tick readable copy React's
//!   `lastFailure()` needs (React 232-237).
//! * **`effectOutcome` stays the raw wire string** (`Option<String>`),
//!   exactly as the React consumers match on the literal spellings
//!   (`'completed'` / `'rolledBack'` / …) — including the `'unknown'`
//!   downgrade the hooks apply themselves.
//! * **`recoveryVerdict` / `recoveryReason` are strings**, same reason.
//! * **`watch_job_id` / `terminal_details_job_id`** are the effect
//!   identities: the host starts and stops its polling/details queries
//!   from them (React 346-462's effects key off them), and
//!   [`apply_job_view`] / [`apply_terminal_details`] guard the responses
//!   against the *current* apply id, as React's `applyJobIdRef.current
//!   !== jobId` guards do.
//! * **Timestamps** (`createdAt` / `updatedAt`) ride the views because
//!   React's job-center hydration keys its effect identity on them.

use crate::views::data_transfer::data_transfer_attached_jobs::{
    read_job_counter, JobProgress, JobState,
};
use crate::views::data_transfer::data_transfer_terminal_results::{
    derive_migration_job_verdict, is_migration_job_in_flight, MigrationJobVerdictInput,
};
use crate::views::data_transfer::data_transfer_window_state::TransferPreviewVm;

use crate::components::migration::migration_job_verdict_panel::{
    CommitBoundary, MigrationJobVerdict,
};

// ---------------------------------------------------------------------------
// The failure classification (React 64-141).
// ---------------------------------------------------------------------------

/// React `TransferRunPhase` (`useTransferJobRun.ts:49-57`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum TransferRunPhase {
    #[default]
    Idle,
    Preparing,
    Prepared,
    /// Backend-scope / prepare-time refusal: the plan was not admitted for
    /// execution.
    Blocked,
    Applying,
    /// Terminal verdict available — render it, do not offer a blind retry.
    Settled,
}

/// React `TransferRunFailureKind` (`useTransferJobRun.ts:64-73`): how a
/// command refusal must be phrased. Each arm is fail-closed on purpose.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransferRunFailureKind {
    /// Backend scope: endpoints could not prove they are the local desktop
    /// backend.
    BackendScope,
    /// The 8 MiB pipeline budget refused the write on purpose.
    PipelineBudget,
    /// This planId was already spent by an earlier apply Job.
    PlanConsumed,
    /// Plan expired / source drifted / permission changed — re-prepare.
    StalePlan,
    Other,
}

/// React `TransferRunFailure` (`useTransferJobRun.ts:75-78`).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferRunFailure {
    pub kind: TransferRunFailureKind,
    pub message: String,
}

/// React `isBackendScopeRejection` (`lib/migrationJobVerdict.ts:355-358`):
/// the backend refuses with a `Validation` error mentioning the scope.
fn is_backend_scope_rejection(message: &str) -> bool {
    // /backend\s*scope|local-desktop-backend|scope/i
    let lower = message.to_ascii_lowercase();
    lower.contains("backend scope") || lower.contains("local-desktop-backend") || lower.contains("scope")
}

/// React `isPipelineBudgetInterception` (`lib/migrationJobVerdict.ts:333-336`).
fn is_pipeline_budget_interception(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("pipeline budget")
        || lower.contains("pipelinebudget")
        || lower.contains("budget exceed")
        || lower.contains("8 mib")
        || lower.contains("8388608")
}

/// React `isPlanConsumedRejection` (`lib/migrationJobVerdict.ts:343-348`).
fn is_plan_consumed_rejection(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    lower.contains("already consumed")
        || lower.contains("planalreadyconsumed")
        || lower.contains("re-review the migration to mint a new plan")
}

/// React `STALE_PLAN_ERROR_CODES` (`lib/migrationJobHydration.ts:114`).
const STALE_PLAN_ERROR_CODES: [&str; 3] = ["PlanStale", "SourceChanged", "PermissionDenied"];

/// React `isStalePlanError` (`lib/migrationJobHydration.ts:125-149`), the
/// message-mention arm the desktop invoke path exercises: a failure whose
/// message mentions one of the stale-plan codes is a stale plan.
fn is_stale_plan_error(message: &str) -> bool {
    STALE_PLAN_ERROR_CODES.iter().any(|code| message.contains(code))
}

/// React `classifyTransferRunFailure` (`useTransferJobRun.ts:134-141`):
/// classify a refused command so the UI can phrase it correctly.
pub(crate) fn classify_transfer_run_failure(message: &str) -> TransferRunFailure {
    if is_backend_scope_rejection(message) {
        return TransferRunFailure {
            kind: TransferRunFailureKind::BackendScope,
            message: message.to_owned(),
        };
    }
    if is_pipeline_budget_interception(message) {
        return TransferRunFailure {
            kind: TransferRunFailureKind::PipelineBudget,
            message: message.to_owned(),
        };
    }
    if is_plan_consumed_rejection(message) {
        return TransferRunFailure {
            kind: TransferRunFailureKind::PlanConsumed,
            message: message.to_owned(),
        };
    }
    if is_stale_plan_error(message) {
        return TransferRunFailure {
            kind: TransferRunFailureKind::StalePlan,
            message: message.to_owned(),
        };
    }
    TransferRunFailure {
        kind: TransferRunFailureKind::Other,
        message: message.to_owned(),
    }
}

// ---------------------------------------------------------------------------
// The wire mirrors (`commands/transferJobs.ts:84-125`).
// ---------------------------------------------------------------------------

/// React `TransferPrepareJobView` (`commands/transferJobs.ts:84-99`) — the
/// admission receipt the prepare Job returns.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferPrepareJobViewVm {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub effect_outcome: Option<String>,
    pub plan_id: String,
    pub plan_digest: String,
    #[allow(dead_code)] // the wire carries it; the window reads planId/digest/revision/canExecute
    pub plan_version: i64,
    #[allow(dead_code)] // same
    pub handler_version: i64,
    #[allow(dead_code)] // same
    pub checkpoint_version: i64,
    pub selection_revision: i64,
    #[allow(dead_code)] // the review surface reads the deadline hint; the window does not
    pub expires_at: String,
    pub can_execute: bool,
    pub block_reason: Option<String>,
    /// React `review: TransferPreview` — the same review the old
    /// `preview_data_transfer` returned, so every downstream consumer of
    /// the preview is unchanged (React 738-743).
    pub review: TransferPreviewVm,
}

/// React `TransferApplyJobView` (`commands/transferJobs.ts:101-125`) — the
/// acceptance receipt the apply Job returns, progressively merged by the
/// poll / details projections.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferApplyJobViewVm {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub effect_outcome: Option<String>,
    pub progress: JobProgress,
    pub plan_id: String,
    pub plan_digest: String,
    pub selection_revision: i64,
    /// Durable write markers; the only trustworthy record of what landed.
    pub commit_boundaries: Vec<CommitBoundary>,
    /// SQL-file runs return `transfer-sql-<sha256hex>` here.
    pub artifact_ids: Vec<String>,
    /// Cancel intent. Never equivalent to "cancelled".
    pub cancelled: bool,
    pub partial: bool,
    /// True when this reply came from the idempotent receipt, not a fresh
    /// write.
    pub replayed: bool,
    pub error: Option<String>,
    pub recovery_verdict: Option<String>,
    pub recovery_resume_through: Option<i64>,
    pub recovery_reason: Option<String>,
}

// ---------------------------------------------------------------------------
// The request the host sends (React 287-299).
// ---------------------------------------------------------------------------

/// React `TransferApplyJobRequest` (`commands/transferJobs.ts:74-82`) as
/// the Vm assembles it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TransferApplyRequestVm {
    pub plan_id: String,
    pub plan_digest: String,
    pub selection_revision: i64,
    /// `None` means "every table the plan froze" (React 291-293).
    pub source_tables: Option<Vec<String>>,
    pub confirmed_destructive: bool,
    /// The backend receipt key; see the invariant above.
    pub idempotency_key: String,
}

/// React `TransferApplyInput` (`useTransferJobRun.ts:80-90`).
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TransferApplyInput {
    /// Empty / absent for a SQL-file run with no explicit table list.
    pub source_tables: Option<Vec<String>>,
    pub confirmed_destructive: bool,
    /// Stable across a retry of *this* apply so the backend replays the
    /// recorded receipt instead of committing a second time. Never share
    /// it with the prepare call. Omit to let the plan id scope it.
    pub idempotency_key: Option<String>,
}

/// React `isAmbiguousApplySubmissionError`
/// (`useTransferJobRun.ts:143-154`): an apply may have been accepted even
/// though its receipt was lost (OutcomeUnknown / ServiceUnavailable /
/// Timeout codes, or a transport-flavoured message).
pub(crate) fn is_ambiguous_apply_submission_error(message: &str) -> bool {
    let lower = message.to_ascii_lowercase();
    // React matches /timed?\s*out|timeout|transport|connection
    // (closed|lost)|ack...|outcome unknown/i — the word boundaries are
    // approximated by the contains checks below on the lowered text.
    lower.contains("timed out")
        || lower.contains("timeout")
        || lower.contains("transport")
        || lower.contains("connection closed")
        || lower.contains("connection lost")
        || lower.contains("acknowledgement lost")
        || lower.contains("acknowledgment lost")
        || lower.contains("ack lost")
        || lower.contains("outcome unknown")
}

/// React `isActiveJobState` (`useTransferJobRun.ts:156-158`).
fn is_active_job_state(state: JobState) -> bool {
    matches!(state, JobState::Queued | JobState::Running)
}

// ---------------------------------------------------------------------------
// The run machine.
// ---------------------------------------------------------------------------

/// The Job run state — every `useState` slot the hook holds (React
/// 205-213) plus the refs it reads inside stable callbacks (React
/// 224-237).
#[derive(Clone, Debug, Default)]
pub(crate) struct TransferJobRunVm {
    pub phase: TransferRunPhase,
    pub prepare_view: Option<TransferPrepareJobViewVm>,
    pub apply_view: Option<TransferApplyJobViewVm>,
    pub failure: Option<TransferRunFailure>,
    /// The click happened. Never rendered as "cancelled" (React 98).
    pub cancel_requested: bool,
    /// The backend recognised the Job id and recorded the intent (React
    /// 100).
    pub cancel_acknowledged: bool,
    /// The cancel could not be delivered — either the backend has no such
    /// Job, or no cancellable id existed yet (React 102-105).
    pub cancel_unknown_job: bool,
    /// React `prepareJobIdRef` (React 224): the prepare Job's id.
    prepare_job_id: Option<String>,
    /// React `applyJobIdRef` (React 225): the apply Job's id.
    apply_job_id: Option<String>,
    /// React `failureRef` (React 232): the same-tick readable failure
    /// mirror, so a caller that swallowed the rejection can still read
    /// the backend's own words before the state is committed anywhere.
    failure_slot: Option<TransferRunFailure>,
    /// React `watchJobId` (React 212): the polling effect's identity.
    pub watch_job_id: Option<String>,
    /// React `terminalDetailsJobId` (React 213): the details query's
    /// identity.
    pub terminal_details_job_id: Option<String>,
}

impl TransferJobRunVm {
    // -- the readable mirrors (React 92-131) ------------------------------

    /// The verdict, folded from the apply view by the shared engine
    /// (React 517-535). `None` only once a verdict can be stated.
    pub(crate) fn verdict(&self) -> Option<MigrationJobVerdict> {
        let apply = self.apply_view.as_ref()?;
        let input = MigrationJobVerdictInput {
            state: apply.state,
            effect_outcome: apply.effect_outcome.as_deref(),
            cancel_requested: self.cancel_requested,
            commit_boundaries: Some(&apply.commit_boundaries),
            recovery_verdict: apply.recovery_verdict.as_deref(),
            recovery_reason: apply.recovery_reason.as_deref(),
            error: apply.error.as_deref(),
            // A commit is only trustworthy when a boundary backs it. The
            // data-transfer handler records no boundaries, so these two
            // counters are the only way the verdict can tell "wrote
            // nothing" from "wrote something nobody recorded" (React
            // 526-534).
            committed_rows: read_job_counter(apply.progress.committed.as_ref()),
            unknown_rows: read_job_counter(apply.progress.unknown.as_ref()),
        };
        Some(derive_migration_job_verdict(&input))
    }

    /// The addressable active apply Job id, or the prepare Job id before
    /// apply (React 537-543). `None` during admission (no apply id exists
    /// yet to guess) and once the apply has left its active states.
    pub(crate) fn cancel_target_job_id(&self) -> Option<&str> {
        if self.apply_job_id.is_some() {
            let active = self
                .apply_view
                .as_ref()
                .map(|view| is_active_job_state(view.state))
                .unwrap_or(false);
            if active {
                self.apply_job_id.as_deref()
            } else {
                None
            }
        } else if self.phase == TransferRunPhase::Applying {
            // Guessing the apply id from the prepare id would report a
            // cancellation that never happened (React 483-485).
            None
        } else {
            self.prepare_view.as_ref().map(|view| view.job_id.as_str())
        }
    }

    /// True while the apply Job is queued/running — disables "apply" but
    /// not "cancel" (React 544).
    pub(crate) fn is_in_flight(&self) -> bool {
        match &self.apply_view {
            Some(view) => is_migration_job_in_flight(&view.state),
            None => self.phase == TransferRunPhase::Applying,
        }
    }

    /// React `lastFailure` (React 233): the same-tick readable failure.
    pub(crate) fn last_failure(&self) -> Option<&TransferRunFailure> {
        self.failure_slot.as_ref()
    }

    // -- prepare (React 239-267) --------------------------------------------

    /// The prepare's opening bracket (React 241-246): the phase turns
    /// `preparing`, the failure clears, and the previous apply's residue
    /// (an old receipt, an old cancel intent) goes with it.
    pub(crate) fn begin_prepare(&mut self) {
        self.phase = TransferRunPhase::Preparing;
        self.fail(None);
        self.apply_view = None;
        self.cancel_requested = false;
        self.cancel_acknowledged = false;
        self.cancel_unknown_job = false;
        self.apply_job_id = None;
    }

    /// The prepare's success (React 248-258): the admission receipt is
    /// published, the phase follows `canExecute`, and the prepare Job id
    /// becomes the cancel address before apply exists.
    pub(crate) fn finish_prepare(&mut self, view: TransferPrepareJobViewVm) {
        self.prepare_job_id = Some(view.job_id.clone());
        self.prepare_view = Some(view);
        self.phase = if self.prepare_view.as_ref().is_some_and(|v| v.can_execute) {
            TransferRunPhase::Prepared
        } else {
            TransferRunPhase::Blocked
        };
    }

    /// The prepare's refusal (React 259-264): the receipt is not
    /// published, the failure is classified and readable the same tick.
    pub(crate) fn fail_prepare(&mut self, message: &str) {
        self.prepare_job_id = None;
        self.fail(Some(classify_transfer_run_failure(message)));
        self.phase = TransferRunPhase::Blocked;
    }

    // -- apply (React 269-344) ----------------------------------------------

    /// The apply's local guards (React 271-284), as a verdict for the host:
    /// `Err` carries the failure to apply with
    /// [`apply_local_refusal`] — an un-admitted plan, or a plan this
    /// machine has already spent (the local refusal keeps the window from
    /// offering an action that cannot succeed, and keeps `planConsumed`
    /// a first-class outcome).
    pub(crate) fn begin_apply(&mut self) -> Result<(), TransferRunFailure> {
        if self.prepare_view.is_none() {
            return Err(TransferRunFailure {
                kind: TransferRunFailureKind::Other,
                message: "apply requires an admitted plan".to_owned(),
            });
        }
        // The planId was already spent (React 278-284).
        if self.phase == TransferRunPhase::Settled
            && self
                .apply_view
                .as_ref()
                .is_some_and(|view| !view.replayed)
        {
            return Err(TransferRunFailure {
                kind: TransferRunFailureKind::PlanConsumed,
                message: "this planId was already applied; review a new plan".to_owned(),
            });
        }
        self.phase = TransferRunPhase::Applying;
        self.fail(None);
        Ok(())
    }

    /// A local refusal lands exactly like a backend one (React 272-273,
    /// 279-283): classified, phase `blocked`, no apply submitted.
    pub(crate) fn apply_local_refusal(&mut self, failure: TransferRunFailure) {
        self.fail(Some(failure));
        self.phase = TransferRunPhase::Blocked;
    }

    /// The request the host sends (React 287-299): the plan's admission
    /// identity, the caller's selection and confirm, and the receipt key —
    /// the caller's, or the plan-scoped default (React 296-299).
    pub(crate) fn apply_request(&self, input: &TransferApplyInput) -> TransferApplyRequestVm {
        let prepare = self.prepare_view.as_ref().expect("begin_apply admitted a plan");
        TransferApplyRequestVm {
            plan_id: prepare.plan_id.clone(),
            plan_digest: prepare.plan_digest.clone(),
            selection_revision: prepare.selection_revision,
            // An absent list means "every table the plan froze", which is
            // what a SQL-file run with no explicit selection means (React
            // 291-293).
            source_tables: input.source_tables.clone(),
            confirmed_destructive: input.confirmed_destructive,
            idempotency_key: input
                .idempotency_key
                .clone()
                .unwrap_or_else(|| format!("data-transfer/apply/{}", prepare.plan_id)),
        }
    }

    /// The apply's ambiguous-reply retry (React 303-321): the receipt was
    /// lost but the apply may have been accepted, so the host repeats the
    /// *exact* request under the same idempotency key — the backend
    /// replays the original Job view and cannot create a second write. A
    /// new key here would turn an ambiguous response into a second
    /// execution attempt.
    ///
    /// The Vm keeps its state (`applying`, no failure): React's catch
    /// retries without touching the failure slots until the retry itself
    /// settles.
    pub(crate) fn begin_apply_retry(&mut self) {
        // No state change beyond staying on the apply path; recorded so
        // the host's contract has one entry point per hop.
    }

    /// The apply's acceptance (React 323-341): the returned id is
    /// published before the next render so cancellation and polling
    /// address this Job while it is still queued or running. An active
    /// receipt keeps `applying`; a terminal one is downgraded to
    /// `unknown` + `pendingVerification` until the details query returns
    /// boundaries and the exact recovery disposition (React 328-338).
    pub(crate) fn finish_apply(&mut self, mut view: TransferApplyJobViewVm) {
        self.apply_job_id = Some(view.job_id.clone());
        let active = is_active_job_state(view.state);
        if active {
            self.phase = TransferRunPhase::Applying;
            self.watch_job_id = Some(view.job_id.clone());
            self.terminal_details_job_id = None;
        } else {
            // A terminal summary is not the durable report. Keep it
            // unknown until the details query returns boundaries and the
            // exact recovery disposition (React 331-337).
            view.effect_outcome = Some("unknown".to_owned());
            view.recovery_verdict = Some("pendingVerification".to_owned());
            view.recovery_reason = Some("jobDetailsPending".to_owned());
            self.phase = TransferRunPhase::Settled;
            self.watch_job_id = None;
            self.terminal_details_job_id = Some(view.job_id.clone());
        }
        self.apply_view = Some(view);
    }

    /// The apply's refusal (React 309-320): classified, phase `blocked`,
    /// nothing submitted.
    pub(crate) fn fail_apply(&mut self, message: &str) {
        self.fail(Some(classify_transfer_run_failure(message)));
        self.phase = TransferRunPhase::Blocked;
    }

    // -- the poll / details projections (React 156-202) ---------------------

    /// React `projectApplyJobView` (`useTransferJobRun.ts:161-175`): the
    /// lightweight Job summary merges over the live apply receipt. A
    /// terminal summary is not the durable report — the outcome stays
    /// unknown until the details query returns boundaries.
    pub(crate) fn apply_job_view(&mut self, job_id: &str, state: JobState, effect_outcome: Option<&str>, cancel_requested: bool) {
        let Some(view) = self.apply_view.as_mut() else {
            return;
        };
        if view.job_id != job_id {
            // React's `applyJobIdRef.current !== jobId` guard (React 400).
            return;
        }
        view.state = state;
        let active = is_active_job_state(state);
        // A terminal summary is not the durable report (React 167-169).
        view.effect_outcome = if active {
            effect_outcome.map(str::to_owned).or_else(|| view.effect_outcome.clone())
        } else {
            Some("unknown".to_owned())
        };
        view.cancelled = state == JobState::Cancelled;
        view.partial = effect_outcome == Some("partiallyApplied") || effect_outcome == Some("unknown");
        if cancel_requested {
            self.cancel_requested = true;
            self.cancel_acknowledged = true;
            self.cancel_unknown_job = false;
        }
    }

    /// React `projectApplyJobDetails` (`useTransferJobRun.ts:178-202`):
    /// the durable terminal read model merges over the live receipt — the
    /// boundaries, the exact recovery disposition, and the error.
    pub(crate) fn apply_terminal_details(
        &mut self,
        job_id: &str,
        state: JobState,
        effect_outcome: Option<&str>,
        error: Option<&str>,
        cancel_requested: bool,
        commit_boundaries: Vec<CommitBoundary>,
        artifact_ids: Vec<String>,
        progress: JobProgress,
        recovery_verdict: Option<&str>,
        recovery_resume_through: Option<i64>,
        recovery_reason: Option<&str>,
    ) {
        let Some(view) = self.apply_view.as_mut() else {
            return;
        };
        if view.job_id != job_id {
            // React's identity mismatch guard (React 355-357, 413-416).
            return;
        }
        view.state = state;
        view.effect_outcome = effect_outcome.map(str::to_owned).or(Some("unknown".to_owned()));
        view.progress = progress;
        view.commit_boundaries = commit_boundaries;
        view.artifact_ids = artifact_ids;
        view.cancelled = state == JobState::Cancelled;
        view.partial =
            effect_outcome == Some("partiallyApplied") || effect_outcome == Some("unknown");
        view.error = error.map(str::to_owned);
        view.recovery_verdict = recovery_verdict.map(str::to_owned);
        view.recovery_resume_through = recovery_resume_through;
        view.recovery_reason = recovery_reason.map(str::to_owned);
        if cancel_requested {
            self.cancel_requested = true;
            self.cancel_acknowledged = true;
            self.cancel_unknown_job = false;
        }
        // React's settle path (React 366-369 / 444-448): still active ->
        // back to `applying` (and re-armed polling); terminal -> settled.
        self.phase = if is_active_job_state(state) {
            self.watch_job_id = Some(job_id.to_owned());
            self.terminal_details_job_id = None;
            TransferRunPhase::Applying
        } else {
            self.watch_job_id = None;
            self.terminal_details_job_id = None;
            TransferRunPhase::Settled
        };
    }

    /// A missing durable report stays uncertain (React 371-385 / 429-443):
    /// the lightweight summary cannot prove terminal boundaries or a
    /// recovery verdict, so the receipt degrades to
    /// `pendingVerification` / `jobDetailsUnavailable`.
    pub(crate) fn apply_details_unavailable(&mut self, job_id: &str) {
        let Some(view) = self.apply_view.as_mut() else {
            return;
        };
        if view.job_id != job_id {
            return;
        }
        view.effect_outcome = Some("unknown".to_owned());
        view.recovery_verdict = Some("pendingVerification".to_owned());
        view.recovery_reason = Some("jobDetailsUnavailable".to_owned());
    }

    // -- cancel (React 473-501) ----------------------------------------------

    /// The cancel request's address resolution (React 473-490): the click
    /// latches the intent, and the addressable id is decided — the apply
    /// id while its Job is still active; the prepare id only while no
    /// apply exists; nothing while admission is in flight (there is no
    /// apply id to guess, and cancelling the prepare Job would not stop
    /// the write).
    ///
    /// Returns the id the host may address, or `None` for the
    /// unknown-Job disposition (React 486-489).
    pub(crate) fn begin_cancel(&mut self) -> Option<String> {
        self.cancel_requested = true;
        if let Some(apply_job_id) = self.apply_job_id.clone() {
            let active = self
                .apply_view
                .as_ref()
                .map(|view| is_active_job_state(view.state))
                .unwrap_or(false);
            if active {
                return Some(apply_job_id);
            }
            // A finished apply Job cannot be cancelled; nothing may be
            // guessed from the prepare id while an apply id exists.
            self.cancel_acknowledged = false;
            self.cancel_unknown_job = true;
            return None;
        }
        if self.phase == TransferRunPhase::Applying {
            self.cancel_acknowledged = false;
            self.cancel_unknown_job = true;
            return None;
        }
        match self.prepare_job_id.clone() {
            Some(job_id) => Some(job_id),
            None => {
                self.cancel_acknowledged = false;
                self.cancel_unknown_job = true;
                None
            }
        }
    }

    /// The backend's answer (React 491-495): `true` only when the cancel
    /// was actually sent *and* the backend answered `true` (it knows the
    /// Job).
    pub(crate) fn finish_cancel(&mut self, acknowledged: bool) {
        self.cancel_acknowledged = acknowledged;
        self.cancel_unknown_job = !acknowledged;
    }

    /// The cancel command's own failure (React 496-500): classified, the
    /// unknown-Job disposition set.
    pub(crate) fn fail_cancel(&mut self, message: &str) {
        self.cancel_unknown_job = true;
        self.fail(Some(classify_transfer_run_failure(message)));
    }

    // -- reset (React 503-515) ------------------------------------------------

    /// React `reset` (React 503-515) — and `reprepare` (React 561-562)
    /// is the same function: return to `idle` for a fresh run; keeps
    /// nothing a recovery needs.
    pub(crate) fn reset(&mut self) {
        self.phase = TransferRunPhase::Idle;
        self.prepare_view = None;
        self.apply_view = None;
        self.fail(None);
        self.cancel_requested = false;
        self.cancel_acknowledged = false;
        self.cancel_unknown_job = false;
        self.prepare_job_id = None;
        self.apply_job_id = None;
        self.watch_job_id = None;
        self.terminal_details_job_id = None;
    }

    /// React `reprepare` alias (React 122-123, 561-562).
    pub(crate) fn reprepare(&mut self) {
        self.reset();
    }

    // -- internal ------------------------------------------------------------

    /// React `fail` (React 234-237): the failure lands in both the state
    /// slot and the same-tick mirror.
    fn fail(&mut self, failure: Option<TransferRunFailure>) {
        self.failure = failure.clone();
        self.failure_slot = failure;
    }
}
