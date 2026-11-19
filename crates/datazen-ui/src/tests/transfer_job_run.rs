//! T147-S2: the Data Transfer Job run machine — coder tests.
//!
//! Every assertion is checked against
//! `packages/frontend/src/hooks/useTransferJobRun.ts` (and the lib
//! predicates it folds); the React line numbers are cited per section.
//! Pure-logic tests drive the [`TransferJobRunVm`] and the window Vm's
//! run wiring directly.

use crate::components::migration::migration_job_verdict_panel::CommitBoundary;
use crate::views::{JobCounter, JobProgress, JobState};
use crate::views::{
    TransferApplyInput, TransferApplyJobViewVm, TransferJobRunVm, TransferPrepareJobViewVm,
    TransferPreviewVm, TransferRunFailureKind, TransferRunPhase, DataTransferWindowVm,
    WizardStep,
};

// ---------------------------------------------------------------------------
// Builders.
// ---------------------------------------------------------------------------

fn prepare_view(can_execute: bool) -> TransferPrepareJobViewVm {
    TransferPrepareJobViewVm {
        job_id: "job-prepare-1".to_owned(),
        kind: "dataTransferPrepare".to_owned(),
        state: JobState::Succeeded,
        effect_outcome: Some("completed".to_owned()),
        plan_id: "plan-1".to_owned(),
        plan_digest: "digest-1".to_owned(),
        plan_version: 1,
        handler_version: 1,
        checkpoint_version: 1,
        selection_revision: 7,
        expires_at: "2026-12-01T00:00:00.000Z".to_owned(),
        can_execute,
        block_reason: None,
        review: TransferPreviewVm::default(),
    }
}

