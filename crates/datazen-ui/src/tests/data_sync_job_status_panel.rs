//! T148: `windows/data-sync/DataSyncJobStatusPanel.tsx` port — the loose
//! `Number()` semantics, the recovery-verdict label branches, the committed
//! fallback chain, the conditional banners / history list, the button
//! mounts + disabled logic + callbacks, and window-width geometry.

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::{has, t};
use crate::views::{
    committed_result, durable_job_state_label, js_number, js_number_to_string,
    DataSyncJobDetailsVm, DataSyncJobStatusPanel, DataSyncJobStatusPanelVm, DomainCounter,
    DurableJobView, JobCounter, JobProgress, JobRecoveryVerdict, JobState,
};

const PREFIX: &str = "data-sync";
const JOB_ID: &str = "sync-job-1";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn progress(committed: &str) -> JobProgress {
    JobProgress {
        committed: Some(JobCounter::Text(committed.to_owned())),
        ..JobProgress::default()
    }
}

fn details(
    job_id: &str,
    state: JobState,
    verdict: Option<JobRecoveryVerdict>,
    domain_committed: Option<&str>,
) -> DataSyncJobDetailsVm {
    DataSyncJobDetailsVm {
        job: DurableJobView {
            job_id: job_id.to_owned(),
            kind: "dataSyncApply".to_owned(),
            state,
            cancel_requested: false,
            progress: progress("7"),
        },
        recovery_verdict: verdict,
        domain_counters: domain_committed
            .map(|value| {
                vec![DomainCounter {
                    code: "committed".to_owned(),
                    value: JobCounter::Text(value.to_owned()),
                }]
            })
            .unwrap_or_default(),
    }
}

#[derive(Default)]
struct Scene {
    vm: DataSyncJobStatusPanelVm,
    verify: Vec<DataSyncJobDetailsVm>,
    cancels: Vec<String>,
    dark: bool,
}

impl Scene {
    fn with(vm: DataSyncJobStatusPanelVm) -> Self {
        Self {
            vm,
            dark: true,
            ..Self::default()
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        DataSyncJobStatusPanel::<Scene>::new(scene.vm.clone())
            .on_verify_recovery(|s: &mut Scene, d| s.verify.push(d))
            .on_cancel_job(|s: &mut Scene, j| s.cancels.push(j))
            .build(scene.dark),
    )
}

fn view_without_callbacks(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full()
        .child(DataSyncJobStatusPanel::<Scene>::new(scene.vm.clone()).build(scene.dark))
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn maybe<S>(rt: &Runtime<S>, key: &str) -> Option<rune::ElementInfo> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    maybe(rt, key).is_some()
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key).and_then(|e| e.text).unwrap_or_default()
}

/// A kit button's accessible label: the button element itself carries it
/// (`ElementInfo.label`).
fn label_of<S>(rt: &Runtime<S>, key: &str) -> String {
    maybe(rt, key)
        .and_then(|e| e.label)
        .unwrap_or_else(|| panic!("{key} has a label"))
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

fn row_key(job_id: &str) -> String {
    format!("data-sync-durable-job-{job_id}")
}

fn assert_no_horizontal_overflow<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let mut outside = Vec::new();
    for e in rt.elements() {
        let Some(key) = e.key.as_deref() else {
            continue;
        };
        if !key.starts_with(PREFIX) {
            continue;
        }
        let [x, _y, ew, _eh] = e.rect;
        if x < -0.5 || x + ew > w + 0.5 {
            outside.push(format!("{key} {:?}", e.rect));
        }
    }
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) out of {w}x{h}: {outside:?}",
        outside.len()
    );
}

// ---- pure helpers: the loose Number() semantics ----

#[test]
fn js_number_passes_numbers_through_without_truncation() {
    // `Number(number)` is the identity — no `Math.trunc`, unlike
    // `readJobCounter`.
    assert_eq!(js_number(&JobCounter::Number(8.9)), 8.9);
    assert_eq!(js_number(&JobCounter::Number(-3.0)), -3.0);
    assert!(js_number(&JobCounter::Number(f64::NAN)).is_nan());
    assert_eq!(js_number(&JobCounter::Number(9_007_199_254_740_993.0)), 9_007_199_254_740_993.0);
}

