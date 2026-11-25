//! Port of `packages/frontend/src/hooks/useTransferJobRun.ts` (T438,
//! split with `transfer_job_run_helpers.rs`) — the Data Transfer Job
//! execution state machine.
//!
//! The window used to own a single `execute` call and a `cancelled`
//! boolean. That shape cannot say *where* a run stopped, so this
//! hook owns the whole prepare → apply → verdict lifecycle and
//! exposes exactly two truths:
//!
//! - `cancel_requested` — *intent*. Latched the instant the user
//!   clicks.
//! - the verdict input — what actually happened, folded from the
//!   Job's `effectOutcome` + `commitBoundaries` by the shared engine
//!   (`lib/migrationJobVerdict.ts`, **T561 segment 2** — the port
//!   exposes [`Self::verdict_input`], the exact shape that engine
//!   consumes; Schema Diff and Data Sync reuse the same engine
//!   unchanged).
//!
//! Four invariants this hook exists to protect:
//!
//! - A `planId` is spent once. A rejected second apply is surfaced
//!   as `planConsumed` with a re-prepare action, never as a retry
//!   button that would look safe but is not.
//! - A cancel click is not a cancellation. `cancel` stays
//!   unacknowledged until the backend returns a verdict.
//! - A cancel is addressed to a Job id we actually know. Guessing
//!   the apply id from the prepare id would report a cancellation
//!   that never happened.
//! - Prepare and apply never share an `idempotencyKey`. The backend
//!   receipt map is keyed by it, so one shared key would make the
//!   apply *replay the prepare Job* instead of writing anything.
//!
//! ## What maps where
//!
//! - The async `prepare`/`apply`/`requestCancel` bodies split at the
//!   await points: the sync heads and the `note_*` answers below,
//!   with the command rail behind [`TransferJobRunCenter`] (the
//!   host's transport — the desktop adapter's `transferJobs`
//!   commands, T424's shapers).
//! - The two `useEffect`s (the terminal-details read and the watch
//!   poll) are the `terminal_details_due` / `watch_due` due-flags +
//!   their note methods; the 1500ms poll re-arm is the host's timer
//!   (the machine answers [`PollAnswer::ReArmIn1500ms]).
//! - The refs (`prepareJobIdRef` / `applyJobIdRef` / `failureRef`)
//!   are plain fields — the synchronous model has no commit gap, so
//!   [`Self::last_failure`] reads the same failure the next frame
//!   renders (the ref's whole reason is React's batching, which the
//!   port does not have; the API shape stays).
//! - `isAmbiguousApplySubmissionError`'s single same-key retry is
//!   [`Self::apply_submission_ambiguous`]: the host repeats the
//!   exact request once (a new key would turn an ambiguous response
//!   into a second execution attempt).
//!
//! Known deltas: none behavioural — the machine is the port; the
//! verdict fold itself is T561's (the seam answers its input).

// No production consumer yet (the data-transfer window track wires
// it); this crate's tests are the callers meanwhile.
#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use crate::commands::transfer::TransferJob;
use crate::commands::transfer_jobs::{
    TransferApplyJobRequest, TransferApplyJobView, TransferPrepareJobRequest,
    TransferPrepareJobView, local_backend_scope,
};
use crate::hooks::migration_job_hydration_helpers::MigrationJobViewVm;
use crate::hooks::transfer_job_run_helpers::{
    JobDetailsVm, TransferCommandError, TransferRunFailure, TransferRunFailureKind,
    classify_transfer_run_failure, is_active_job_state, is_ambiguous_apply_submission_error,
    is_migration_job_in_flight, job_state_as_str, project_apply_job_details,
    project_apply_job_view, read_job_counter,
};

/// `TransferRunPhase`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TransferRunPhase {
    #[default]
    Idle,
    Preparing,
    Prepared,
    /// Backend-scope / prepare-time refusal: the plan was not
    /// admitted for execution.
    Blocked,
    Applying,
    /// Terminal verdict available — render it, do not offer a blind
    /// retry.
    Settled,
}

/// `TransferApplyInput`: empty/absent source tables for a SQL-file
/// run with no explicit table list; the idempotency key is stable
/// across a retry of *this* apply so the backend replays the
/// recorded receipt instead of committing a second time (never
/// shared with the prepare call; absent lets the machine mint one).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransferApplyInput {
    pub source_tables: Option<Vec<String>>,
    pub confirmed_destructive: bool,
    pub idempotency_key: Option<String>,
}

/// The verdict engine's input (T561's `MigrationJobVerdictInput`):
/// the exact fields the React `useMemo` folds. A commit is only
/// trustworthy when a boundary backs it; the data-transfer handler
/// records no boundaries, so the two counters are the only way the
/// verdict can tell "wrote nothing" from "wrote something nobody
/// recorded".
#[derive(Clone, Debug, PartialEq)]
pub struct TransferVerdictInput {
    pub state: String,
    pub effect_outcome: String,
    pub cancel_requested: bool,
    pub commit_boundaries: Vec<serde_json::Value>,
    pub recovery_verdict: Option<String>,
    pub recovery_reason: Option<String>,
    pub error: Option<String>,
    pub committed_rows: Option<u64>,
    pub unknown_rows: Option<u64>,
}

