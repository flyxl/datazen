//! T113: `windows/data-transfer/DataTransferTerminalResults.tsx` ported to Rune.
//!
//! The strip that restores the latest settled `dataTransferApply` report
//! from the durable Job details query, mounted above the Data Transfer
//! wizard steps (React mounts it in
//! `windows/data-transfer/DataTransferWindow.tsx`, feeding it
//! `migrationJobs.hydration.jobs`, the current run's `applyView.jobId` as
//! `excludeJobId`, and the step's `hide` while a fresh run is in flight).
//!
//! A summary is never enough to render a completed verdict: while details
//! load, or if the query fails, this surface stays explicitly uncompleted
//! (`data-verdict-severity="uncertain"`, `data-completed="false"`,
//! `data-requires-reconcile="true"`) — never a clean success over an
//! unverifiable write.
//!
//! ## Input boundary
//!
//! * `latestApplyJob` picks the newest `dataTransferApply` job by
//!   `createdAt` (a numeric `Timestamp`, so the comparison is numeric);
//!   the view renders nothing when there is none, when it is excluded by
//!   `excludeJobId`, when `hide` is set, or while it is still queued or
//!   running.
//! * The details fetch is async and host-owned: React's
//!   `transferJobCommands.getDetails` promise (with the `subscribed`
//!   closure guard against a settled-but-stale effect) becomes the
//!   [`TerminalResultsState`] reducer — the host resets it when the
//!   terminal-job identity changes ([`TerminalResultsState::begin_load`],
//!   the effect's synchronous preamble) and applies the settlement
//!   ([`TerminalResultsState::finish_load`], the promise's `then` /
//!   `catch` with the `subscribed` guard re-checked against the terminal
//!   job the view shows *now*).
//!
//! ## Key convention
//!
//! React `data-testid` becomes the element key, verbatim; the eight root
//! `data-*` attributes (which rune's `El` cannot set natively) are carried
//! by a hidden keyed node as a `|`-separated label, exactly as the verdict
//! panel's own mirror does:
//!
//! | part | key |
//! |------|-----|
//! | root (absent when there is no terminal job) | `data-transfer-result` |
//! | data-* mirror | `data-transfer-result-attrs` (internal) |
//! | uncompleted status line (`role="status"`) | `data-transfer-result-status` (internal) |
//! | restored title line | `data-transfer-result-title` (internal) |
//!
//! ## T561 placeholder (must be read before extending)
//!
//! `deriveMigrationJobVerdict` / `isMigrationJobInFlight` live in React's
//! `lib/migrationJobVerdict.ts`, whose real port is **T561 (segment 2,
//! pending)** — a cross-segment dependency. Following the
//! `transferMappingView` placeholder precedent, this module carries them
//! as pure functions transliterated line for line from the TypeScript
//! source (each function's doc names its TS reference), so this slice gets
//! real verdicts today and T561's port replaces them without changing the
//! seams. **Do not extend them here with new semantics**: anything T561
//! has not ported belongs here only as a labelled placeholder, and
//! anything it has ported replaces the placeholder here.
//!
//! ## Deliberate deviations
//!
//! * **`Array.isArray(boundary.evidence)`** in the verified-boundary count
//!   collapses to `!evidence.is_empty()`: Rust's `Vec<String>` cannot be
//!   `undefined`, which is the only case the `Array.isArray` guard rejects.
//! * **`useLocaleDomains(['sync'])`** has no Rune counterpart: the port's
//!   generated zh-CN table already covers every pack, so the call needs no
//!   model (the parity test asserts the three keys it gates exist).
//! * **`data-replayed="false"`** and the fixed uncompleted attribute set
//!   are constants in both branches; they ride the same mirror label.
//! * **`updatedAt` is part of the effect identity**: React re-runs the
//!   fetch when `[terminalJob?.jobId, terminalJob?.updatedAt]` changes
//!   even for the same job id, so [`TerminalJobView`] carries it and
//!   [`TerminalResultsState::finish_load`] compares it.

use std::rc::Rc;

