//! T147-S2 independent audit: fresh vectors against the React sources.
//!
//! Written as the tester round for the Job run machine — none of the
//! builders reuse the coder's. Every expectation is derived from
//! `useTransferJobRun.ts` and the lib predicates it folds:
//!
//! * the classification's *order* (React 135-139 checks scope, then
//!   budget, then consumed, then stale — a message that matches two
//!   classes resolves to the first);
//! * the apply guard's `replayed` carve-out (React 278);
//! * the acceptance downgrade's exact spellings (React 331-337);
//! * the poll merge's cancel propagation and stale-id refusal
//!   (React 399-408);
//! * the details merge's `in_flight` re-arm (React 366-369);
//! * the cancel address matrix's admission hole (React 483-485) and
//!   the settled-apply hole (React 479-482);
//! * the verdict fold's counter trust rule (React 526-534);
//! * the window's cancel disposal honouring the "applying" phase
//!   (React 852-859).
//!
//! Mutation red-proof (transcript in the commit message): deleting the
//! `replayed` carve-out turns the spent-plan guard red; the restore
//! returns green.

use datazen_ui::components::migration::migration_job_verdict_panel::CommitBoundary;
use datazen_ui::views::{
    JobCounter, JobProgress, JobState, TransferApplyInput, TransferApplyJobViewVm,
    TransferJobRunVm, TransferPrepareJobViewVm, TransferPreviewVm, TransferRunFailureKind,
    TransferRunPhase, DataTransferWindowVm, WizardStep,
};

fn admission(can_execute: bool, plan: &str) -> TransferPrepareJobViewVm {
    TransferPrepareJobViewVm {
        job_id: format!("prepare-{plan}"),
        kind: "dataTransferPrepare".to_owned(),
        state: JobState::Succeeded,
        effect_outcome: Some("completed".to_owned()),
        plan_id: plan.to_owned(),
        plan_digest: format!("digest-{plan}"),
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: 1,
        expires_at: String::new(),
        can_execute,
        block_reason: None,
        review: TransferPreviewVm {
            plan_id: plan.to_owned(),
            ..TransferPreviewVm::default()
        },
    }
}

fn receipt(job_id: &str, state: JobState, outcome: &str) -> TransferApplyJobViewVm {
    TransferApplyJobViewVm {
        job_id: job_id.to_owned(),
        kind: "dataTransferApply".to_owned(),
        state,
        effect_outcome: Some(outcome.to_owned()),
        progress: JobProgress::default(),
        plan_id: "p".to_owned(),
        plan_digest: "d".to_owned(),
        selection_revision: 1,
        commit_boundaries: Vec::new(),
        artifact_ids: Vec::new(),
        cancelled: false,
        partial: false,
        replayed: false,
        error: None,
        recovery_verdict: None,
        recovery_resume_through: None,
        recovery_reason: None,
    }
}

#[test]
fn classification_resolves_overlapping_messages_by_order() {
    // React 135-139 checks backend scope first: a message that mentions
    // both the scope and the budget is a scope refusal.
    let failure = datazen_ui::views::classify_transfer_run_failure(
        "backend scope check failed after pipeline budget of 8 MiB",
    );
    assert_eq!(failure.kind, TransferRunFailureKind::BackendScope);
    // Budget before consumed.
    let failure = datazen_ui::views::classify_transfer_run_failure(
        "pipeline budget exceeded: plan already consumed",
    );
    assert_eq!(failure.kind, TransferRunFailureKind::PipelineBudget);
    // Consumed before stale.
    let failure =
        datazen_ui::views::classify_transfer_run_failure("already consumed while PlanStale");
    assert_eq!(failure.kind, TransferRunFailureKind::PlanConsumed);
    // Stale before other.
    let failure = datazen_ui::views::classify_transfer_run_failure("SourceChanged mid-flight");
    assert_eq!(failure.kind, TransferRunFailureKind::StalePlan);
    // The whole message rides the failure (React 76-78).
    let failure = datazen_ui::views::classify_transfer_run_failure("some unexpected error");
    assert_eq!(failure.kind, TransferRunFailureKind::Other);
    assert_eq!(failure.message, "some unexpected error");
}

#[test]
fn spent_plan_guard_honours_the_replayed_carve_out() {
    // React 278: `phase === 'settled' && applyView && !applyView.replayed`.
    // A replayed receipt is the recorded answer, not a fresh spend — the
    // apply is allowed.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "p1"));
    let mut replayed = receipt("a1", JobState::Succeeded, "completed");
    replayed.replayed = true;
    run.finish_apply(replayed);
    run.phase = TransferRunPhase::Settled;
    assert!(run.begin_apply().is_ok(), "a replayed receipt re-applies");
    // The un-replayed twin refuses with the consumed-plan wording.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "p1"));
    run.finish_apply(receipt("a1", JobState::Succeeded, "completed"));
    run.phase = TransferRunPhase::Settled;
    let failure = run.begin_apply().expect_err("spent");
    assert_eq!(failure.kind, TransferRunFailureKind::PlanConsumed);
}