/// The command rail the host drives (the desktop adapter's
/// `transferJobs` commands; the error is the classified shape).
pub trait TransferJobRunCenter {
    /// `prepare_data_transfer_job`.
    fn prepare(&self, request: &TransferPrepareJobRequest)
        -> Result<TransferPrepareJobView, TransferCommandError>;
    /// `apply_data_transfer_job`.
    fn apply(&self, request: &TransferApplyJobRequest)
        -> Result<TransferApplyJobView, TransferCommandError>;
    /// `get_job`.
    fn get_job(&self, job_id: &str) -> Result<MigrationJobViewVm, TransferCommandError>;
    /// `get_transfer_job_details`.
    fn get_details(&self, job_id: &str) -> Result<JobDetailsVm, TransferCommandError>;
    /// `cancel_data_transfer` — `Ok(true)` only when the backend
    /// knows the Job and recorded the intent.
    fn cancel(&self, job_id: &str) -> Result<bool, TransferCommandError>;
}

/// The watch poll's answer: keep polling (re-arm the 1500ms timer),
/// or stop (the run settled).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PollAnswer {
    ReArmIn1500ms,
    Stop,
}

/// `useTransferJobRun()`'s state machine.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct UseTransferJobRunState {
    pub phase: TransferRunPhase,
    pub prepare_view: Option<TransferPrepareJobView>,
    pub apply_view: Option<TransferApplyJobView>,
    pub failure: Option<TransferRunFailure>,
    /// The click happened. Never rendered as "cancelled".
    pub cancel_requested: bool,
    /// The backend recognised the Job id and recorded the intent.
    pub cancel_acknowledged: bool,
    /// The cancel could not be delivered — either the backend has no
    /// such Job, or no cancellable id existed yet. Never folded into
    /// a successful cancellation.
    pub cancel_unknown_job: bool,
    /// The watch poll's subject (`watchJobId`).
    watch_job_id: Option<String>,
    /// The terminal-details read's subject.
    terminal_details_job_id: Option<String>,
    /// `prepareJobIdRef` — cancelling the *prepare* Job while a plan
    /// is merely frozen is a real cancellation; sending that same id
    /// at an in-flight *apply* would make the backend answer `true`
    /// for a Job nobody asked about.
    prepare_job_id: Option<String>,
    /// `applyJobIdRef` — published at admission, before the worker
    /// settles, so cancellation and polling address this Job while
    /// it is still queued or running.
    apply_job_id: Option<String>,
}

impl UseTransferJobRunState {
    pub fn new() -> Self {
        Self::default()
    }

    /// `lastFailure()` — the synchronous read (the ref's mirror; no
    /// commit gap exists in the port, so it is the failure itself).
    pub fn last_failure(&self) -> Option<&TransferRunFailure> {
        self.failure.as_ref()
    }

    // ---- prepare --------------------------------------------------

    /// `prepare`'s sync head: arm the phase, clear the previous
    /// run. The host then sends [`Self::prepare_request`]'s build
    /// through the center and answers through
    /// [`Self::note_prepare_ok`] / [`Self::note_prepare_err`].
    pub fn prepare_begin(&mut self) {
        self.phase = TransferRunPhase::Preparing;
        self.failure = None;
        self.apply_view = None;
        self.cancel_requested = false;
        self.cancel_acknowledged = false;
        self.cancel_unknown_job = false;
    }

    /// The prepare request the host sends: the job, the scope both
    /// endpoints must present (T424's shape) and the optional stable
    /// key ("preview again" replays the same admitted plan instead
    /// of freezing a second one).
    pub fn prepare_request(
        &self,
        job: &TransferJob,
        idempotency_key: Option<&str>,
    ) -> TransferPrepareJobRequest {
        TransferPrepareJobRequest {
            job: job.clone(),
            backend_scope: local_backend_scope(),
            idempotency_key: idempotency_key.map(str::to_owned),
        }
    }

    /// The prepare's answer: `canExecute` admits the plan
    /// (`prepared`); anything else is a prepare-time block.
    pub fn note_prepare_ok(&mut self, view: TransferPrepareJobView) {
        self.prepare_job_id = Some(view.job_id.clone());
        self.prepare_view = Some(view.clone());
        self.phase = if view.can_execute {
            TransferRunPhase::Prepared
        } else {
            TransferRunPhase::Blocked
        };
    }

    /// The prepare's refusal.
    pub fn note_prepare_err(&mut self, error: &TransferCommandError) {
        self.prepare_job_id = None;
        self.failure = Some(classify_transfer_run_failure(error));
        self.phase = TransferRunPhase::Blocked;
    }

    // ---- apply ----------------------------------------------------