use rune::{col, el, row, text, El, Tx};
use rune_kit::Kit;

use crate::components::migration::migration_job_verdict_panel::{
    read_job_counter, CancelDisposition, CommitBoundary, JobProgress, MigrationJobVerdict,
    MigrationJobVerdictPanel, PlanInfo, UncertaintyReason, VerdictPanelVm, VerdictSeverity,
};
use crate::i18n::t;
use crate::ui::kit::kit;
use crate::views::data_transfer::data_transfer_attached_jobs::JobState;

// ---------------------------------------------------------------------------
// T561 placeholder: `lib/migrationJobVerdict.ts` (segment 2) — transliterated.
// ---------------------------------------------------------------------------

/// React `isMigrationJobInFlight` (`lib/migrationJobVerdict.ts:151-153`):
/// true while the Job is still queued or running.
#[allow(dead_code)] // T561 replaces this placeholder; wired by this slice's tests meanwhile
pub(crate) fn is_migration_job_in_flight(state: &JobState) -> bool {
    matches!(state, JobState::Queued | JobState::Running)
}

/// React `MigrationJobVerdictInput` (`lib/migrationJobVerdict.ts:73-92`).
/// `state` is the closed five-state enum of the Job wire; the effect
/// outcome stays the raw wire string because every React consumer of it
/// takes `EffectOutcome | string | null` and matches on the literal
/// spellings.
#[allow(dead_code)] // T561 replaces this placeholder; wired by this slice's tests meanwhile
pub(crate) struct MigrationJobVerdictInput<'a> {
    pub state: JobState,
    pub effect_outcome: Option<&'a str>,
    pub cancel_requested: bool,
    pub commit_boundaries: Option<&'a [CommitBoundary]>,
    pub recovery_verdict: Option<&'a str>,
    pub recovery_reason: Option<&'a str>,
    pub error: Option<&'a str>,
    pub committed_rows: Option<u64>,
    pub unknown_rows: Option<u64>,
}

/// React `deriveCancelDisposition` (`lib/migrationJobVerdict.ts:162-187`),
/// line for line. The `cancelled` / `partial` booleans the old command
/// returned are not inputs on purpose: they cannot distinguish "cancelled
/// before writing" from "cancelled after committing".
#[allow(dead_code)] // T561 replaces this placeholder; wired by this slice's tests meanwhile
fn derive_cancel_disposition(
    effect_outcome: Option<&str>,
    cancel_requested: bool,
    boundaries: Option<&[CommitBoundary]>,
    in_flight: bool,
) -> CancelDisposition {
    if !cancel_requested {
        return CancelDisposition::None;
    }
    if in_flight {
        return CancelDisposition::RequestedInFlight;
    }
    let committed = boundaries.map_or(0, |list| list.len()) > 0;
    match effect_outcome {
        // The write finished before the cancel landed; reporting this as
        // "cancelled" would be the exact lie the cancel model forbids.
        Some("completed") => CancelDisposition::SettledCompleted,
        Some("rolledBack") => CancelDisposition::SettledRolledBack,
        Some("partiallyApplied") => CancelDisposition::SettledPartially,
        Some("unknown") => CancelDisposition::SettledUnknown,
        // `notStarted`, an absent outcome, or an unknown string. A
        // committed boundary still contradicts "nothing happened".
        _ => {
            if committed {
                CancelDisposition::SettledPartially
            } else {
                CancelDisposition::NotStarted
            }
        }
    }
}