#[test]
fn js_number_reads_decimal_strings() {
    assert_eq!(js_number(&JobCounter::Text("42".to_owned())), 42.0);
    assert_eq!(js_number(&JobCounter::Text("8.5".to_owned())), 8.5);
    assert_eq!(js_number(&JobCounter::Text("1e3".to_owned())), 1000.0);
    assert_eq!(
        js_number(&JobCounter::Text("9007199254740993".to_owned())),
        9_007_199_254_740_993.0,
        "no safe-integer gate on the loose branch"
    );
}

#[test]
fn js_number_trims_js_whitespace_and_blanks_to_zero() {
    assert_eq!(js_number(&JobCounter::Text("  8\t".to_owned())), 8.0);
    assert_eq!(js_number(&JobCounter::Text("\u{FEFF}8".to_owned())), 8.0);
    assert_eq!(js_number(&JobCounter::Text(String::new())), 0.0);
    assert_eq!(js_number(&JobCounter::Text("  ".to_owned())), 0.0);
}

#[test]
fn js_number_parses_hex_and_infinity_spellings_like_js() {
    assert_eq!(js_number(&JobCounter::Text("0x10".to_owned())), 16.0);
    assert_eq!(js_number(&JobCounter::Text("+0x1F".to_owned())), 31.0);
    assert!(js_number(&JobCounter::Text("-0x10".to_owned())).is_nan());
    assert!(js_number(&JobCounter::Text("0x".to_owned())).is_nan());
    assert!(js_number(&JobCounter::Text("0xg".to_owned())).is_nan());
    assert_eq!(
        js_number(&JobCounter::Text("Infinity".to_owned())),
        f64::INFINITY
    );
    assert_eq!(
        js_number(&JobCounter::Text("+Infinity".to_owned())),
        f64::INFINITY
    );
    assert_eq!(
        js_number(&JobCounter::Text("-Infinity".to_owned())),
        f64::NEG_INFINITY
    );
}

#[test]
fn js_number_is_nan_for_the_rest() {
    assert!(js_number(&JobCounter::Text("abc".to_owned())).is_nan());
    assert!(js_number(&JobCounter::Text("12,5".to_owned())).is_nan());
}

#[test]
fn js_number_to_string_prints_the_js_spellings() {
    assert_eq!(js_number_to_string(42.0), "42");
    assert_eq!(js_number_to_string(8.5), "8.5");
    assert_eq!(js_number_to_string(0.0), "0");
    assert_eq!(js_number_to_string(f64::NAN), "NaN");
    assert_eq!(js_number_to_string(f64::INFINITY), "Infinity");
    assert_eq!(js_number_to_string(f64::NEG_INFINITY), "-Infinity");
}

#[test]
fn recovery_verdict_round_trips_the_wire_spellings() {
    for verdict in [
        JobRecoveryVerdict::NotExecuted,
        JobRecoveryVerdict::PendingVerification,
        JobRecoveryVerdict::ResumeAfterVerify,
        JobRecoveryVerdict::Reject,
        JobRecoveryVerdict::RequireManualReview,
    ] {
        assert_eq!(JobRecoveryVerdict::from_str(verdict.as_str()), Some(verdict));
    }
    assert_eq!(JobRecoveryVerdict::from_str("settled"), None);
    assert_eq!(JobRecoveryVerdict::from_str(""), None);
}

// ---- pure helpers: the label and committed chains ----

#[test]
fn state_label_prefers_the_two_named_verdicts() {
    assert!(has("migrationJob.notExecutedJob"));
    assert!(has("migrationJob.pendingVerificationHint"));
    let mut d = details(JOB_ID, JobState::Succeeded, None, None);
    assert_eq!(durable_job_state_label(&d), "succeeded");
    d.recovery_verdict = Some(JobRecoveryVerdict::NotExecuted);
    assert_eq!(
        durable_job_state_label(&d),
        t("migrationJob.notExecutedJob")
    );
    d.recovery_verdict = Some(JobRecoveryVerdict::PendingVerification);
    assert_eq!(
        durable_job_state_label(&d),
        t("migrationJob.pendingVerificationHint")
    );
}

