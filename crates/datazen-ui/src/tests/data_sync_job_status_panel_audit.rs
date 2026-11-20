//! T148 independent audit (tester): fresh vectors the coder suite does not
//! cover — the partial banner matrix, the empty-string hydration error,
//! the `Number()` edge spellings (`+8`, `-0`), the NaN counter printing in
//! the mounted row, the raw wire-state label table, the row's
//! justify-between geometry (order of the parts by x position), and the
//! light theme.

use rune::{col, Cx, El, Env, Runtime};

use crate::i18n::t;
use crate::views::{
    committed_result, durable_job_state_label, js_number, js_number_to_string,
    DataSyncJobDetailsVm, DataSyncJobStatusPanel, DataSyncJobStatusPanelVm, DomainCounter,
    DurableJobView, JobCounter, JobProgress, JobRecoveryVerdict, JobState,
};

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn details(job_id: &str, state: JobState, verdict: Option<JobRecoveryVerdict>) -> DataSyncJobDetailsVm {
    DataSyncJobDetailsVm {
        job: DurableJobView {
            job_id: job_id.to_owned(),
            kind: "dataSyncApply".to_owned(),
            state,
            cancel_requested: false,
            progress: JobProgress {
                committed: Some(JobCounter::Text("3".to_owned())),
                ..JobProgress::default()
            },
        },
        recovery_verdict: verdict,
        domain_counters: Vec::new(),
    }
}