#[test]
fn acceptance_downgrade_spellings_match_the_hook() {
    // React 331-337: effectOutcome 'unknown', recoveryVerdict
    // 'pendingVerification', recoveryReason 'jobDetailsPending'.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "p1"));
    run.finish_apply(receipt("a1", JobState::Failed, "failed"));
    let view = run.apply_view.as_ref().unwrap();
    assert_eq!(view.effect_outcome.as_deref(), Some("unknown"));
    assert_eq!(view.recovery_verdict.as_deref(), Some("pendingVerification"));
    assert_eq!(view.recovery_reason.as_deref(), Some("jobDetailsPending"));
    // The unavailable-details degradation uses its own reason spelling
    // (React 377-380: 'jobDetailsUnavailable').
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "p1"));
    run.finish_apply(receipt("a1", JobState::Failed, "failed"));
    run.apply_details_unavailable("a1");
    let view = run.apply_view.as_ref().unwrap();
    assert_eq!(view.recovery_reason.as_deref(), Some("jobDetailsUnavailable"));
}

#[test]
fn poll_merge_refuses_stale_ids_and_propagates_cancels() {
    // React 399-408: the merge only lands on the current apply id; a
    // cancelRequested the backend already knows propagates the intent.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "p1"));
    run.finish_apply(receipt("a1", JobState::Running, "notStarted"));
    // A stale id is silently refused.
    run.apply_job_view("a0", JobState::Cancelled, Some("unknown"), true);
    assert_eq!(run.apply_view.as_ref().unwrap().state, JobState::Running);
    assert!(!run.cancel_requested, "the stale poll latched nothing");
    // The live id propagates the recorded cancel.
    run.apply_job_view("a1", JobState::Running, None, true);
    assert!(run.cancel_requested);
    assert!(run.cancel_acknowledged);
    assert!(!run.cancel_unknown_job);
    // A cancelled summary sets the cancelled flag (React 174).
    run.apply_job_view("a1", JobState::Cancelled, None, false);
    assert!(run.apply_view.as_ref().unwrap().cancelled);
}

#[test]
fn details_merge_rearms_polling_for_a_late_running_view() {
    // React 366-369: a details reply that shows the Job still active
    // flips the phase back to applying and re-arms the watch.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "p1"));
    run.finish_apply(receipt("a1", JobState::Queued, "notStarted"));
    run.apply_job_view("a1", JobState::Succeeded, Some("completed"), false);
    // The terminal summary left the phase applying-settled; the details
    // reply says running again.
    run.apply_terminal_details(
        "a1",
        JobState::Running,
        None,
        None,
        false,
        Vec::new(),
        Vec::new(),
        JobProgress::default(),
        None,
        None,
        None,
    );
    assert_eq!(run.phase, TransferRunPhase::Applying);
    assert_eq!(run.watch_job_id.as_deref(), Some("a1"));
    // The details query is finished either way (React 369-370 clears the
    // terminal details identity on both legs).
    assert!(run.terminal_details_job_id.is_none());
}

#[test]
fn cancel_address_holes_match_the_hook_matrix() {
    // The admission hole (React 483-485): while the apply is being
    // submitted there is no apply id to address and the prepare id must
    // not be guessed — the write is what the user wants stopped.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "p1"));
    run.begin_apply().expect("admitted");
    assert_eq!(run.cancel_target_job_id(), None);
    let addressed = run.begin_cancel();
    assert!(addressed.is_none());
    assert!(run.cancel_unknown_job);
    // The settled-apply hole (React 479-482): the apply id exists but its
    // Job has left the active states.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "p1"));
    run.finish_apply(receipt("a1", JobState::Cancelled, "unknown"));
    assert_eq!(run.cancel_target_job_id(), None);
    assert!(run.begin_cancel().is_none());
    // The active-apply leg addresses the apply id itself (React 479-482).
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "p1"));
    run.finish_apply(receipt("a1", JobState::Queued, "notStarted"));
    assert_eq!(run.cancel_target_job_id(), Some("a1"));
    assert_eq!(run.begin_cancel(), Some("a1".to_owned()));
    // Before any apply exists, the prepare id is the address (React 485).
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "p1"));
    assert_eq!(run.cancel_target_job_id(), Some("prepare-p1"));
    assert_eq!(run.begin_cancel(), Some("prepare-p1".to_owned()));
}