#[test]
fn state_label_falls_through_for_the_other_verdicts() {
    for verdict in [
        JobRecoveryVerdict::ResumeAfterVerify,
        JobRecoveryVerdict::Reject,
        JobRecoveryVerdict::RequireManualReview,
    ] {
        let d = details(JOB_ID, JobState::Failed, Some(verdict), None);
        assert_eq!(durable_job_state_label(&d), "failed");
    }
}

#[test]
fn committed_prefers_the_domain_counter_over_the_progress_bucket() {
    let d = details(JOB_ID, JobState::Succeeded, None, Some("42"));
    assert_eq!(committed_result(&d), 42.0);
}

#[test]
fn committed_falls_back_to_the_progress_bucket_when_no_counter_matches() {
    let d = details(JOB_ID, JobState::Succeeded, None, None);
    assert_eq!(committed_result(&d), 7.0);
    // A counter with a different code is not a match.
    let d = DataSyncJobDetailsVm {
        domain_counters: vec![DomainCounter {
            code: "read".to_owned(),
            value: JobCounter::Text("99".to_owned()),
        }],
        ..details(JOB_ID, JobState::Succeeded, None, None)
    };
    assert_eq!(committed_result(&d), 7.0);
}

#[test]
fn committed_is_nan_when_both_readings_are_missing() {
    let mut d = details(JOB_ID, JobState::Succeeded, None, None);
    d.job.progress.committed = None;
    assert!(committed_result(&d).is_nan());
}

#[test]
fn committed_keeps_a_nan_counter_reading_never_rescues_it() {
    // `??` fires on undefined only: a counter that reads as NaN stays NaN
    // even though the progress bucket holds a good number.
    let d = DataSyncJobDetailsVm {
        domain_counters: vec![DomainCounter {
            code: "committed".to_owned(),
            value: JobCounter::Text("n/a".to_owned()),
        }],
        ..details(JOB_ID, JobState::Succeeded, None, None)
    };
    assert!(committed_result(&d).is_nan());
}

#[test]
fn committed_takes_the_first_matching_counter() {
    let d = DataSyncJobDetailsVm {
        domain_counters: vec![
            DomainCounter {
                code: "committed".to_owned(),
                value: JobCounter::Text("10".to_owned()),
            },
            DomainCounter {
                code: "committed".to_owned(),
                value: JobCounter::Text("20".to_owned()),
            },
        ],
        ..details(JOB_ID, JobState::Succeeded, None, None)
    };
    assert_eq!(committed_result(&d), 10.0);
}

// ---- mounted rendering ----

#[test]
fn empty_state_mounts_only_the_root() {
    let rt = mount(Scene::with(DataSyncJobStatusPanelVm::default()), (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-job-status-panel"));
    assert!(!has_key(&rt, "data-sync-pending-hint"));
    assert!(!has_key(&rt, "data-sync-hydration-error"));
    assert!(!has_key(&rt, "data-sync-not-executed-hint"));
    assert!(!has_key(&rt, "data-sync-job-history"));
}

#[test]
fn banners_mount_on_their_own_truthiness() {
    let vm = DataSyncJobStatusPanelVm {
        pending_verification_count: 2,
        hydration_error: Some("job list unreadable".to_owned()),
        not_executed_count: 1,
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with(vm), (960.0, 640.0));
    assert!(has("migrationJob.pendingVerificationHint"));
    assert!(has("migrationJob.notExecutedHint"));
    assert_eq!(
        text_of(&rt, "data-sync-pending-hint-text"),
        t("migrationJob.pendingVerificationHint")
    );
    assert_eq!(text_of(&rt, "data-sync-hydration-error-text"), "job list unreadable");
    assert_eq!(
        text_of(&rt, "data-sync-not-executed-hint-text"),
        t("migrationJob.notExecutedHint")
    );
}

#[test]
fn history_mounts_only_with_durable_jobs_and_caps_at_five() {
    let mut vm = DataSyncJobStatusPanelVm::default();
    vm.durable_jobs = (0..6)
        .map(|i| details(&format!("job-{i}"), JobState::Succeeded, None, None))
        .collect();
    let rt = mount(Scene::with(vm), (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-job-history"));
    for i in 0..5 {
        assert!(has_key(&rt, &row_key(&format!("job-{i}"))));
    }
    assert!(
        !has_key(&rt, &row_key("job-5")),
        "slice(0, 5): the sixth job never renders"
    );
}

#[test]
fn row_renders_kind_state_and_committed_count() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![details(JOB_ID, JobState::Succeeded, None, Some("42"))],
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with(vm), (960.0, 640.0));
    let row = row_key(JOB_ID);
    assert_eq!(text_of(&rt, &format!("{row}-kind")), "dataSyncApply");
    assert_eq!(text_of(&rt, &format!("{row}-state")), "succeeded");
    assert_eq!(
        text_of(&rt, &format!("{row}-committed")),
        format!("{}: 42", t("migration.verdict.committedRows"))
    );
    assert!(has("migration.verdict.committedRows"));
}

#[test]
fn row_state_label_renders_the_verdict_texts() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![
            details("j-notexec", JobState::Succeeded, Some(JobRecoveryVerdict::NotExecuted), None),
            details("j-pending", JobState::Succeeded, Some(JobRecoveryVerdict::PendingVerification), None),
        ],
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with(vm), (960.0, 640.0));
    assert_eq!(
        text_of(&rt, &format!("{}-state", row_key("j-notexec"))),
        t("migrationJob.notExecutedJob")
    );
    assert_eq!(
        text_of(&rt, &format!("{}-state", row_key("j-pending"))),
        t("migrationJob.pendingVerificationHint")
    );
}