#[derive(Default)]
struct Scene {
    vm: DataSyncJobStatusPanelVm,
    dark: bool,
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        DataSyncJobStatusPanel::<Scene>::new(scene.vm.clone()).build(scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn text_of<S>(rt: &Runtime<S>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn rect_of<S>(rt: &Runtime<S>, key: &str) -> [f32; 4] {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .map(|e| e.rect)
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

// ---- the partial banner matrix ----

#[test]
fn hydration_error_alone_mounts_one_banner_and_no_list() {
    let vm = DataSyncJobStatusPanelVm {
        hydration_error: Some("store poisoned".to_owned()),
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with_vm(vm), (960.0, 640.0));
    assert!(!has_key(&rt, "data-sync-pending-hint"));
    assert!(has_key(&rt, "data-sync-hydration-error"));
    assert_eq!(text_of(&rt, "data-sync-hydration-error-text"), "store poisoned");
    assert!(!has_key(&rt, "data-sync-not-executed-hint"));
    assert!(!has_key(&rt, "data-sync-job-history"));
}

#[test]
fn counts_without_jobs_mount_banners_but_no_list() {
    let vm = DataSyncJobStatusPanelVm {
        pending_verification_count: 1,
        not_executed_count: 3,
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with_vm(vm), (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-pending-hint"));
    assert!(!has_key(&rt, "data-sync-hydration-error"));
    assert!(has_key(&rt, "data-sync-not-executed-hint"));
    assert!(!has_key(&rt, "data-sync-job-history"));
}

#[test]
fn an_empty_hydration_error_string_is_falsy_and_renders_nothing() {
    let vm = DataSyncJobStatusPanelVm {
        hydration_error: Some(String::new()),
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with_vm(vm), (960.0, 640.0));
    assert!(
        !has_key(&rt, "data-sync-hydration-error"),
        "React's `hydrationError ?` hides the empty string like null"
    );
}

// ---- Number() edge spellings ----

#[test]
fn js_number_sign_spellings_parse() {
    assert_eq!(js_number(&JobCounter::Text("+8".to_owned())), 8.0);
    assert_eq!(js_number(&JobCounter::Text("-0".to_owned())), 0.0);
    assert_eq!(js_number(&JobCounter::Text("  +0X1f  ".to_owned())), 31.0);
}

#[test]
fn a_nan_counter_prints_nan_in_the_row() {
    let d = DataSyncJobDetailsVm {
        domain_counters: vec![DomainCounter {
            code: "committed".to_owned(),
            value: JobCounter::Text("n/a".to_owned()),
        }],
        ..details("j-nan", JobState::Succeeded, None)
    };
    assert!(committed_result(&d).is_nan());
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![d],
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with_vm(vm), (960.0, 640.0));
    assert_eq!(
        text_of(&rt, "data-sync-durable-job-j-nan-committed"),
        format!("{}: NaN", t("migration.verdict.committedRows"))
    );
}

// ---- the raw wire-state label table ----

#[test]
fn the_five_wire_states_print_verbatim_without_a_receipt() {
    for (state, wire) in [
        (JobState::Queued, "queued"),
        (JobState::Running, "running"),
        (JobState::Succeeded, "succeeded"),
        (JobState::Failed, "failed"),
        (JobState::Cancelled, "cancelled"),
    ] {
        let d = details("j", state, None);
        assert_eq!(durable_job_state_label(&d), wire);
        assert_eq!(js_number_to_string(committed_result(&d)), "3");
    }
}

// ---- row part order by geometry (justify-between) ----

#[test]
fn row_parts_keep_the_react_dom_order_left_to_right() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: vec![details(
            "j-order",
            JobState::Running,
            Some(JobRecoveryVerdict::PendingVerification),
        )],
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with_vm(vm), (960.0, 640.0));
    let base = "data-sync-durable-job-j-order";
    let kind_x = rect_of(&rt, &format!("{base}-kind"))[0];
    let state_x = rect_of(&rt, &format!("{base}-state"))[0];
    let committed_x = rect_of(&rt, &format!("{base}-committed"))[0];
    let verify_x = rect_of(&rt, "data-sync-verify-recovery-j-order")[0];
    let cancel_x = rect_of(&rt, "data-sync-cancel-job-j-order")[0];
    assert!(
        kind_x <= state_x && state_x <= committed_x && committed_x <= verify_x && verify_x <= cancel_x,
        "kind {kind_x} <= state {state_x} <= committed {committed_x} <= verify {verify_x} <= cancel {cancel_x}"
    );
    // The three spans and the two buttons are all vertically centered on
    // the same row (items-center).
    let row_h = rect_of(&rt, base)[3];
    let row_y = rect_of(&rt, base)[1];
    for part_x_key in [
        format!("{base}-kind"),
        format!("{base}-state"),
        format!("{base}-committed"),
    ] {
        let [x, y, w, h] = rect_of(&rt, &part_x_key);
        assert!(y >= row_y - 0.5 && y + h <= row_y + row_h + 0.5, "{part_x_key} stays inside the row: x={x} y={y}");
    }
}

// ---- five rows render one after another ----

#[test]
fn five_rows_stack_in_job_order() {
    let vm = DataSyncJobStatusPanelVm {
        durable_jobs: (0..5)
            .map(|i| details(&format!("j-{i}"), JobState::Succeeded, None))
            .collect(),
        ..DataSyncJobStatusPanelVm::default()
    };
    let rt = mount(Scene::with_vm(vm), (960.0, 640.0));
    let mut last_y = -1.0;
    for i in 0..5 {
        let key = format!("data-sync-durable-job-j-{i}");
        let y = rect_of(&rt, &key)[1];
        assert!(y >= last_y, "{key} at y={y} follows y={last_y}");
        last_y = y;
    }
}

// ---- light theme ----

#[test]
fn the_light_theme_mounts_the_same_tree() {
    let vm = DataSyncJobStatusPanelVm {
        pending_verification_count: 1,
        durable_jobs: vec![details(
            "j-light",
            JobState::Running,
            Some(JobRecoveryVerdict::PendingVerification),
        )],
        ..DataSyncJobStatusPanelVm::default()
    };
    let mut scene = Scene::with_vm(vm);
    scene.dark = false;
    let rt = mount(scene, (960.0, 640.0));
    assert!(has_key(&rt, "data-sync-job-status-panel"));
    assert!(has_key(&rt, "data-sync-pending-hint"));
    assert!(has_key(&rt, "data-sync-job-history"));
    assert!(has_key(&rt, "data-sync-verify-recovery-j-light"));
    assert!(has_key(&rt, "data-sync-cancel-job-j-light"));
}

impl Scene {
    fn with_vm(vm: DataSyncJobStatusPanelVm) -> Self {
        Self {
            vm,
            dark: true,
            ..Self::default()
        }
    }
}