fn apply_view(state: JobState, effect_outcome: Option<&str>) -> TransferApplyJobViewVm {
    TransferApplyJobViewVm {
        job_id: "job-apply-1".to_owned(),
        kind: "dataTransferApply".to_owned(),
        state,
        effect_outcome: effect_outcome.map(str::to_owned),
        progress: JobProgress::default(),
        plan_id: "plan-1".to_owned(),
        plan_digest: "digest-1".to_owned(),
        selection_revision: 7,
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

// ---------------------------------------------------------------------------
// The failure classification (React 64-141 + the lib predicates).
// ---------------------------------------------------------------------------

#[test]
fn classify_covers_every_kind() {
    // backendScope (lib/migrationJobVerdict.ts:355-358).
    for message in [
        "backend scope mismatch",
        "not local-desktop-backend",
        "invalid Scope token",
    ] {
        let failure = crate::views::classify_transfer_run_failure(message);
        assert_eq!(failure.kind, TransferRunFailureKind::BackendScope, "{message}");
        assert_eq!(failure.message, message);
    }
    // pipelineBudget (lib/migrationJobVerdict.ts:333-336).
    for message in [
        "pipeline budget exceeded",
        "PipelineBudget refusal",
        "budget exceed limit",
        "payload over 8 MiB",
        "8388608 bytes",
    ] {
        let failure = crate::views::classify_transfer_run_failure(message);
        assert_eq!(failure.kind, TransferRunFailureKind::PipelineBudget, "{message}");
    }
    // planConsumed (lib/migrationJobVerdict.ts:343-348).
    for message in [
        "planId already consumed",
        "PlanAlreadyConsumed",
        "re-review the migration to mint a new plan",
    ] {
        let failure = crate::views::classify_transfer_run_failure(message);
        assert_eq!(failure.kind, TransferRunFailureKind::PlanConsumed, "{message}");
    }
    // stalePlan (migrationJobHydration.ts:114-149).
    for message in ["PlanStale: source changed", "SourceChanged", "PermissionDenied"] {
        let failure = crate::views::classify_transfer_run_failure(message);
        assert_eq!(failure.kind, TransferRunFailureKind::StalePlan, "{message}");
    }
    // other.
    let failure = crate::views::classify_transfer_run_failure("connection reset by peer");
    assert_eq!(failure.kind, TransferRunFailureKind::Other);
}

#[test]
fn ambiguous_submission_errors_match_the_react_regex() {
    // React 143-154: /timed?\s*out|timeout|transport|connection
    // (closed|lost)|ack…|outcome unknown/i.
    for message in [
        "request timed out",
        "timeout while submitting",
        "transport closed",
        "connection closed",
        "connection lost",
        "acknowledgement lost",
        "outcome unknown",
    ] {
        assert!(
            crate::views::is_ambiguous_apply_submission_error(message),
            "{message}"
        );
    }
    assert!(!crate::views::is_ambiguous_apply_submission_error("scope"));
    assert!(!crate::views::is_ambiguous_apply_submission_error("plan consumed"));
}

// ---------------------------------------------------------------------------
// Prepare (React 239-267).
// ---------------------------------------------------------------------------

#[test]
fn prepare_bracket_publishes_the_admission() {
    // React 241-246: the opening bracket clears the residue.
    let mut run = TransferJobRunVm::default();
    run.fail_prepare("backend scope mismatch");
    run.cancel_requested = true;
    run.cancel_acknowledged = true;
    run.apply_view = Some(apply_view(JobState::Succeeded, Some("completed")));
    run.begin_prepare();
    assert_eq!(run.phase, TransferRunPhase::Preparing);
    assert!(run.failure.is_none());
    assert!(run.apply_view.is_none());
    assert!(!run.cancel_requested);
    assert!(!run.cancel_acknowledged);
    // React 248-258: canExecute decides the phase.
    run.finish_prepare(prepare_view(true));
    assert_eq!(run.phase, TransferRunPhase::Prepared);
    assert_eq!(run.prepare_view.as_ref().unwrap().job_id, "job-prepare-1");
    // The prepare id becomes the cancel address before apply exists
    // (React 537-543).
    assert_eq!(run.cancel_target_job_id(), Some("job-prepare-1"));
    // A blocked review keeps the phase blocked (React 257).
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(false));
    assert_eq!(run.phase, TransferRunPhase::Blocked);
}

#[test]
fn prepare_refusal_classifies_and_reads_same_tick() {
    // React 259-264 + lastFailure (React 233).
    let mut run = TransferJobRunVm::default();
    run.fail_prepare("backend scope mismatch");
    assert_eq!(run.phase, TransferRunPhase::Blocked);
    assert!(run.prepare_view.is_none());
    let failure = run.last_failure().expect("the same-tick mirror");
    assert_eq!(failure.kind, TransferRunFailureKind::BackendScope);
    assert_eq!(failure.message, "backend scope mismatch");
    assert_eq!(run.failure.as_ref().unwrap().message, "backend scope mismatch");
}

// ---------------------------------------------------------------------------
// Apply (React 269-344).
// ---------------------------------------------------------------------------

#[test]
fn apply_guards_refuse_without_a_plan_and_after_a_spend() {
    // React 271-273: apply requires an admitted plan.
    let mut run = TransferJobRunVm::default();
    let failure = run.begin_apply().expect_err("no plan");
    assert_eq!(failure.kind, TransferRunFailureKind::Other);
    assert_eq!(failure.message, "apply requires an admitted plan");
    // React 278-284: the planId was already spent — the local refusal
    // keeps planConsumed a first-class outcome.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.finish_apply(apply_view(JobState::Succeeded, Some("completed")));
    run.phase = TransferRunPhase::Settled;
    let failure = run.begin_apply().expect_err("plan spent");
    assert_eq!(failure.kind, TransferRunFailureKind::PlanConsumed);
    assert_eq!(
        failure.message,
        "this planId was already applied; review a new plan"
    );
    // A replayed receipt is not a fresh spend: re-apply is allowed (React
    // 278's `!applyView.replayed` guard).
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    let mut replayed = apply_view(JobState::Succeeded, Some("completed"));
    replayed.replayed = true;
    run.finish_apply(replayed);
    run.phase = TransferRunPhase::Settled;
    assert!(run.begin_apply().is_ok());
}

#[test]
fn apply_request_scopes_the_idempotency_key_to_the_plan() {
    // React 287-299: the caller's key wins; otherwise the planId scopes it.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    let request = run.apply_request(&TransferApplyInput {
        source_tables: Some(vec!["t1".to_owned()]),
        confirmed_destructive: true,
        idempotency_key: None,
    });
    assert_eq!(request.plan_id, "plan-1");
    assert_eq!(request.plan_digest, "digest-1");
    assert_eq!(request.selection_revision, 7);
    assert_eq!(request.source_tables, Some(vec!["t1".to_owned()]));
    assert!(request.confirmed_destructive);
    assert_eq!(request.idempotency_key, "data-transfer/apply/plan-1");
    // An explicit key is passed through unchanged (React 297-299).
    let request = run.apply_request(&TransferApplyInput {
        source_tables: None,
        confirmed_destructive: false,
        idempotency_key: Some("explicit-key".to_owned()),
    });
    assert_eq!(request.idempotency_key, "explicit-key");
    assert!(request.source_tables.is_none(), "absent means every table");
}

