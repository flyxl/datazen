//! T246 independent audit — fresh vectors against the transfer job
//! result panel port, none shared with the coder tests: the counter
//! dialect bridge's fractional / negative / oversized forms, the
//! in-flight card's zh-CN exact strings and its label wire, the attrs
//! mirror's stalePlan severity spelling, a boundary-backed succeeded
//! apply (the `ok` verdict the coder vectors never reach), the cancel
//! disposition propagation from a cancel-acknowledged run, and the
//! unknown-prefix probe through the mounted tree.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::migration::migration_job_verdict_panel::CommitBoundary;
use crate::i18n::t;
use crate::views::data_transfer::data_transfer_attached_jobs::{
    JobCounter, JobProgress, JobState,
};
use crate::views::data_transfer::transfer_job_result_panel::{
    active_progress, TransferJobResultPanel,
};
use crate::views::data_transfer::transfer_job_run::{
    TransferApplyJobViewVm, TransferJobRunVm, TransferRunFailure, TransferRunFailureKind,
    TransferRunPhase,
};

fn text_counter(value: &str) -> Option<JobCounter> {
    Some(JobCounter::Text(value.to_owned()))
}

struct Scene {
    run: TransferJobRunVm,
    dark: bool,
    reviews: u32,
}

fn view(s: &Scene, _cx: &mut Cx<Scene>) -> El {
    col()
        .key("probe-root")
        .w_full()
        .child(TransferJobResultPanel::<Scene>::new(&s.run)
            .on_re_review(|s| s.reviews += 1)
            .test_id_prefix("audit-transfer")
            .build(s.dark))
}

fn runtime(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, Env { system_dark: dark, scale: 1.0 });
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

// ---------------------------------------------------------------------------
// The counter dialect bridge (fresh vectors)
// ---------------------------------------------------------------------------

#[test]
fn fractional_and_negative_numbers_truncate_or_vanish_in_both_surfaces() {
    let view = |read: Option<JobCounter>| {
        let mut apply = TransferApplyJobViewVm {
            job_id: "job-a".to_owned(),
            kind: "dataTransferApply".to_owned(),
            state: JobState::Running,
            effect_outcome: None,
            progress: JobProgress {
                read,
                converted: None,
                attempted: None,
                committed: None,
                unknown: None,
            },
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
        };
        apply.state = JobState::Running;
        apply
    };
    // 9.9 truncates to 9 (Math.trunc); -3 is refused and renders 0.
    assert_eq!(active_progress(&view(Some(JobCounter::Number(9.9)))).as_deref(), Some("9 / 0 / 0 / 0 / 0"));
    assert_eq!(active_progress(&view(Some(JobCounter::Number(-3.0)))).as_deref(), Some("0 / 0 / 0 / 0 / 0"));
    // A padded text counter keeps its own digits (trim then digits).
    assert_eq!(active_progress(&view(text_counter("  17\t"))).as_deref(), Some("17 / 0 / 0 / 0 / 0"));
}

// ---------------------------------------------------------------------------
// The mounted section (fresh vectors)
// ---------------------------------------------------------------------------

#[test]
fn the_in_flight_card_carries_the_zh_cn_pack_and_the_label_wire() {
    let mut run = TransferJobRunVm::default();
    run.phase = TransferRunPhase::Applying;
    run.apply_view = Some(TransferApplyJobViewVm {
        job_id: "job-apply-77".to_owned(),
        kind: "dataTransferApply".to_owned(),
        state: JobState::Queued,
        effect_outcome: None,
        progress: JobProgress {
            read: text_counter("100"),
            converted: text_counter("80"),
            attempted: text_counter("60"),
            committed: text_counter("40"),
            unknown: text_counter("2"),
        },
        plan_id: "plan-77".to_owned(),
        plan_digest: "digest-77".to_owned(),
        selection_revision: 3,
        commit_boundaries: Vec::new(),
        artifact_ids: Vec::new(),
        cancelled: false,
        partial: false,
        replayed: true,
        error: None,
        recovery_verdict: None,
        recovery_resume_through: None,
        recovery_reason: None,
    });
    let rt = runtime(Scene { run, dark: true, reviews: 0 }, (960.0, 720.0));
    assert_eq!(text_of(&rt, "data-transfer-job-active-state"), "传输已排队");
    assert_eq!(
        text_of(&rt, "data-transfer-job-active-progress"),
        "已读取 / 已转换 / 已尝试 / 已提交 / 未知: 100 / 80 / 60 / 40 / 2"
    );
    assert_eq!(
        label_of(&rt, "data-transfer-job-active-state"),
        "job_id=job-apply-77|state=queued"
    );
    // The replayed apply view rides the mirror, not the card.
    let label = label_of(&rt, "data-transfer-result-attrs");
    assert!(label.contains("replayed=true"), "{label}");
    assert!(label.contains("verdict_severity=active"), "{label}");
    assert!(label.contains("plan_id=plan-77"), "{label}");
}