    /// `apply`'s gate + request build: the plan must exist, and a
    /// settled non-replayed apply means the planId was already spent
    /// (the backend would refuse this, but a local refusal keeps the
    /// window from offering an action that cannot succeed, and keeps
    /// `planConsumed` a first-class outcome). `Err` answers the
    /// refusal — the phase and failure already landed.
    pub fn apply_request(
        &mut self,
        input: &TransferApplyInput,
    ) -> Result<TransferApplyJobRequest, TransferRunFailure> {
        let Some(prepare_view) = self.prepare_view.clone() else {
            let failure = TransferRunFailure {
                kind: TransferRunFailureKind::Other,
                message: "apply requires an admitted plan".to_owned(),
            };
            self.failure = Some(failure.clone());
            return Err(failure);
        };
        if self.phase == TransferRunPhase::Settled
            && self
                .apply_view
                .as_ref()
                .is_some_and(|view| !view.replayed)
        {
            let failure = TransferRunFailure {
                kind: TransferRunFailureKind::PlanConsumed,
                message: "this planId was already applied; review a new plan".to_owned(),
            };
            self.failure = Some(failure.clone());
            return Err(failure);
        }
        Ok(TransferApplyJobRequest {
            plan_id: prepare_view.plan_id.clone(),
            plan_digest: prepare_view.plan_digest.clone(),
            selection_revision: prepare_view.selection_revision,
            // An absent list means "every table the plan frozen",
            // which is what a SQL-file run with no explicit selection
            // means.
            selection: crate::commands::transfer_jobs::TransferApplySelection {
                source_tables: input.source_tables.clone(),
            },
            confirmed_destructive: input.confirmed_destructive,
            backend_scope: local_backend_scope(),
            // A plan is spent once, so its id is a stable and
            // naturally scoped receipt key even when a caller did
            // not supply an explicit one.
            idempotency_key: Some(
                input
                    .idempotency_key
                    .clone()
                    .unwrap_or_else(|| format!("data-transfer/apply/{}", prepare_view.plan_id)),
            ),
        })
    }

    /// `apply`'s sync head (after the gate passed): arm the phase,
    /// clear the failure.
    pub fn apply_begin(&mut self) {
        self.phase = TransferRunPhase::Applying;
        self.failure = None;
    }

    /// The apply's answer: publish the returned id before anything
    /// else so cancellation and polling address this Job while it is
    /// still queued or running. A non-active admission answer is
    /// already terminal-but-unverified until the details read lands.
    pub fn note_apply_ok(&mut self, view: TransferApplyJobView) {
        self.apply_job_id = Some(view.job_id.clone());
        let active = is_active_job_state(&view.state);
        self.apply_view = Some(if active {
            view.clone()
        } else {
            let mut unverified = view.clone();
            unverified.effect_outcome = "unknown".to_owned();
            unverified.recovery_verdict = Some("pendingVerification".to_owned());
            unverified.recovery_reason = Some("jobDetailsPending".to_owned());
            unverified
        });
        self.phase = if active {
            TransferRunPhase::Applying
        } else {
            TransferRunPhase::Settled
        };
        self.watch_job_id = if active { Some(view.job_id.clone()) } else { None };
        self.terminal_details_job_id = if active { None } else { Some(view.job_id) };
    }

    /// The apply's refusal split: an ambiguous submission error means
    /// the receipt was lost but the write may have been accepted —
    /// the host repeats the exact request once under the same
    /// idempotency key (the backend replays the original Job view
    /// and cannot create a second write). Every other refusal
    /// settles as a classified block.
    pub fn apply_submission_ambiguous(&mut self, error: &TransferCommandError) -> bool {
        if !is_ambiguous_apply_submission_error(error) {
            self.note_apply_err(error);
            return false;
        }
        true
    }

    /// A refused apply (or the retry's refusal).
    pub fn note_apply_err(&mut self, error: &TransferCommandError) {
        self.failure = Some(classify_transfer_run_failure(error));
        self.phase = TransferRunPhase::Blocked;
    }

    // ---- the terminal-details effect -------------------------------

    /// The effect's arm: the durable report to fetch (once).
    pub fn terminal_details_due(&self) -> Option<&str> {
        self.terminal_details_job_id.as_deref()
    }

    /// The read's answer: the identity must match (`jobId` + the
    /// `dataTransferApply` kind — a mismatch lands in the failure
    /// arm), the receipt merges only while it is still this Job's,
    /// and a still-active state hands the Job back to the watch
    /// poll. Answers whether the effect is done (the subject
    /// cleared) or the watch took over.
    pub fn note_terminal_details(&mut self, details: &JobDetailsVm) -> bool {
        let Some(job_id) = self.terminal_details_job_id.clone() else {
            return false;
        };
        // The stopped/`applyJobIdRef` guard: a superseded Job's late
        // answer never lands.
        if self.apply_job_id.as_deref() != Some(job_id.as_str()) {
            return false;
        }
        if details.job.job_id != job_id || details.job.kind != "dataTransferApply" {
            // `throw new Error('jobDetailsIdentityMismatch')` lands in
            // the catch arm.
            return self.note_terminal_details_failed();
        }
        if let Some(current) = self.apply_view.clone() {
            if current.job_id == job_id {
                self.apply_view = Some(project_apply_job_details(&current, details));
            }
        }
        if details.job.cancel_requested {
            self.cancel_requested = true;
            self.cancel_acknowledged = true;
            self.cancel_unknown_job = false;
        }
        self.terminal_details_job_id = None;
        if is_active_job_state(job_state_as_str(&details.job.state)) {
            self.phase = TransferRunPhase::Applying;
            self.watch_job_id = Some(job_id);
        }
        true
    }

    /// The read's failure: the effect outcome stays unknown and the
    /// recovery stays pending verification — a missing durable
    /// report cannot prove terminal boundaries or a recovery
    /// verdict.
    pub fn note_terminal_details_failed(&mut self) -> bool {
        let Some(job_id) = self.terminal_details_job_id.clone() else {
            return false;
        };
        if self.apply_job_id.as_deref() != Some(job_id.as_str()) {
            return false;
        }
        if let Some(current) = self.apply_view.clone() {
            if current.job_id == job_id {
                let mut next = current;
                next.effect_outcome = "unknown".to_owned();
                next.recovery_verdict = Some("pendingVerification".to_owned());
                next.recovery_reason = Some("jobDetailsUnavailable".to_owned());
                self.apply_view = Some(next);
            }
        }
        self.terminal_details_job_id = None;
        true
    }

