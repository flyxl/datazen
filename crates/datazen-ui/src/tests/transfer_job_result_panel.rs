//! T246: the transfer job result panel's coder tests — the pure
//! derivations (the five-bucket progress line with its unreadable-is-
//! zero rule, the plan identity, the counter dialect bridge) and the
//! mounted section's three blocks (the in-flight card with its state /
//! progress / cancel lines, the refusal notice mount with the re-review
//! wire, the settled verdict panel mount), plus the hidden attrs
//! mirror's six attribute pairs.

use rune::{col, Cx, El, Env, Runtime};

use crate::views::data_transfer::data_transfer_attached_jobs::{
    JobCounter, JobProgress, JobState,
};
use crate::views::data_transfer::transfer_job_result_panel::{
    active_progress, plan_info, TransferJobResultPanel,
};
use crate::views::data_transfer::transfer_job_run::{
    TransferApplyJobViewVm, TransferJobRunVm, TransferRunFailure, TransferRunFailureKind,
    TransferRunPhase,
};
use crate::i18n::t;

// ---------------------------------------------------------------------------
// The probe
// ---------------------------------------------------------------------------

struct Probe {
    run: TransferJobRunVm,
    dark: bool,
    reviews: u32,
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    col()
        .key("probe-root")
        .w_full()
        .child(TransferJobResultPanel::<Probe>::new(&s.run)
            .on_re_review(|s| s.reviews += 1)
            .build(s.dark))
}

fn runtime(p: Probe, size: (f32, f32)) -> Runtime<Probe> {
    let dark = p.dark;
    let mut rt = Runtime::new(p, view, size, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn has<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text.clone())
        .unwrap_or_default()
}

fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.label.clone())
        .unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out before click"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