#[test]
fn apply_acceptance_publishes_the_id_and_arms_the_right_effect() {
    // React 323-341.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.begin_apply().expect("admitted");
    // An active receipt keeps applying and arms polling (React 327-340).
    run.finish_apply(apply_view(JobState::Queued, Some("notStarted")));
    assert_eq!(run.phase, TransferRunPhase::Applying);
    assert_eq!(run.apply_view.as_ref().unwrap().job_id, "job-apply-1");
    assert_eq!(run.watch_job_id.as_deref(), Some("job-apply-1"));
    assert!(run.terminal_details_job_id.is_none());
    // The cancel address is the apply id while it is active (React 537-540).
    assert_eq!(run.cancel_target_job_id(), Some("job-apply-1"));
    assert!(run.is_in_flight());
    // A terminal receipt is downgraded until the details query returns
    // (React 331-337, 340).
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.begin_apply().expect("admitted");
    run.finish_apply(apply_view(JobState::Succeeded, Some("completed")));
    assert_eq!(run.phase, TransferRunPhase::Settled);
    let view = run.apply_view.as_ref().unwrap();
    assert_eq!(view.effect_outcome.as_deref(), Some("unknown"));
    assert_eq!(view.recovery_verdict.as_deref(), Some("pendingVerification"));
    assert_eq!(view.recovery_reason.as_deref(), Some("jobDetailsPending"));
    assert_eq!(run.terminal_details_job_id.as_deref(), Some("job-apply-1"));
    assert!(run.watch_job_id.is_none());
    // A settled apply is no longer a cancel address (React 537-540).
    assert_eq!(run.cancel_target_job_id(), None);
    assert!(!run.is_in_flight());
}

#[test]
fn apply_refusal_classifies_and_blocks() {
    // React 309-320.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.begin_apply().expect("admitted");
    run.fail_apply("PlanAlreadyConsumed");
    assert_eq!(run.phase, TransferRunPhase::Blocked);
    let failure = run.last_failure().expect("classified");
    assert_eq!(failure.kind, TransferRunFailureKind::PlanConsumed);
}

// ---------------------------------------------------------------------------
// The poll / details projections (React 156-202).
// ---------------------------------------------------------------------------

#[test]
fn job_view_merge_keeps_terminal_summaries_uncertain() {
    // React 161-175 + the identity guard (React 400).
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.finish_apply(apply_view(JobState::Running, Some("notStarted")));
    // An active poll keeps a supplied outcome.
    run.apply_job_view("job-apply-1", JobState::Running, Some("partiallyApplied"), false);
    let view = run.apply_view.as_ref().unwrap();
    assert_eq!(view.effect_outcome.as_deref(), Some("partiallyApplied"));
    assert!(view.partial, "partiallyApplied sets partial");
    // A cancel the backend knows propagates the intent (React 404-408).
    run.apply_job_view("job-apply-1", JobState::Running, None, true);
    assert!(run.cancel_requested);
    assert!(run.cancel_acknowledged);
    assert!(!run.cancel_unknown_job);
    // A terminal summary downgrades the outcome (React 167-169).
    run.apply_job_view("job-apply-1", JobState::Succeeded, Some("completed"), false);
    let view = run.apply_view.as_ref().unwrap();
    assert_eq!(view.state, JobState::Succeeded);
    assert_eq!(view.effect_outcome.as_deref(), Some("unknown"), "not the durable report");
    assert!(view.cancelled == (view.state == JobState::Cancelled));
    // A stale id is refused (React 400's applyJobIdRef guard).
    let before = run.apply_view.clone().unwrap();
    run.apply_job_view("job-other", JobState::Failed, Some("rolledBack"), false);
    assert_eq!(run.apply_view.as_ref().unwrap().state, before.state);
}