    // ---- the watch poll effect -------------------------------------

    /// The effect's arm: the Job to poll.
    pub fn watch_due(&self) -> Option<&str> {
        self.watch_job_id.as_deref()
    }

    /// One poll's answer: the light read merges, the cancel intent
    /// latches, and a non-active state needs the durable report
    /// (the host fetches the details and answers through
    /// [`Self::note_watch_details`] /
    /// [`Self::note_watch_details_failed`]).
    pub fn note_watch_job(&mut self, latest: &MigrationJobViewVm) -> PollAnswer {
        let Some(job_id) = self.watch_job_id.clone() else {
            return PollAnswer::Stop;
        };
        if self.apply_job_id.as_deref() != Some(job_id.as_str()) {
            return PollAnswer::Stop;
        }
        if let Some(current) = self.apply_view.clone() {
            if current.job_id == job_id {
                self.apply_view = Some(project_apply_job_view(&current, latest));
            }
        }
        if latest.cancel_requested {
            self.cancel_requested = true;
            self.cancel_acknowledged = true;
            self.cancel_unknown_job = false;
        }
        if is_active_job_state(job_state_as_str(&latest.state)) {
            PollAnswer::ReArmIn1500ms
        } else {
            PollAnswer::Stop
        }
    }

    /// The terminal details the watch fetched: the identity must
    /// match; the merge and the cancel latch ride it. A still-active
    /// durable state keeps the poll alive (the light read raced a
    /// restart); anything else settles the run.
    pub fn note_watch_details(&mut self, details: &JobDetailsVm) -> PollAnswer {
        let Some(job_id) = self.watch_job_id.clone() else {
            return PollAnswer::Stop;
        };
        if self.apply_job_id.as_deref() != Some(job_id.as_str()) {
            return PollAnswer::Stop;
        }
        let mut terminal_state = job_state_as_str(&details.job.state).to_owned();
        if details.job.job_id != job_id || details.job.kind != "dataTransferApply" {
            // The identity mismatch keeps the light read's state as
            // the terminal state (the React throw lands in the catch
            // arm below).
            return self.note_watch_details_failed();
        }
        if let Some(current) = self.apply_view.clone() {
            if current.job_id == job_id {
                self.apply_view = Some(project_apply_job_details(&current, details));
            }
        }
        if details.job.cancel_requested {
            self.cancel_requested = true;
            self.cancel_acknowledged = true;
            self.cancel_unknown_job = false;
        }
        terminal_state = job_state_as_str(&details.job.state).to_owned();
        let _ = terminal_state;
        if !is_active_job_state(job_state_as_str(&details.job.state)) {
            self.phase = TransferRunPhase::Settled;
            self.watch_job_id = None;
            PollAnswer::Stop
        } else {
            PollAnswer::ReArmIn1500ms
        }
    }

    /// A missing durable report during the watch: the outcome stays
    /// uncertain, and the light read's terminal state still settles
    /// the run (the summary cannot prove boundaries, but it can
    /// prove the Job stopped).
    pub fn note_watch_details_failed(&mut self) -> PollAnswer {
        let Some(job_id) = self.watch_job_id.clone() else {
            return PollAnswer::Stop;
        };
        if let Some(current) = self.apply_view.clone() {
            if current.job_id == job_id {
                let mut next = current;
                next.effect_outcome = "unknown".to_owned();
                next.recovery_verdict = Some("pendingVerification".to_owned());
                next.recovery_reason = Some("jobDetailsUnavailable".to_owned());
                self.apply_view = Some(next);
            }
        }
        // The light read already answered non-active (the caller
        // only fetches details then), so the run settles.
        self.phase = TransferRunPhase::Settled;
        self.watch_job_id = None;
        PollAnswer::Stop
    }

    /// A failed poll: a temporary loss of observation, not a job
    /// failure — retry against the same id; never resubmit the apply
    /// to recover status.
    pub fn note_watch_poll_failed(&mut self) -> PollAnswer {
        if self.watch_job_id.is_some() {
            PollAnswer::ReArmIn1500ms
        } else {
            PollAnswer::Stop
        }
    }

    // ---- cancel ---------------------------------------------------

    /// The cancel's address: the apply id while its Job is active;
    /// before admission there is no apply id to guess (cancelling
    /// the prepare Job would not stop the write) — except while
    /// `applying` without an id yet, where nothing can be addressed;
    /// a merely frozen plan cancels its prepare Job (a real
    /// cancellation). `None` means nothing can be addressed.
    pub fn cancel_target(&self) -> Option<String> {
        if let Some(apply_job_id) = &self.apply_job_id {
            if self
                .apply_view
                .as_ref()
                .is_some_and(|view| is_active_job_state(&view.state))
            {
                return Some(apply_job_id.clone());
            }
            return None;
        }
        if self.phase == TransferRunPhase::Applying {
            return None;
        }
        self.prepare_job_id.clone()
    }

    /// `requestCancel`'s sync head: the intent latches.
    pub fn begin_cancel(&mut self) {
        self.cancel_requested = true;
    }

    /// No addressable id: nothing was cancelled (never treat as
    /// one).
    pub fn note_cancel_no_target(&mut self) {
        self.cancel_acknowledged = false;
        self.cancel_unknown_job = true;
    }