/// React `deriveUncertainty` (`lib/migrationJobVerdict.ts:193-241`), line
/// for line. Recovery adjudication: anything other than
/// `resumeAfterVerify` means the backend refuses to certify the write.
#[allow(dead_code)] // T561 replaces this placeholder; wired by this slice's tests meanwhile
fn derive_uncertainty(
    effect_outcome: Option<&str>,
    recovery_verdict: Option<&str>,
    recovery_reason: Option<&str>,
    unverified_boundaries: u64,
    has_unbacked_commit: bool,
    has_write_evidence: bool,
) -> UncertaintyReason {
    if effect_outcome == Some("unknown") {
        return UncertaintyReason::EffectOutcomeUnknown;
    }
    if recovery_verdict == Some("requireManualReview") {
        return UncertaintyReason::ManualReviewRequired;
    }
    if recovery_verdict == Some("reject") {
        return UncertaintyReason::RecoveryRejected;
    }
    // Only explicit backend proofs may clear the fence. A restart before
    // dispatch and a confirmed no-write/rollback outcome both require zero
    // committed or unknown rows and no commit boundaries.
    if recovery_verdict == Some("notExecuted") {
        let restart_before_dispatch = effect_outcome == Some("notStarted")
            && recovery_reason == Some("notDispatchedAfterRestart");
        let no_committed_writes = matches!(effect_outcome, Some("notStarted") | Some("rolledBack"))
            && recovery_reason == Some("noCommittedWrites");
        if (!restart_before_dispatch && !no_committed_writes) || has_write_evidence {
            return UncertaintyReason::RecoveryRejected;
        }
    }
    if let Some(verdict) = recovery_verdict {
        if verdict != "resumeAfterVerify" && verdict != "notExecuted" {
            return UncertaintyReason::RecoveryRejected;
        }
    }
    // A recorded reason is the fail-closed evidence even when the verdict
    // itself stays optimistic; "no checkpoint was recorded" is the
    // canonical one (`/no checkpoint/i`).
    if let Some(reason) = recovery_reason {
        if reason.to_lowercase().contains("no checkpoint") {
            return UncertaintyReason::NoCheckpoint;
        }
    }
    // Rows without a boundary behind them are an evidence gap, checked
    // before the per-boundary count because a structurally empty list
    // would otherwise make this unreachable.
    if has_unbacked_commit {
        return UncertaintyReason::MissingEvidence;
    }
    if unverified_boundaries > 0 {
        return UncertaintyReason::MissingEvidence;
    }
    UncertaintyReason::None
}

/// React `pickSeverity` (`lib/migrationJobVerdict.ts:303-330`), line for
/// line — including its final quirk: the `none` / default arm returns
/// `'partial'` for *both* branches of
/// `input.state === 'succeeded' ? 'partial' : 'partial'`.
#[allow(dead_code)] // T561 replaces this placeholder; wired by this slice's tests meanwhile
fn pick_severity(
    in_flight: bool,
    uncertainty: UncertaintyReason,
    completed: bool,
    state: &JobState,
    cancel_disposition: CancelDisposition,
    has_error: bool,
) -> VerdictSeverity {
    // Uncertainty outranks everything: an undecided verdict must never be
    // presented as a clean one.
    if uncertainty != UncertaintyReason::None {
        return VerdictSeverity::Uncertain;
    }
    if completed {
        return VerdictSeverity::Ok;
    }
    if in_flight {
        return VerdictSeverity::Partial;
    }
    if has_error || *state == JobState::Failed {
        return VerdictSeverity::Failed;
    }
    match cancel_disposition {
        CancelDisposition::NotStarted => VerdictSeverity::Failed,
        CancelDisposition::SettledRolledBack => VerdictSeverity::Failed,
        // Completed on its own merits; `completed` above already returned 'ok'.
        CancelDisposition::SettledCompleted => VerdictSeverity::Ok,
        CancelDisposition::SettledPartially => VerdictSeverity::Partial,
        CancelDisposition::SettledUnknown => VerdictSeverity::Uncertain,
        CancelDisposition::RequestedInFlight => VerdictSeverity::Partial,
        // React's `case 'none': default:` — both ternary arms are 'partial'.
        CancelDisposition::None => VerdictSeverity::Partial,
    }
}