#[test]
fn a_stale_plan_refusal_carries_its_own_severity_and_the_re_review() {
    let mut run = TransferJobRunVm::default();
    run.failure = Some(TransferRunFailure {
        kind: TransferRunFailureKind::StalePlan,
        message: "PlanStale: source changed".to_owned(),
    });
    let mut rt = runtime(Scene { run, dark: false, reviews: 0 }, (960.0, 720.0));
    // The audit prefix: both the notice and its re-review ride it.
    assert!(has(&rt, "audit-transfer-failure"));
    assert!(has(&rt, "audit-transfer-re-review"));
    assert_eq!(
        text_of(&rt, "audit-transfer-failure-title"),
        t("migration.failure.stalePlan.title")
    );
    // No apply: the mirror carries React's no-verdict spellings.
    let label = label_of(&rt, "data-transfer-result-attrs");
    assert!(
        label.contains("verdict_severity=failed")
            && label.contains("completed=false")
            && label.contains("cancel_disposition=none")
            && label.contains("plan_id=undefined"),
        "{label}"
    );
    // The re-review press reaches the panel's callback.
    click(&mut rt, "audit-transfer-re-review");
    assert_eq!(rt.state.reviews, 1);
}

#[test]
fn a_boundary_backed_succeeded_apply_mounts_the_ok_verdict() {
    // The coder vectors never reach `ok` (no boundaries); one verified
    // boundary does — the only path React trusts a transfer write.
    let mut run = TransferJobRunVm::default();
    run.apply_view = Some(TransferApplyJobViewVm {
        job_id: "job-ok".to_owned(),
        kind: "dataTransferApply".to_owned(),
        state: JobState::Succeeded,
        effect_outcome: Some("completed".to_owned()),
        progress: JobProgress {
            read: text_counter("5"),
            converted: text_counter("5"),
            attempted: text_counter("5"),
            committed: text_counter("5"),
            unknown: None,
        },
        plan_id: "plan-ok".to_owned(),
        plan_digest: "digest-ok".to_owned(),
        selection_revision: 9,
        commit_boundaries: vec![CommitBoundary {
            stage_id: "write".to_owned(),
            committed_at: "2026-11-21T00:00:00.000Z".to_owned(),
            evidence: vec!["receipt".to_owned()],
            operation_id: None,
        }],
        artifact_ids: Vec::new(),
        cancelled: false,
        partial: false,
        replayed: false,
        error: None,
        recovery_verdict: None,
        recovery_resume_through: None,
        recovery_reason: None,
    });
    let rt = runtime(Scene { run, dark: false, reviews: 0 }, (960.0, 720.0));
    assert!(has(&rt, "audit-transfer-verdict"), "the shared panel mounts under the audit prefix");
    let label = label_of(&rt, "data-transfer-result-attrs");
    assert!(
        label.contains("verdict_severity=ok") && label.contains("completed=true"),
        "{label}"
    );
}

#[test]
fn a_cancel_acknowledged_run_propagates_its_disposition() {
    let cancelled_view = |effect_outcome: Option<&str>| {
        TransferApplyJobViewVm {
            job_id: "job-c".to_owned(),
            kind: "dataTransferApply".to_owned(),
            state: JobState::Cancelled,
            effect_outcome: effect_outcome.map(str::to_owned),
            progress: JobProgress::default(),
            plan_id: "plan-c".to_owned(),
            plan_digest: "digest-c".to_owned(),
            selection_revision: 1,
            commit_boundaries: Vec::new(),
            artifact_ids: Vec::new(),
            cancelled: true,
            partial: false,
            replayed: false,
            error: None,
            recovery_verdict: None,
            recovery_resume_through: None,
            recovery_reason: None,
        }
    };

    // Nothing committed and no outcome: the cancel landed before
    // anything happened — `notStarted` (the engine's own spelling, the
    // exact lie the cancel model forbids is calling it "rolled back").
    let mut run = TransferJobRunVm::default();
    run.apply_view = Some(cancelled_view(None));
    run.cancel_requested = true;
    run.cancel_acknowledged = true;
    let rt = runtime(Scene { run, dark: false, reviews: 0 }, (960.0, 720.0));
    let label = label_of(&rt, "data-transfer-result-attrs");
    assert!(label.contains("cancel_disposition=notStarted"), "{label}");

    // A backend that reports `rolledBack` settles rolled back.
    let mut run = TransferJobRunVm::default();
    run.apply_view = Some(cancelled_view(Some("rolledBack")));
    run.cancel_requested = true;
    run.cancel_acknowledged = true;
    let rt = runtime(Scene { run, dark: false, reviews: 0 }, (960.0, 720.0));
    let label = label_of(&rt, "data-transfer-result-attrs");
    assert!(
        label.contains("cancel_disposition=settledRolledBack") && label.contains("verdict_severity=failed"),
        "{label}"
    );
}