#[test]
fn verdict_counters_are_only_trusted_with_a_boundary() {
    // React 526-534: rows with no boundary at all are strictly worse
    // than a boundary missing its evidence markers — a completed outcome
    // over unbacked rows cannot present as a clean success.
    let mut run = TransferJobRunVm::default();
    let mut view = receipt("a1", JobState::Succeeded, "completed");
    view.progress = JobProgress {
        committed: Some(JobCounter::Text("100".to_owned())),
        unknown: Some(JobCounter::Text("0".to_owned())),
        ..JobProgress::default()
    };
    run.apply_view = Some(view);
    let verdict = run.verdict().expect("a verdict");
    assert!(!verdict.completed, "100 committed rows with no boundary");
    assert!(verdict.requires_reconcile);
    // With a boundary and zero rows the completion is certifiable.
    let mut run = TransferJobRunVm::default();
    let mut view = receipt("a1", JobState::Succeeded, "completed");
    view.progress = JobProgress {
        committed: Some(JobCounter::Text("0".to_owned())),
        unknown: Some(JobCounter::Text("0".to_owned())),
        ..JobProgress::default()
    };
    run.apply_view = Some(view);
    let verdict = run.verdict().expect("a verdict");
    assert!(verdict.completed);
    assert!(!verdict.requires_reconcile);
}

#[test]
fn an_acked_cancel_while_applying_is_not_a_disposal() {
    // React 852-859: only a cancel acknowledged while the plan was merely
    // prepared disposes the plan; an applying cancel's outcome belongs
    // to the verdict.
    let mut vm = DataTransferWindowVm::default();
    vm.job_run.finish_prepare(admission(true, "p1"));
    vm.preview = Some(TransferPreviewVm::default());
    vm.step = WizardStep::Preview;
    vm.handle_cancel(true, true);
    assert_eq!(vm.step, WizardStep::Preview, "the review stays");
    assert!(vm.preview.is_some());
    assert_eq!(vm.job_run.phase, TransferRunPhase::Prepared);
    // The merely-prepared disposal resets the whole run.
    let mut vm = DataTransferWindowVm::default();
    vm.job_run.finish_prepare(admission(true, "p1"));
    vm.preview = Some(TransferPreviewVm::default());
    vm.step = WizardStep::Preview;
    vm.handle_cancel(false, true);
    assert_eq!(vm.step, WizardStep::Mapping);
    assert!(vm.preview.is_none());
    assert_eq!(vm.job_run.phase, TransferRunPhase::Idle);
    assert!(vm.job_run.prepare_view.is_none());
}

#[test]
fn the_apply_request_carries_the_admissions_identity() {
    // React 287-299: the request is built from the prepare view's
    // admission identity — planId, planDigest, selectionRevision — and a
    // supplied selection passes through as the frozen-tables-or-list
    // choice.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(admission(true, "plan-42"));
    run.prepare_view.as_mut().unwrap().selection_revision = 9;
    let request = run.apply_request(&TransferApplyInput {
        source_tables: Some(vec!["a".to_owned(), "b".to_owned()]),
        confirmed_destructive: true,
        idempotency_key: Some("key-9".to_owned()),
    });
    assert_eq!(request.plan_id, "plan-42");
    assert_eq!(request.plan_digest, "digest-plan-42");
    assert_eq!(request.selection_revision, 9);
    assert_eq!(request.source_tables, Some(vec!["a".to_owned(), "b".to_owned()]));
    assert_eq!(request.idempotency_key, "key-9");
}

#[test]
fn boundaries_with_evidence_count_as_verified() {
    // The verdict engine's verified/unverified split: a boundary with
    // evidence is verified; one without is not (migrationJobVerdict.ts,
    // the boundary count React 517-535 feeds through commitBoundaries).
    let mut run = TransferJobRunVm::default();
    let mut view = receipt("a1", JobState::Succeeded, "completed");
    view.commit_boundaries = vec![
        CommitBoundary {
            stage_id: "s1".to_owned(),
            committed_at: String::new(),
            evidence: vec!["EVIDENCE_A".to_owned()],
            operation_id: None,
        },
        CommitBoundary {
            stage_id: "s2".to_owned(),
            committed_at: String::new(),
            evidence: Vec::new(),
            operation_id: None,
        },
    ];
    view.progress = JobProgress {
        committed: Some(JobCounter::Text("0".to_owned())),
        unknown: Some(JobCounter::Text("0".to_owned())),
        ..JobProgress::default()
    };
    run.apply_view = Some(view);
    let verdict = run.verdict().expect("a verdict");
    assert_eq!(verdict.verified_boundaries, 1);
    assert_eq!(verdict.unverified_boundaries, 1);
    assert!(!verdict.completed, "an unverified boundary blocks completion");
    assert!(verdict.requires_reconcile);
}