/// React `deriveMigrationJobVerdict` (`lib/migrationJobVerdict.ts:241-278`),
/// line for line: folds the raw Job verdict into the state a user is shown.
#[allow(dead_code)] // T561 replaces this placeholder; wired by this slice's tests meanwhile
pub(crate) fn derive_migration_job_verdict(
    input: &MigrationJobVerdictInput<'_>,
) -> MigrationJobVerdict {
    let boundaries = input.commit_boundaries.unwrap_or(&[]);
    let verified_boundaries = boundaries
        .iter()
        .filter(|boundary| !boundary.evidence.is_empty())
        .count() as u64;
    let unverified_boundaries = boundaries.len() as u64 - verified_boundaries;
    let in_flight = is_migration_job_in_flight(&input.state);
    let cancel_disposition = derive_cancel_disposition(
        input.effect_outcome,
        input.cancel_requested,
        input.commit_boundaries,
        in_flight,
    );
    let uncertainty = derive_uncertainty(
        input.effect_outcome,
        input.recovery_verdict,
        input.recovery_reason,
        unverified_boundaries,
        // A commit is only trustworthy when a boundary backs it. Rows with
        // no boundary at all are strictly worse than a boundary missing its
        // `EVIDENCE_*` markers, and both must fail closed.
        boundaries.is_empty()
            && (input.committed_rows.unwrap_or(0) > 0 || input.unknown_rows.unwrap_or(0) > 0),
        !boundaries.is_empty()
            || input.committed_rows.unwrap_or(0) > 0
            || input.unknown_rows.unwrap_or(0) > 0,
    );
    // "Completed" means finished *and* certifiable. An `effectOutcome` of
    // `completed` alone is the backend's progress flag, not proof — letting
    // it pass while uncertainty is open is how an unverifiable write gets
    // presented as a clean success.
    let completed = input.effect_outcome == Some("completed")
        && unverified_boundaries == 0
        && !in_flight
        && uncertainty == UncertaintyReason::None;
    let severity = pick_severity(
        in_flight,
        uncertainty,
        completed,
        &input.state,
        cancel_disposition,
        input.error.is_some(),
    );
    MigrationJobVerdict {
        severity,
        cancel_disposition,
        completed,
        verified_boundaries,
        unverified_boundaries,
        uncertainty,
        requires_reconcile: in_flight
            || uncertainty != UncertaintyReason::None
            || cancel_disposition == CancelDisposition::SettledUnknown,
    }
}

// ---------------------------------------------------------------------------
// The component's own model.
// ---------------------------------------------------------------------------

/// The fields of `JobView` (`backend-client/types/jobs.ts:92-118`) this
/// component and its verdict read. `state` is the closed five-state enum
/// (the wire never carries anything else); `effect_outcome` stays the raw
/// wire string because the verdict's derive functions match on the literal
/// spellings. `created_at` / `updated_at` are numeric `Timestamp`s.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub(crate) struct TerminalJobView {
    pub job_id: String,
    pub kind: String,
    pub state: JobState,
    pub created_at: f64,
    pub updated_at: f64,
    pub effect_outcome: Option<String>,
    pub cancel_requested: bool,
    pub error: Option<String>,
    pub progress: JobProgress,
    pub artifact_ids: Vec<String>,
}

/// React `JobRecoveryResult` (`backend-client/types/jobs.ts:155-160`): a
/// bounded, stable recovery receipt.
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub(crate) struct JobRecoveryResult {
    pub verdict: String,
    pub resume_through: Option<u64>,
    pub reason_code: Option<String>,
}

/// The fields of `JobDetails` (`backend-client/types/jobs.ts:164-174`) this
/// component reads (state version, domain results and recovery targets
/// never reach this strip).
#[derive(Clone, Debug, PartialEq)]
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub(crate) struct TerminalJobDetails {
    pub job: TerminalJobView,
    pub plan_id: Option<String>,
    pub plan_digest: Option<String>,
    pub selection_revision: Option<u64>,
    pub commit_boundaries: Vec<CommitBoundary>,
    pub recovery: Option<JobRecoveryResult>,
}