    /// The backend's answer: `true` only when it knows the Job and
    /// recorded the intent.
    pub fn note_cancel_answer(&mut self, acknowledged: bool) {
        self.cancel_acknowledged = acknowledged;
        self.cancel_unknown_job = !acknowledged;
    }

    /// The cancel's own refusal: classified, and the job stays
    /// unknown.
    pub fn note_cancel_err(&mut self, error: &TransferCommandError) {
        self.cancel_unknown_job = true;
        self.failure = Some(classify_transfer_run_failure(error));
    }

    // ---- reset / reprepare ----------------------------------------

    /// `reset` / `reprepare` — return to `idle` for a fresh run;
    /// keeps nothing a recovery needs (a re-prepare mints a new
    /// plan, never re-applies).
    pub fn reset(&mut self) {
        self.phase = TransferRunPhase::Idle;
        self.prepare_view = None;
        self.apply_view = None;
        self.failure = None;
        self.cancel_requested = false;
        self.cancel_acknowledged = false;
        self.cancel_unknown_job = false;
        self.prepare_job_id = None;
        self.apply_job_id = None;
        self.watch_job_id = None;
        self.terminal_details_job_id = None;
    }

    // ---- derived --------------------------------------------------

    /// The verdict fold's input (T561's engine consumes it; `None`
    /// before an apply view exists).
    pub fn verdict_input(&self) -> Option<TransferVerdictInput> {
        let view = self.apply_view.as_ref()?;
        let progress = &view.progress;
        Some(TransferVerdictInput {
            state: view.state.clone(),
            effect_outcome: view.effect_outcome.clone(),
            cancel_requested: self.cancel_requested,
            commit_boundaries: view.commit_boundaries.clone(),
            recovery_verdict: view.recovery_verdict.clone(),
            recovery_reason: view.recovery_reason.clone(),
            error: view.error.clone(),
            committed_rows: progress.get("committed").and_then(read_job_counter),
            unknown_rows: progress.get("unknown").and_then(read_job_counter),
        })
    }

    /// `cancelTargetJobId` — the addressable active apply Job id, or
    /// the prepare Job id before apply.
    pub fn cancel_target_job_id(&self) -> Option<String> {
        if let Some(apply_job_id) = &self.apply_job_id {
            if self
                .apply_view
                .as_ref()
                .is_some_and(|view| is_active_job_state(&view.state))
            {
                return Some(apply_job_id.clone());
            }
            return None;
        }
        if self.phase == TransferRunPhase::Applying {
            return None;
        }
        self.prepare_view.as_ref().map(|view| view.job_id.clone())
    }