#[test]
fn terminal_details_merge_settles_or_rearms() {
    // React 178-202.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.finish_apply(apply_view(JobState::Succeeded, Some("completed")));
    // The durable report merges boundaries, recovery, error (React 178-202).
    run.apply_terminal_details(
        "job-apply-1",
        JobState::Succeeded,
        Some("completed"),
        None,
        false,
        vec![CommitBoundary {
            stage_id: "stage-1".to_owned(),
            committed_at: "2026-12-01T00:00:00.000Z".to_owned(),
            evidence: vec!["EVIDENCE_COMMIT".to_owned()],
            operation_id: Some("op-1".to_owned()),
        }],
        vec!["transfer-sql-abc".to_owned()],
        JobProgress {
            committed: Some(JobCounter::Text("42".to_owned())),
            ..JobProgress::default()
        },
        Some("notExecuted"),
        None,
        Some("noCommittedWrites"),
    );
    let view = run.apply_view.as_ref().unwrap();
    assert_eq!(view.effect_outcome.as_deref(), Some("completed"));
    assert_eq!(view.commit_boundaries.len(), 1);
    assert_eq!(view.artifact_ids, vec!["transfer-sql-abc".to_owned()]);
    assert_eq!(
        view.progress.committed,
        Some(JobCounter::Text("42".to_owned()))
    );
    assert_eq!(view.recovery_verdict.as_deref(), Some("notExecuted"));
    assert_eq!(view.recovery_reason.as_deref(), Some("noCommittedWrites"));
    assert_eq!(run.phase, TransferRunPhase::Settled);
    assert!(run.watch_job_id.is_none());
    assert!(run.terminal_details_job_id.is_none(), "the query finished");
    // A details view that shows the Job still active re-arms polling
    // (React 366-369).
    run.apply_terminal_details(
        "job-apply-1",
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
    assert_eq!(run.watch_job_id.as_deref(), Some("job-apply-1"));
}

#[test]
fn unavailable_details_stay_uncertain() {
    // React 371-385 / 429-443.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.finish_apply(apply_view(JobState::Succeeded, Some("completed")));
    run.apply_details_unavailable("job-apply-1");
    let view = run.apply_view.as_ref().unwrap();
    assert_eq!(view.effect_outcome.as_deref(), Some("unknown"));
    assert_eq!(view.recovery_verdict.as_deref(), Some("pendingVerification"));
    assert_eq!(view.recovery_reason.as_deref(), Some("jobDetailsUnavailable"));
}

// ---------------------------------------------------------------------------
// Cancel (React 473-501).
// ---------------------------------------------------------------------------

#[test]
fn cancel_addressing_matrix() {
    // The full matrix of React 473-490.
    // Idle with a prepare: the prepare id is addressable (React 485).
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    assert_eq!(run.begin_cancel(), Some("job-prepare-1".to_owned()));
    assert!(run.cancel_requested, "the click latched the intent");
    assert!(!run.cancel_unknown_job);
    // Apply admission in flight: no id exists to guess (React 483-484).
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.begin_apply().expect("admitted");
    assert_eq!(run.begin_cancel(), None);
    assert!(run.cancel_unknown_job);
    assert!(!run.cancel_acknowledged);
    // An active apply: its own id (React 479-482).
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.finish_apply(apply_view(JobState::Running, Some("notStarted")));
    assert_eq!(run.begin_cancel(), Some("job-apply-1".to_owned()));
    // A finished apply: nothing may be guessed (React 479-482: the
    // active-state guard).
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.finish_apply(apply_view(JobState::Succeeded, Some("completed")));
    assert_eq!(run.begin_cancel(), None);
    assert!(run.cancel_unknown_job);
    // Nothing at all (React 486-489).
    let mut run = TransferJobRunVm::default();
    assert_eq!(run.begin_cancel(), None);
    assert!(run.cancel_unknown_job);
    assert!(!run.cancel_acknowledged);
}

#[test]
fn cancel_answers_record_the_disposition() {
    // React 491-495: true only when sent and answered true.
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.begin_cancel();
    run.finish_cancel(true);
    assert!(run.cancel_acknowledged);
    assert!(!run.cancel_unknown_job);
    // A refusal records the unknown disposition.
    run.finish_cancel(false);
    assert!(!run.cancel_acknowledged);
    assert!(run.cancel_unknown_job, "never folded into a success");
    // The command's own failure (React 496-500).
    run.fail_cancel("scope refused");
    assert!(run.cancel_unknown_job);
    assert_eq!(
        run.last_failure().map(|f| f.kind),
        Some(TransferRunFailureKind::BackendScope)
    );
}

// ---------------------------------------------------------------------------
// Reset (React 503-515).
// ---------------------------------------------------------------------------

#[test]
fn reset_and_reprepare_keep_nothing() {
    let mut run = TransferJobRunVm::default();
    run.finish_prepare(prepare_view(true));
    run.finish_apply(apply_view(JobState::Running, Some("notStarted")));
    run.begin_cancel();
    run.fail_prepare("x"); // arms the failure slot
    run.reset();
    assert_eq!(run.phase, TransferRunPhase::Idle);
    assert!(run.prepare_view.is_none());
    assert!(run.apply_view.is_none());
    assert!(run.failure.is_none());
    assert!(run.last_failure().is_none());
    assert!(!run.cancel_requested);
    assert!(!run.cancel_acknowledged);
    assert!(!run.cancel_unknown_job);
    assert!(run.watch_job_id.is_none());
    assert!(run.terminal_details_job_id.is_none());
    assert_eq!(run.cancel_target_job_id(), None);
    // React 561-562: reprepare is the same function.
    run.finish_prepare(prepare_view(true));
    run.reprepare();
    assert_eq!(run.phase, TransferRunPhase::Idle);
}

// ---------------------------------------------------------------------------
// The verdict fold (React 517-535).
// ---------------------------------------------------------------------------

#[test]
fn verdict_folds_only_from_the_apply_view() {
    // No apply view -> no verdict (React 518-519).
    let run = TransferJobRunVm::default();
    assert!(run.verdict().is_none());

    // A clean completed apply with zero counters: completed, no reconcile.
    let mut run = TransferJobRunVm::default();
    let mut view = apply_view(JobState::Succeeded, Some("completed"));
    view.progress = JobProgress {
        committed: Some(JobCounter::Text("0".to_owned())),
        unknown: Some(JobCounter::Text("0".to_owned())),
        ..JobProgress::default()
    };
    run.apply_view = Some(view);
    let verdict = run.verdict().expect("a settled verdict");
    assert!(verdict.completed);
    assert!(!verdict.requires_reconcile);

    // An unknown outcome is uncertain: a re-review is pending (the
    // migrationJobVerdict engine, React 517-535).
    let mut run = TransferJobRunVm::default();
    run.apply_view = Some(apply_view(JobState::Succeeded, Some("unknown")));
    let verdict = run.verdict().expect("a verdict");
    assert!(!verdict.completed);
    assert!(verdict.requires_reconcile);

    // An in-flight apply keeps the affordance shut.
    let mut run = TransferJobRunVm::default();
    run.apply_view = Some(apply_view(JobState::Running, Some("notStarted")));
    let verdict = run.verdict().expect("a verdict");
    assert!(verdict.requires_reconcile);
}

// ---------------------------------------------------------------------------
// The window's run wiring (React 773-877).
// ---------------------------------------------------------------------------

fn window_with_run() -> DataTransferWindowVm {
    let mut vm = DataTransferWindowVm::default();
    vm.job_run.finish_prepare(prepare_view(true));
    vm.preview = Some(TransferPreviewVm {
        can_execute: true,
        ..TransferPreviewVm::default()
    });
    vm.step = WizardStep::Preview;
    vm
}

#[test]
fn reconcile_pending_follows_the_verdict() {
    // React 933.
    let vm = DataTransferWindowVm::default();
    assert!(!vm.reconcile_pending(), "no verdict: affordance open");
    let mut vm = window_with_run();
    // A settled apply with an unknown outcome: reconcile pending.
    vm.job_run.apply_view = Some(apply_view(JobState::Succeeded, Some("unknown")));
    assert!(vm.reconcile_pending());
}

#[test]
fn handle_cancel_drops_a_disposed_plan_back_to_mapping() {
    // React 847-865.
    let mut vm = window_with_run();
    // A cancel acknowledged while merely prepared: the plan is disposed.
    vm.handle_cancel(false, true);
    assert_eq!(vm.step, WizardStep::Mapping);
    assert!(vm.preview.is_none());
    assert_eq!(vm.job_run.phase, TransferRunPhase::Idle);
    assert!(vm.execute_progress.is_empty());
    // A cancel that could not be addressed changes nothing (React 862-865).
    let mut vm = window_with_run();
    vm.handle_cancel(false, false);
    assert_eq!(vm.step, WizardStep::Preview);
    assert!(vm.preview.is_some());
    assert_eq!(vm.job_run.phase, TransferRunPhase::Prepared);
    // A cancel acknowledged while applying is NOT a disposal (React
    // 852-859: the verdict owns the outcome).
    let mut vm = window_with_run();
    vm.handle_cancel(true, true);
    assert_eq!(vm.step, WizardStep::Preview);
    assert!(vm.preview.is_some());
}

#[test]
fn execute_brackets_arm_the_progress_line_and_the_step() {
    // React 793-819.
    let mut vm = window_with_run();
    vm.begin_execute(3);
    assert!(vm.executing);
    assert!(!vm.execute_progress.is_empty(), "the count line is armed");
    assert!(vm.execute_progress.contains('3'));
    // A settled apply steps to result (React 815).
    vm.finish_execute(true);
    assert!(!vm.executing);
    assert!(vm.execute_progress.is_empty());
    assert_eq!(vm.step, WizardStep::Result);
    // A refusal stays on preview so the fail-closed notices show (React
    // 813-815).
    let mut vm = window_with_run();
    vm.begin_execute(1);
    vm.finish_execute(false);
    assert_eq!(vm.step, WizardStep::Preview);
}

#[test]
fn go_next_prepare_brackets_disable_the_editor_for_the_round_trip() {
    // React 942-953.
    let mut vm = window_with_run();
    vm.step = WizardStep::Mapping;
    vm.begin_go_next_prepare();
    assert!(vm.preparing);
    vm.finish_go_next_prepare();
    assert!(!vm.preparing);
    // The prepare refusal steps to preview with the error surface there
    // (React 944-947).
    vm.begin_go_next_prepare();
    vm.go_next_prepare_refused();
    assert!(!vm.preparing);
    assert_eq!(vm.step, WizardStep::Preview);
}

#[test]
fn profile_load_resets_the_run_with_the_plan() {
    // React 550-552.
    let mut vm = window_with_run();
    vm.profile_run_reset();
    assert_eq!(vm.job_run.phase, TransferRunPhase::Idle);
}

#[test]
fn footer_gates_follow_the_react_predicates() {
    // React 1760 (back: stepIndex === 0 || executing), 1851-1852 (next:
    // !canNext || loading).
    let mut vm = DataTransferWindowVm::default();
    assert!(vm.back_disabled(), "endpoints: index 0");
    vm.step = WizardStep::Setup;
    assert!(!vm.back_disabled());
    vm.executing = true;
    assert!(vm.back_disabled());
    vm.executing = false;
    // next: the setup gate passes by default, so next is enabled until
    // loading.
    assert!(!vm.next_disabled());
    vm.loading = true;
    assert!(vm.next_disabled());
    // The run phase the footer keys off (React 1767-1807).
    assert_eq!(vm.run_phase(), TransferRunPhase::Idle);
    vm.job_run.finish_prepare(prepare_view(true));
    assert_eq!(vm.run_phase(), TransferRunPhase::Prepared);
}