/// React `latestApplyJob` (`DataTransferTerminalResults.tsx:14-18`): the
/// newest `dataTransferApply` by `createdAt` (numeric `Timestamp`
/// comparison; a tie keeps the earlier job, as React's strict `>` does).
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub(crate) fn latest_apply_job(jobs: &[TerminalJobView]) -> Option<&TerminalJobView> {
    jobs.iter()
        .filter(|job| job.kind == "dataTransferApply")
        .reduce(|latest, job| if job.created_at > latest.created_at { job } else { latest })
}

/// React's `terminalJob` derivation (`DataTransferTerminalResults.tsx:28-31`):
/// the latest apply job, unless it is still in flight, excluded by
/// `excludeJobId`, or `hide` is set. `None` renders nothing.
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub(crate) fn terminal_job<'a>(
    jobs: &'a [TerminalJobView],
    exclude_job_id: Option<&str>,
    hide: bool,
) -> Option<&'a TerminalJobView> {
    if hide {
        return None;
    }
    let latest = latest_apply_job(jobs)?;
    if is_migration_job_in_flight(&latest.state) {
        return None;
    }
    // React's `latest.jobId !== excludeJobId`: `undefined` / `null` never
    // excludes, exactly as `jobId !== undefined` never did in T073.
    if exclude_job_id == Some(latest.job_id.as_str()) {
        return None;
    }
    Some(latest)
}

/// React's `useState` pair (`DataTransferTerminalResults.tsx:33-34`) plus
/// the `useEffect` body (`DataTransferTerminalResults.tsx:36-60`) as the
/// host-facing reducer. Rune has no component-local `useState`, so this
/// lives in the app's state `S`; the host drives it.
#[derive(Clone, Debug, Default, PartialEq)]
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub(crate) struct TerminalResultsState {
    /// React's `loaded`: `{ jobId, details } | null`.
    pub loaded: Option<(String, TerminalJobDetails)>,
    /// React's `failedJobId`: `string | null`.
    pub failed_job_id: Option<String>,
}

#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
impl TerminalResultsState {
    /// The effect's synchronous preamble, run at the top of every effect
    /// pass (`setLoaded(null); setFailedJobId(null)`): the host calls this
    /// whenever the terminal-job identity changes — on mount, when the
    /// terminal job becomes another job or moves on (`updatedAt`
    /// changes), or when it goes away — and then starts the new
    /// `getDetails` fetch.
    pub(crate) fn begin_load(&mut self) {
        self.loaded = None;
        self.failed_job_id = None;
    }

    /// The `getDetails` settlement for the load started with `captured`
    /// (the terminal job the effect closed over). `current` is the
    /// terminal job the view shows *now*: a different identity (id or
    /// `updatedAt`, or no terminal job at all) means the effect re-ran, so
    /// the closure's `subscribed` is false and the settlement is dropped
    /// untouched. Returns whether it was applied.
    ///
    /// * `Err(())` — the promise rejected: `failedJobId = captured.jobId`.
    /// * `Ok(details)` with `details.job.jobId !== captured.jobId` or the
    ///   details' job still in flight — the exact same failed branch.
    /// * Otherwise `loaded = (captured.jobId, details)`.
    pub(crate) fn finish_load(
        &mut self,
        captured: &TerminalJobView,
        current: Option<&TerminalJobView>,
        outcome: Result<TerminalJobDetails, ()>,
    ) -> bool {
        let subscribed = match current {
            Some(now) => now.job_id == captured.job_id && now.updated_at == captured.updated_at,
            None => false,
        };
        if !subscribed {
            return false;
        }
        match outcome {
            Err(()) => {
                self.failed_job_id = Some(captured.job_id.clone());
                true
            }
            Ok(details) => {
                if details.job.job_id != captured.job_id
                    || is_migration_job_in_flight(&details.job.state)
                {
                    self.failed_job_id = Some(captured.job_id.clone());
                    true
                } else {
                    self.loaded = Some((captured.job_id.clone(), details));
                    true
                }
            }
        }
    }
}