    /// `isInFlight` — true while the apply Job is queued/running
    /// (disables "apply" but not "cancel").
    pub fn is_in_flight(&self) -> bool {
        match &self.apply_view {
            Some(view) => is_migration_job_in_flight(&view.state),
            None => self.phase == TransferRunPhase::Applying,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hooks::migration_job_hydration_helpers::{
        EffectOutcome, JobProgress, JobState, MigrationJobViewVm,
    };
    use crate::hooks::transfer_job_run_helpers::JobRecoveryResultVm;
    use serde_json::json;

    fn prepare_view(can_execute: bool) -> TransferPrepareJobView {
        TransferPrepareJobView {
            job_id: "prep-1".to_owned(),
            kind: "data_transfer_prepare".to_owned(),
            state: "succeeded".to_owned(),
            effect_outcome: "committed".to_owned(),
            plan_id: "plan-1".to_owned(),
            plan_digest: "digest".to_owned(),
            plan_version: 1,
            handler_version: 1,
            checkpoint_version: 1,
            selection_revision: 7,
            expires_at: "2026-01-01T00:00:00Z".to_owned(),
            can_execute,
            block_reason: if can_execute { None } else { Some("destructive".to_owned()) },
            review: crate::commands::transfer::TransferPreview {
                plan_id: "plan-1".to_owned(),
                pairing_path: "postgres::postgres".to_owned(),
                mode: crate::commands::transfer::TransferMode::Data,
                write_mode: crate::commands::transfer::WriteMode::Insert,
                ddl: Vec::new(),
                write_plans: Vec::new(),
                warnings: Vec::new(),
                can_execute,
                block_reason: None,
            },
        }
    }

    fn apply_view(state: &str) -> TransferApplyJobView {
        TransferApplyJobView {
            job_id: "apply-1".to_owned(),
            kind: "data_transfer_apply".to_owned(),
            state: state.to_owned(),
            effect_outcome: "unknown".to_owned(),
            progress: json!({ "committed": 5, "unknown": 0 }),
            plan_id: "plan-1".to_owned(),
            plan_digest: "digest".to_owned(),
            selection_revision: 7,
            commit_boundaries: Vec::new(),
            artifact_ids: Vec::new(),
            cancelled: false,
            partial: true,
            replayed: false,
            error: None,
            recovery_verdict: None,
            recovery_resume_through: None,
            recovery_reason: None,
            created_at: None,
            updated_at: None,
        }
    }

    fn job_view(state: JobState, cancel_requested: bool) -> MigrationJobViewVm {
        MigrationJobViewVm {
            job_id: "apply-1".to_owned(),
            kind: "dataTransferApply".to_owned(),
            state,
            stage: None,
            execution_ids: Vec::new(),
            artifact_ids: vec!["transfer-sql-abc".to_owned()],
            created_at: 1,
            updated_at: 2,
            effect_outcome: Some(EffectOutcome::PartiallyApplied),
            cancel_requested,
            pending_verification_reason: None,
            error: None,
            progress: JobProgress { committed: 5, unknown: 0, ..JobProgress::default() },
        }
    }

    fn details(state: JobState) -> JobDetailsVm {
        JobDetailsVm {
            job: job_view(state, false),
            plan_id: Some("plan-1".to_owned()),
            plan_digest: Some("digest".to_owned()),
            selection_revision: Some(7),
            commit_boundaries: vec![json!({ "stageId": "write", "evidence": "sha" })],
            recovery: Some(JobRecoveryResultVm {
                verdict: Some("verified".to_owned()),
                resume_through: Some(3),
                reason_code: None,
            }),
        }
    }

    fn error(message: &str) -> TransferCommandError {
        TransferCommandError { code: None, message: message.to_owned() }
    }

    /// The classifier's four specific arms, in order, then other.
    #[test]
    fn the_classifier_orders_its_arms() {
        assert_eq!(
            classify_transfer_run_failure(&error("backend scope mismatch")).kind,
            TransferRunFailureKind::BackendScope
        );
        assert_eq!(
            classify_transfer_run_failure(&error("pipeline budget exceeded 8 MiB")).kind,
            TransferRunFailureKind::PipelineBudget
        );
        assert_eq!(
            classify_transfer_run_failure(&error("planId already consumed")).kind,
            TransferRunFailureKind::PlanConsumed
        );
        // Stale plan: the code membership.
        assert_eq!(
            classify_transfer_run_failure(&TransferCommandError {
                code: Some("SourceChanged".to_owned()),
                message: "the source changed".to_owned(),
            })
            .kind,
            TransferRunFailureKind::StalePlan
        );
        assert_eq!(
            classify_transfer_run_failure(&error("boom")).kind,
            TransferRunFailureKind::Other
        );
        // The scope arm wins over the budget arm (the order).
        assert_eq!(
            classify_transfer_run_failure(&error("backend scope: pipeline budget")).kind,
            TransferRunFailureKind::BackendScope
        );
    }

    /// The ambiguity detector: the code triple and the transport
    /// phrasings.
    #[test]
    fn the_ambiguity_detector_knows_its_transports() {
        for code in ["OutcomeUnknown", "ServiceUnavailable", "Timeout"] {
            assert!(is_ambiguous_apply_submission_error(&TransferCommandError {
                code: Some(code.to_owned()),
                message: String::new(),
            }));
        }
        for message in [
            "request timed out",
            "gateway timeout",
            "transport reset",
            "connection closed",
            "connection lost",
            "ack lost",
            "acknowledgement lost",
            "outcome unknown",
        ] {
            assert!(is_ambiguous_apply_submission_error(&error(message)), "{message}");
        }
        assert!(!is_ambiguous_apply_submission_error(&error("planId already consumed")));
    }

    /// The prepare lifecycle: admission vs the prepare-time block.
    #[test]
    fn the_prepare_lifecycle_admits_or_blocks() {
        let mut state = UseTransferJobRunState::new();
        state.prepare_begin();
        assert_eq!(state.phase, TransferRunPhase::Preparing);
        state.note_prepare_ok(prepare_view(true));
        assert_eq!(state.phase, TransferRunPhase::Prepared);
        assert_eq!(state.prepare_view.as_ref().unwrap().job_id, "prep-1");
        // A blocked prepare.
        state.prepare_begin();
        state.note_prepare_ok(prepare_view(false));
        assert_eq!(state.phase, TransferRunPhase::Blocked);
        // A refused prepare.
        state.prepare_begin();
        state.note_prepare_err(&error("backend scope mismatch"));
        assert_eq!(state.phase, TransferRunPhase::Blocked);
        assert_eq!(state.failure.as_ref().unwrap().kind, TransferRunFailureKind::BackendScope);
    }

    /// The apply gate: no plan refuses; a spent plan refuses as
    /// `planConsumed`; a fresh apply builds the exact request.
    #[test]
    fn the_apply_gate_guards_the_spent_plan() {
        let mut state = UseTransferJobRunState::new();
        // No admitted plan.
        assert!(state.apply_request(&TransferApplyInput::default()).is_err());
        assert_eq!(state.failure.as_ref().unwrap().message, "apply requires an admitted plan");
        // Admit a plan.
        state.note_prepare_ok(prepare_view(true));
        let request = state
            .apply_request(&TransferApplyInput {
                source_tables: Some(vec!["a".to_owned()]),
                confirmed_destructive: true,
                idempotency_key: None,
            })
            .expect("the gate passes");
        assert_eq!(request.plan_id, "plan-1");
        assert_eq!(request.plan_digest, "digest");
        assert_eq!(request.selection_revision, 7);
        assert_eq!(request.selection.source_tables, Some(vec!["a".to_owned()]));
        assert!(request.confirmed_destructive);
        // The minted receipt key is the plan's own id.
        assert_eq!(
            request.idempotency_key.as_deref(),
            Some("data-transfer/apply/plan-1")
        );
        // An explicit key rides verbatim.
        let request = state
            .apply_request(&TransferApplyInput {
                source_tables: None,
                confirmed_destructive: false,
                idempotency_key: Some("caller-key".to_owned()),
            })
            .expect("the gate passes");
        assert_eq!(request.idempotency_key.as_deref(), Some("caller-key"));
        assert_eq!(request.selection.source_tables, None, "absent list stays absent");
        // Settle a fresh (non-replayed) apply, then re-apply: refused
        // as planConsumed.
        state.apply_begin();
        state.note_apply_ok(apply_view("succeeded"));
        assert_eq!(state.phase, TransferRunPhase::Settled);
        let refusal = state.apply_request(&TransferApplyInput::default()).unwrap_err();
        assert_eq!(refusal.kind, TransferRunFailureKind::PlanConsumed);
        // A replayed receipt may re-apply (the backend replays it).
        let mut replayed = apply_view("succeeded");
        replayed.replayed = true;
        state.apply_begin();
        state.note_apply_ok(replayed);
        assert!(state.apply_request(&TransferApplyInput::default()).is_ok());
    }

    /// The apply answer: an active Job arms the watch; a terminal
    /// admission arms the terminal-details read with the
    /// unverified-pending shape.
    #[test]
    fn the_apply_answer_arms_the_right_effect() {
        let mut state = UseTransferJobRunState::new();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        state.note_apply_ok(apply_view("running"));
        assert_eq!(state.phase, TransferRunPhase::Applying);
        assert_eq!(state.watch_due(), Some("apply-1"));
        assert_eq!(state.terminal_details_due(), None);
        assert!(state.is_in_flight());
        // A terminal admission.
        state.reset();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        state.note_apply_ok(apply_view("succeeded"));
        assert_eq!(state.phase, TransferRunPhase::Settled);
        assert_eq!(state.watch_due(), None);
        assert_eq!(state.terminal_details_due(), Some("apply-1"));
        let view = state.apply_view.as_ref().unwrap();
        assert_eq!(view.effect_outcome, "unknown");
        assert_eq!(view.recovery_verdict.as_deref(), Some("pendingVerification"));
        assert_eq!(view.recovery_reason.as_deref(), Some("jobDetailsPending"));
        assert!(!state.is_in_flight());
    }

    /// The ambiguous submission: one same-key retry, then the
    /// refusal settles.
    #[test]
    fn the_ambiguous_submission_retries_once() {
        let mut state = UseTransferJobRunState::new();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        // An ambiguous loss: the host retries the exact request.
        assert!(state.apply_submission_ambiguous(&error("request timed out")));
        assert_eq!(state.phase, TransferRunPhase::Applying, "still applying");
        assert!(state.failure.is_none());
        // The retry's refusal settles.
        state.note_apply_err(&error("pipeline budget exceeded"));
        assert_eq!(state.phase, TransferRunPhase::Blocked);
        assert_eq!(state.failure.as_ref().unwrap().kind, TransferRunFailureKind::PipelineBudget);
        // A non-ambiguous refusal never retries.
        assert!(!state.apply_submission_ambiguous(&error("planId already consumed")));
    }

    /// The terminal-details read: the identity check, the merge, and
    /// the active hand-back to the watch.
    #[test]
    fn the_terminal_details_merge_and_hand_back() {
        let mut state = UseTransferJobRunState::new();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        state.note_apply_ok(apply_view("succeeded"));
        let due = state.terminal_details_due().expect("the read is due").to_owned();
        // An identity mismatch lands in the failure arm.
        let mut wrong = details(JobState::Succeeded);
        wrong.job.kind = "dataSyncApply".to_owned();
        let _ = due;
        assert!(state.note_terminal_details(&wrong));
        assert_eq!(
            state.apply_view.as_ref().unwrap().recovery_reason.as_deref(),
            Some("jobDetailsUnavailable"),
            "the mismatch degrades to pending verification"
        );
        // The real read merges.
        state.reset();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        state.note_apply_ok(apply_view("succeeded"));
        assert!(state.note_terminal_details(&details(JobState::Succeeded)));
        let view = state.apply_view.as_ref().unwrap();
        assert_eq!(view.effect_outcome, "partiallyApplied");
        assert_eq!(view.commit_boundaries.len(), 1);
        assert_eq!(view.recovery_verdict.as_deref(), Some("verified"));
        assert_eq!(state.terminal_details_due(), None, "the subject cleared");
        // An active durable state hands the Job to the watch.
        state.reset();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        state.note_apply_ok(apply_view("succeeded"));
        assert!(state.note_terminal_details(&details(JobState::Running)));
        assert_eq!(state.phase, TransferRunPhase::Applying);
        assert_eq!(state.watch_due(), Some("apply-1"));
    }

    /// The watch poll: the light merge, the cancel latch, the
    /// terminal details and the settle.
    #[test]
    fn the_watch_poll_settles_through_the_details() {
        let mut state = UseTransferJobRunState::new();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        state.note_apply_ok(apply_view("running"));
        // An active light read re-arms.
        assert_eq!(
            state.note_watch_job(&job_view(JobState::Running, false)),
            PollAnswer::ReArmIn1500ms
        );
        assert_eq!(state.apply_view.as_ref().unwrap().state, "running");
        // A cancel intent in the read latches (never "cancelled").
        assert_eq!(
            state.note_watch_job(&job_view(JobState::Running, true)),
            PollAnswer::ReArmIn1500ms
        );
        assert!(state.cancel_requested);
        assert!(state.cancel_acknowledged);
        assert!(!state.cancel_unknown_job);
        // A terminal light read stops, pending the details. The light
        // summary is not the durable report: the outcome stays
        // unknown until the details land.
        assert_eq!(
            state.note_watch_job(&job_view(JobState::Succeeded, true)),
            PollAnswer::Stop
        );
        assert_eq!(
            state.apply_view.as_ref().unwrap().effect_outcome, "unknown",
            "a terminal summary cannot state an outcome"
        );
        // The details settle the run.
        assert_eq!(state.note_watch_details(&details(JobState::Succeeded)), PollAnswer::Stop);
        assert_eq!(state.phase, TransferRunPhase::Settled);
        assert_eq!(state.watch_due(), None);
        // A failed poll re-arms; a failed details read still settles.
        state.reset();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        state.note_apply_ok(apply_view("running"));
        assert_eq!(state.note_watch_poll_failed(), PollAnswer::ReArmIn1500ms);
        assert_eq!(state.note_watch_job(&job_view(JobState::Failed, false)), PollAnswer::Stop);
        assert_eq!(state.note_watch_details_failed(), PollAnswer::Stop);
        assert_eq!(state.phase, TransferRunPhase::Settled);
        assert_eq!(
            state.apply_view.as_ref().unwrap().recovery_reason.as_deref(),
            Some("jobDetailsUnavailable")
        );
    }

    /// The cancel addressing: the active apply id, the frozen
    /// prepare id, and nothing while applying without an id.
    #[test]
    fn the_cancel_addresses_only_known_ids() {
        let mut state = UseTransferJobRunState::new();
        // Idle: nothing.
        state.begin_cancel();
        assert_eq!(state.cancel_target(), None);
        state.note_cancel_no_target();
        assert!(state.cancel_unknown_job);
        assert!(!state.cancel_acknowledged);
        // A frozen plan: the prepare id is a real cancellation.
        state.reset();
        state.note_prepare_ok(prepare_view(true));
        assert_eq!(state.cancel_target().as_deref(), Some("prep-1"));
        assert_eq!(state.cancel_target_job_id().as_deref(), Some("prep-1"));
        // Applying without an admission id yet: nothing to address.
        state.apply_begin();
        assert_eq!(state.cancel_target(), None, "no id to guess");
        // The apply admission: the apply id while active.
        state.note_apply_ok(apply_view("running"));
        assert_eq!(state.cancel_target().as_deref(), Some("apply-1"));
        assert_eq!(state.cancel_target_job_id().as_deref(), Some("apply-1"));
        // A terminal apply: the apply id no longer addresses anything.
        state.note_watch_job(&job_view(JobState::Succeeded, false));
        assert_eq!(state.cancel_target(), None);
        // The backend's refusal to know the Job.
        state.reset();
        state.note_prepare_ok(prepare_view(true));
        state.begin_cancel();
        state.note_cancel_answer(false);
        assert!(!state.cancel_acknowledged);
        assert!(state.cancel_unknown_job);
        state.note_cancel_answer(true);
        assert!(state.cancel_acknowledged);
        assert!(!state.cancel_unknown_job);
    }

    /// The verdict input: the exact fold shape, counters parsed from
    /// the progress JSON.
    #[test]
    fn the_verdict_input_carries_the_fold_shape() {
        let mut state = UseTransferJobRunState::new();
        assert!(state.verdict_input().is_none());
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        state.note_apply_ok(apply_view("running"));
        let input = state.verdict_input().expect("the fold shape");
        assert_eq!(input.state, "running");
        assert_eq!(input.effect_outcome, "unknown");
        assert!(!input.cancel_requested);
        assert_eq!(input.committed_rows, Some(5));
        assert_eq!(input.unknown_rows, Some(0));
        assert!(input.commit_boundaries.is_empty());
        // A bad counter parses as None (readJobCounter's refusal).
        let mut view = apply_view("running");
        view.progress = json!({ "committed": "not-a-number", "unknown": -3 });
        state.apply_view = Some(view);
        let input = state.verdict_input().expect("the fold shape");
        assert_eq!(input.committed_rows, None);
        assert_eq!(input.unknown_rows, None);
    }

    /// `reset` keeps nothing a recovery needs.
    #[test]
    fn the_reset_keeps_nothing() {
        let mut state = UseTransferJobRunState::new();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        state.note_apply_ok(apply_view("running"));
        state.begin_cancel();
        state.note_cancel_answer(true);
        state.reset();
        assert_eq!(state, UseTransferJobRunState::default());
        // The reprepare alias is the same call.
        state.note_prepare_ok(prepare_view(true));
        state.reset();
        assert_eq!(state, UseTransferJobRunState::default());
    }

    /// A superseded Job's late answer never lands (the
    /// `applyJobIdRef` guard).
    #[test]
    fn a_late_answer_for_a_superseded_job_never_lands() {
        let mut state = UseTransferJobRunState::new();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        state.note_apply_ok(apply_view("succeeded"));
        // A fresh run supersedes the apply id.
        state.reset();
        state.note_prepare_ok(prepare_view(true));
        state.apply_begin();
        let mut second = apply_view("running");
        second.job_id = "apply-2".to_owned();
        state.note_apply_ok(second);
        // The first run's terminal details arrive late.
        assert!(!state.note_terminal_details(&details(JobState::Succeeded)));
        assert_eq!(state.apply_view.as_ref().unwrap().job_id, "apply-2");
        assert_eq!(state.phase, TransferRunPhase::Applying, "the late read changed nothing");
    }
}