// ---- button mounts, disabled logic, callbacks ----

#[test]
fn verify_recovery_button_mounts_only_for_pending_verification() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![
            details("j-pending", JobState::Succeeded, Some(JobRecoveryVerdict::PendingVerification), None),
            details("j-notexec", JobState::Succeeded, Some(JobRecoveryVerdict::NotExecuted), None),
            details("j-none", JobState::Succeeded, None, None),
        ],
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with(vm), (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-verify-recovery-j-pending"));
    assert!(!has_key(&rt, "data-sync-verify-recovery-j-notexec"));
    assert!(!has_key(&rt, "data-sync-verify-recovery-j-none"));
}

#[test]
fn cancel_button_mounts_only_while_queued_or_running() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![
            details("j-queued", JobState::Queued, None, None),
            details("j-running", JobState::Running, None, None),
            details("j-succeeded", JobState::Succeeded, None, None),
            details("j-failed", JobState::Failed, None, None),
        ],
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with(vm), (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-cancel-job-j-queued"));
    assert!(has_key(&rt, "data-sync-cancel-job-j-running"));
    assert!(!has_key(&rt, "data-sync-cancel-job-j-succeeded"));
    assert!(!has_key(&rt, "data-sync-cancel-job-j-failed"));
}

#[test]
fn verify_recovery_press_carries_the_whole_details() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![details(JOB_ID, JobState::Succeeded, Some(JobRecoveryVerdict::PendingVerification), Some("42"))],
        ..DataSyncJobStatusPanelVm::default()
    };
    let mut rt = mount(Scene::with(vm), (960.0, 640.0));
    click(&mut rt, "data-sync-verify-recovery-sync-job-1");
    assert_eq!(rt.state.verify.len(), 1);
    assert_eq!(rt.state.verify[0].job.job_id, JOB_ID);
    assert_eq!(
        rt.state.verify[0].recovery_verdict,
        Some(JobRecoveryVerdict::PendingVerification)
    );
    assert_eq!(
        committed_result(&rt.state.verify[0]),
        42.0,
        "the payload keeps the domain counters"
    );
}

#[test]
fn cancel_press_carries_the_job_id() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![details(JOB_ID, JobState::Running, None, None)],
        ..DataSyncJobStatusPanelVm::default()
    };
    let mut rt = mount(Scene::with(vm), (960.0, 640.0));
    click(&mut rt, "data-sync-cancel-job-sync-job-1");
    assert_eq!(rt.state.cancels, vec![JOB_ID.to_owned()]);
}