/// An apply view with the given state and five progress buckets.
fn apply_view(state: JobState, buckets: [Option<JobCounter>; 5]) -> TransferApplyJobViewVm {
    let [read, converted, attempted, committed, unknown] = buckets;
    TransferApplyJobViewVm {
        job_id: "job-apply-9".to_owned(),
        kind: "dataTransferApply".to_owned(),
        state,
        effect_outcome: None,
        progress: JobProgress { read, converted, attempted, committed, unknown },
        plan_id: "plan-42".to_owned(),
        plan_digest: "digest-42".to_owned(),
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

fn text_counter(value: &str) -> Option<JobCounter> {
    Some(JobCounter::Text(value.to_owned()))
}

// ---------------------------------------------------------------------------
// The progress line (React 39–49)
// ---------------------------------------------------------------------------

#[test]
fn the_progress_line_joins_the_five_buckets_in_react_order() {
    let apply = apply_view(JobState::Running, [
        text_counter("12"),
        text_counter("10"),
        text_counter("9"),
        text_counter("8"),
        text_counter("1"),
    ]);
    assert_eq!(active_progress(&apply).as_deref(), Some("12 / 10 / 9 / 8 / 1"));
}

#[test]
fn an_unreadable_bucket_is_zero_in_the_line_but_present() {
    // A missing bucket renders as 0 (the line's own rule — the verdict
    // engine is stricter); a JSON number truncates as readJobCounter
    // does.
    let apply = apply_view(JobState::Running, [
        text_counter("12"),
        None,
        Some(JobCounter::Number(9.7)),
        text_counter("0"),
        text_counter("not-a-number"),
    ]);
    assert_eq!(active_progress(&apply).as_deref(), Some("12 / 0 / 9 / 0 / 0"));
}

#[test]
fn the_plan_identity_names_the_spent_plan() {
    let apply = apply_view(JobState::Succeeded, Default::default());
    let plan = plan_info(Some(&apply));
    assert_eq!(plan.plan_id.as_deref(), Some("plan-42"));
    assert_eq!(plan.plan_digest.as_deref(), Some("digest-42"));
    assert_eq!(plan.selection_revision, Some(7));
    assert_eq!(plan_info(None), crate::components::migration::migration_job_verdict_panel::PlanInfo::default());
}

// ---------------------------------------------------------------------------
// The mounted section
// ---------------------------------------------------------------------------

#[test]
fn an_idle_run_mounts_only_the_root_and_the_mirror() {
    let rt = runtime(
        Probe { run: TransferJobRunVm::default(), dark: false, reviews: 0 },
        (900.0, 700.0),
    );
    assert!(has(&rt, "data-transfer-result"));
    assert!(has(&rt, "data-transfer-result-attrs"));
    // None of the three blocks: no apply, no failure, no verdict.
    assert!(!has(&rt, "data-transfer-result-active-card"));
    assert!(!has(&rt, "migration-job-failure"));
    assert!(!has(&rt, "migration-job-verdict"));
}

#[test]
fn the_in_flight_card_mounts_the_state_the_progress_and_the_cancel() {
    let mut run = TransferJobRunVm::default();
    run.phase = TransferRunPhase::Applying;
    run.apply_view = Some(apply_view(JobState::Running, [
        text_counter("12"),
        text_counter("10"),
        text_counter("9"),
        text_counter("8"),
        text_counter("1"),
    ]));
    run.cancel_requested = true;

    let mut rt = runtime(Probe { run, dark: false, reviews: 0 }, (900.0, 700.0));
    assert!(has(&rt, "data-transfer-result-active-card"));
    assert_eq!(
        text_of(&rt, "data-transfer-job-active-state"),
        t("transfer.job.running")
    );
    assert_eq!(
        label_of(&rt, "data-transfer-job-active-state"),
        "job_id=job-apply-9|state=running"
    );
    assert_eq!(
        text_of(&rt, "data-transfer-job-active-progress"),
        format!("{}: 12 / 10 / 9 / 8 / 1", t("migration.progress.counts"))
    );
    assert_eq!(
        text_of(&rt, "data-transfer-job-active-cancel"),
        t("migration.cancel.requestedInFlight")
    );
    // In flight: the verdict panel does not mount even with a verdict.
    assert!(!has(&rt, "migration-job-verdict"));
}

#[test]
fn the_cancel_line_is_conditional_on_the_run() {
    let mut run = TransferJobRunVm::default();
    run.phase = TransferRunPhase::Applying;
    run.apply_view = Some(apply_view(JobState::Running, Default::default()));
    run.cancel_requested = false;

    let rt = runtime(Probe { run, dark: false, reviews: 0 }, (900.0, 700.0));
    assert!(has(&rt, "data-transfer-result-active-card"));
    assert!(!has(&rt, "data-transfer-job-active-cancel"));
}

#[test]
fn a_refused_run_mounts_the_notice_with_the_re_review_wire() {
    let mut run = TransferJobRunVm::default();
    run.failure = Some(TransferRunFailure {
        kind: TransferRunFailureKind::PlanConsumed,
        message: "PlanAlreadyConsumed: re-review the migration".to_owned(),
    });

    let mut rt = runtime(Probe { run, dark: false, reviews: 0 }, (900.0, 700.0));
    assert!(has(&rt, "migration-job-failure"));
    assert_eq!(
        text_of(&rt, "migration-job-failure-title"),
        t("migration.failure.planConsumed.title")
    );
    // The re-review button mounts for planConsumed with a handler.
    assert!(has(&rt, "migration-job-re-review"));
    click(&mut rt, "migration-job-re-review");
    assert_eq!(rt.state.reviews, 1, "the press reaches the panel's callback");
}

#[test]
fn a_failed_apply_mounts_the_verdict_panel_beside_the_card_free_root() {
    // A settled failed apply: a verdict exists, the run is not in
    // flight, so the verdict panel mounts and the card does not.
    let mut run = TransferJobRunVm::default();
    run.apply_view = Some(apply_view(JobState::Failed, Default::default()));
    let mut rt = runtime(Probe { run, dark: false, reviews: 0 }, (900.0, 700.0));
    assert!(has(&rt, "migration-job-verdict"), "the settled verdict mounts");
    assert!(!has(&rt, "data-transfer-result-active-card"), "the run is not in flight");

    // The attrs mirror carries the settled severity and the plan id.
    let label = label_of(&rt, "data-transfer-result-attrs");
    assert!(label.contains("verdict_severity=failed"), "{label}");
    assert!(label.contains("plan_id=plan-42"), "{label}");
    assert!(label.contains("completed=false"), "{label}");
    assert!(label.contains("cancel_disposition=none"), "{label}");
}

#[test]
fn the_attrs_mirror_says_active_while_in_flight() {
    let mut run = TransferJobRunVm::default();
    run.phase = TransferRunPhase::Applying;
    run.apply_view = Some(apply_view(JobState::Running, Default::default()));
    let rt = runtime(Probe { run, dark: false, reviews: 0 }, (900.0, 700.0));
    let label = label_of(&rt, "data-transfer-result-attrs");
    assert!(label.contains("verdict_severity=active"), "{label}");
    assert!(label.contains("replayed=false"), "{label}");
    assert!(label.contains("requires_reconcile="), "{label}");
}

#[test]
fn a_succeeded_apply_without_boundaries_mounts_an_uncertain_verdict() {
    // The data-transfer handler records no commit boundaries, so even a
    // `succeeded` apply cannot prove its write (React 526–534): the
    // verdict stays `uncertain` and requires a reconcile, in both
    // palettes.
    for dark in [false, true] {
        let mut run = TransferJobRunVm::default();
        run.apply_view = Some(apply_view(JobState::Succeeded, [
            text_counter("5"),
            text_counter("5"),
            text_counter("5"),
            text_counter("5"),
            text_counter("0"),
        ]));
        let rt = runtime(Probe { run, dark, reviews: 0 }, (900.0, 700.0));
        assert!(has(&rt, "migration-job-verdict"), "the verdict mounts, dark={dark}");
        let label = label_of(&rt, "data-transfer-result-attrs");
        assert!(
            label.contains("verdict_severity=uncertain") && label.contains("requires_reconcile=true"),
            "{label}, dark={dark}"
        );
    }
}