/// The restored terminal-results strip. `S` is the host state that owns the
/// verdict panel's copied-feedback deadline (React's `useCopyFeedback`
/// hook, reached through `MigrationJobVerdictPanel`).
#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
pub(crate) struct DataTransferTerminalResults<S: 'static> {
    jobs: Vec<TerminalJobView>,
    exclude_job_id: Option<String>,
    hide: bool,
    loaded: Option<(String, TerminalJobDetails)>,
    failed_job_id: Option<String>,
    copied: bool,
    on_copy: Option<std::rc::Rc<dyn Fn(&mut S, &mut Tx)>>,
}

#[allow(dead_code)] // reusable component; wired by the Data Transfer window track
impl<S: 'static> DataTransferTerminalResults<S> {
    /// React's props: `jobs`, `excludeJobId`, `hide`.
    pub(crate) fn new(
        jobs: Vec<TerminalJobView>,
        exclude_job_id: Option<String>,
        hide: bool,
    ) -> Self {
        Self {
            jobs,
            exclude_job_id,
            hide,
            loaded: None,
            failed_job_id: None,
            copied: false,
            on_copy: None,
        }
    }

    /// The `loaded` `useState` snapshot as of this frame (the host copies
    /// it out of [`TerminalResultsState`] each render).
    pub(crate) fn loaded(mut self, loaded: Option<(String, TerminalJobDetails)>) -> Self {
        self.loaded = loaded;
        self
    }

    /// The `failedJobId` `useState` snapshot as of this frame.
    pub(crate) fn failed_job_id(mut self, failed_job_id: Option<String>) -> Self {
        self.failed_job_id = failed_job_id;
        self
    }

    /// Whether the verdict panel's transient "已复制" feedback is visible
    /// this frame (`now < copied_until`, computed by the host).
    pub(crate) fn copied(mut self, copied: bool) -> Self {
        self.copied = copied;
        self
    }

    /// The verdict panel's `useCopyFeedback` deadline write, forwarded
    /// from the host. Without it the panel's error copy button renders but
    /// does nothing, like a React panel whose hook never resolves.
    pub(crate) fn on_copy_tx(mut self, f: impl Fn(&mut S, &mut Tx) + 'static) -> Self {
        self.on_copy = Some(Rc::new(f));
        self
    }

    /// Builds the strip, or `el()` (nothing) when there is no terminal job
    /// — React's early `return null`.
    pub(crate) fn build(self, dark: bool) -> El {
        let k = kit(dark);
        let Some(terminal) = terminal_job(&self.jobs, self.exclude_job_id.as_deref(), self.hide)
        else {
            return el();
        };
        // React: `loaded?.jobId === terminalJob.jobId ? loaded.details : null`.
        let details = self
            .loaded
            .as_ref()
            .filter(|(job_id, _)| job_id == &terminal.job_id)
            .map(|(_, details)| details);
        match details {
            None => self.uncompleted_surface(k, terminal),
            Some(details) => self.restored_surface(k, dark, details),
        }
    }

    /// The explicitly-uncompleted surface: React's
    /// `section.rounded border border-warning/30 bg-warning/5 px-4 py-3
    /// text-sm text-fg` with one `role="status"` line — `detailsLoading`
    /// while the query runs, `detailsUnavailable` once it has failed.
    fn uncompleted_surface(&self, k: &'static Kit, terminal: &TerminalJobView) -> El {
        // React: `failedJobId === terminalJob.jobId`.
        let failed = self.failed_job_id.as_deref() == Some(terminal.job_id.as_str());
        let label = format!(
            "job_id={}|restored=true|verdict_severity=uncertain|completed=false|replayed=false|cancel_disposition=none|requires_reconcile=true",
            terminal.job_id
        );
        col()
            .key("data-transfer-result")
            .w_full()
            .radius(4.0)
            .border(1.0, k.alpha(k.c.warning.fg, 0.3))
            .fill(k.alpha(k.c.warning.fg, 0.05))
            .px(16.0)
            .py(12.0)
            .child(attrs_mirror("data-transfer-result-attrs", &label))
            .child(
                text(t(if failed {
                    "transfer.job.detailsUnavailable"
                } else {
                    "transfer.job.detailsLoading"
                }))
                .key("data-transfer-result-status")
                .role("status")
                .font(k.ty.text_small)
                .color(k.c.text_base),
            )
    }

    /// The restored surface: the title line and the shared verdict panel,
    /// fed by the derived verdict and the plan identity.
    fn restored_surface(&self, k: &'static Kit, dark: bool, details: &TerminalJobDetails) -> El {
        // React: `const effectOutcome = job.effectOutcome ?? 'unknown'`.
        let effect_outcome = details.job.effect_outcome.as_deref().or(Some("unknown"));
        let verdict = derive_migration_job_verdict(&MigrationJobVerdictInput {
            state: details.job.state,
            effect_outcome,
            cancel_requested: details.job.cancel_requested,
            commit_boundaries: Some(&details.commit_boundaries),
            recovery_verdict: details.recovery.as_ref().map(|r| r.verdict.as_str()),
            recovery_reason: details.recovery.as_ref().and_then(|r| r.reason_code.as_deref()),
            error: details.job.error.as_deref(),
            committed_rows: read_job_counter(&details.job.progress.committed),
            unknown_rows: read_job_counter(&details.job.progress.unknown),
        });
        let label = format!(
            "job_id={}|restored=true|verdict_severity={}|completed={}|replayed=false|cancel_disposition={}|requires_reconcile={}|plan_id={}",
            details.job.job_id,
            verdict.severity.as_str(),
            verdict.completed,
            verdict.cancel_disposition.as_str(),
            verdict.requires_reconcile,
            details.plan_id.as_deref().unwrap_or(""),
        );
        let vm = VerdictPanelVm {
            verdict,
            progress: Some(details.job.progress.clone()),
            commit_boundaries: details.commit_boundaries.clone(),
            artifact_ids: details.job.artifact_ids.clone(),
            cancel_requested: details.job.cancel_requested,
            // React passes `cancelAcknowledged={job.cancelRequested}` — the
            // same fact twice, deliberately: the panel's "cancel went to a
            // job that does not exist" note only ever fires when the two
            // differ, which this call site never lets happen.
            cancel_acknowledged: details.job.cancel_requested,
            recovery_verdict: details.recovery.as_ref().map(|r| r.verdict.clone()),
            recovery_reason: details
                .recovery
                .as_ref()
                .and_then(|r| r.reason_code.clone()),
            recovery_resume_through: details.recovery.as_ref().and_then(|r| r.resume_through),
            error: details.job.error.clone(),
            plan: Some(PlanInfo {
                plan_id: details.plan_id.clone(),
                plan_digest: details.plan_digest.clone(),
                selection_revision: details.selection_revision,
            }),
            test_id_prefix: "data-transfer-job".to_owned(),
            // React passes no `replayed` prop here: the panel default (false).
            ..VerdictPanelVm::default()
        };
        let mut panel = MigrationJobVerdictPanel::<S>::new(vm).copied(self.copied);
        if let Some(on_copy) = self.on_copy.clone() {
            panel = panel.on_copy_tx(move |s: &mut S, tx: &mut Tx| on_copy(s, tx));
        }
        // React: `section.flex flex-col gap-2` with the `text-xs font-medium
        // text-fg-muted` title and the panel.
        col()
            .key("data-transfer-result")
            .w_full()
            .gap(8.0)
            .child(attrs_mirror("data-transfer-result-attrs", &label))
            .child(
                text(t("transfer.job.restoredTitle"))
                    .key("data-transfer-result-title")
                    .font(k.ty.text_xsmall_medium)
                    .color(k.c.text_muted),
            )
            .child(panel.build(dark))
    }
}

/// The hidden keyed node carrying the root's `data-*` attributes as a
/// `|`-separated label, exactly as the verdict panel's own mirror does:
/// rune's `El` has no arbitrary data-attribute API.
fn attrs_mirror(key: &str, label: &str) -> El {
    row()
        .key(key)
        .size(0.0, 0.0)
        .absolute(0.0, 0.0)
        .a11y_hidden()
        .label(label)
}