#[test]
fn verifying_job_disables_the_button_and_swaps_the_label() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![details(JOB_ID, JobState::Succeeded, Some(JobRecoveryVerdict::PendingVerification), None)],
        verifying_recovery_job: Some(JOB_ID.to_owned()),
        ..DataSyncJobStatusPanelVm::default()
    };
    let mut rt = mount(Scene::with(vm), (960.0, 640.0));
    assert!(has("migrationJob.verifyingRecovery"));
    assert!(has("migrationJob.verifyRecovery"));
    assert_eq!(
        label_of(&rt, "data-sync-verify-recovery-sync-job-1"),
        t("migrationJob.verifyingRecovery")
    );
    click(&mut rt, "data-sync-verify-recovery-sync-job-1");
    assert!(
        rt.state.verify.is_empty(),
        "the button is disabled while the verify is in flight"
    );
}

#[test]
fn verifying_a_different_job_leaves_the_button_enabled() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![details(JOB_ID, JobState::Succeeded, Some(JobRecoveryVerdict::PendingVerification), None)],
        verifying_recovery_job: Some("another-job".to_owned()),
        ..DataSyncJobStatusPanelVm::default()
    };
    let mut rt = mount(Scene::with(vm), (960.0, 640.0));
    assert_eq!(
        label_of(&rt, "data-sync-verify-recovery-sync-job-1"),
        t("migrationJob.verifyRecovery")
    );
    click(&mut rt, "data-sync-verify-recovery-sync-job-1");
    assert_eq!(rt.state.verify.len(), 1);
}

#[test]
fn cancel_requested_disables_the_button_and_swaps_the_label() {
    let mut d = details(JOB_ID, JobState::Running, None, None);
    d.job.cancel_requested = true;
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![d],
        ..DataSyncJobStatusPanelVm::default()
    };
    let mut rt = mount(Scene::with(vm), (960.0, 640.0));
    assert!(has("migrationJob.cancellingJob"));
    assert!(has("migrationJob.cancelJob"));
    assert_eq!(
        label_of(&rt, "data-sync-cancel-job-sync-job-1"),
        t("migrationJob.cancellingJob")
    );
    click(&mut rt, "data-sync-cancel-job-sync-job-1");
    assert!(
        rt.state.cancels.is_empty(),
        "the button is disabled once the backend flagged the cancel"
    );
}

#[test]
fn cancelling_durable_job_disables_the_button_like_the_backend_flag() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![details(JOB_ID, JobState::Running, None, None)],
        cancelling_durable_job: Some(JOB_ID.to_owned()),
        ..DataSyncJobStatusPanelVm::default()
    };
    let mut rt = mount(Scene::with(vm), (960.0, 640.0));
    assert_eq!(
        label_of(&rt, "data-sync-cancel-job-sync-job-1"),
        t("migrationJob.cancellingJob")
    );
    click(&mut rt, "data-sync-cancel-job-sync-job-1");
    assert!(rt.state.cancels.is_empty());
}

#[test]
fn missing_callbacks_leave_the_buttons_harmless() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![details(
            JOB_ID,
            JobState::Running,
            Some(JobRecoveryVerdict::PendingVerification),
            None,
        )],
        ..DataSyncJobStatusPanelVm::default()
    };
    let dark = true;
    let mut rt = Runtime::new(Scene::with(vm), view_without_callbacks, (960.0, 640.0), env(dark));
    rt.frame(0.0);
    click(&mut rt, "data-sync-verify-recovery-sync-job-1");
    click(&mut rt, "data-sync-cancel-job-sync-job-1");
    // No callbacks registered, no panic, no state change to observe.
}

// ---- geometry ----

#[test]
fn five_full_rows_fit_the_window_without_horizontal_overflow() {
    let mut vm = DataSyncJobStatusPanelVm {
        pending_verification_count: 1,
        hydration_error: Some("hydration failed".to_owned()),
        not_executed_count: 1,
        ..DataSyncJobStatusPanelVm::default()
    };
    vm.durable_jobs = (0..5)
        .map(|i| {
            details(
                &format!("job-{i}"),
                JobState::Running,
                Some(JobRecoveryVerdict::PendingVerification),
                Some("123456"),
            )
        })
        .collect();
    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        let rt = mount(Scene::with(vm.clone()), (w, h));
        assert_no_horizontal_overflow(&rt, w, h, "data-sync status panel");
    }
}
