//! T073: `windows/data-transfer/DataTransferAttachedJobs.tsx` port — counter
//! reading, the active-job filter, cancel disposition + state machine, and
//! the mounted strip (state/progress lines, cancel notes, guarded button).

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::i18n::{has, t};
use datazen_ui::views::{
    active_attached_jobs, cancel_sent, cancel_unknown, job_counter_or_zero,
    job_state_label, progress_counts_text, read_job_counter, AttachedJob,
    AttachedJobsState, DataTransferAttachedJobs, JobCounter, JobProgress, JobState,
};

const PREFIX: &str = "data-transfer-attached-jobs";
const JOB_ID: &str = "transfer-dataTransferApply-1";

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn counter(value: u64) -> Option<JobCounter> {
    Some(JobCounter::Number(value as f64))
}

fn job(job_id: &str) -> AttachedJob {
    AttachedJob {
        job_id: job_id.to_string(),
        kind: "dataTransferApply".to_string(),
        state: JobState::Running,
        stage: Some("data".to_string()),
        cancel_requested: false,
        progress: JobProgress {
            read: counter(8),
            converted: counter(6),
            attempted: counter(4),
            committed: counter(2),
            unknown: counter(0),
        },
    }
}

#[derive(Default)]
struct Scene {
    jobs: Vec<AttachedJob>,
    exclude_job_id: Option<String>,
    state: AttachedJobsState,
    cancels: Vec<String>,
    dark: bool,
}

impl Scene {
    fn with(jobs: Vec<AttachedJob>) -> Self {
        Self {
            jobs,
            dark: true,
            ..Self::default()
        }
    }
}

fn view(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        DataTransferAttachedJobs::<Scene>::new(
            scene.jobs.clone(),
            scene.exclude_job_id.clone(),
            |s: &Scene| &s.state,
            |s: &mut Scene, job_id: &str| s.state.begin_cancel(job_id),
        )
        .on_cancel(|s: &mut Scene, job_id| s.cancels.push(job_id))
        .build(scene, scene.dark),
    )
}

fn view_no_cancel(scene: &Scene, _cx: &mut Cx<Scene>) -> El {
    col().w_full().h_full().child(
        DataTransferAttachedJobs::<Scene>::new(
            scene.jobs.clone(),
            scene.exclude_job_id.clone(),
            |s: &Scene| &s.state,
            |s: &mut Scene, job_id: &str| s.state.begin_cancel(job_id),
        )
        .build(scene, scene.dark),
    )
}

fn mount(scene: Scene, size: (f32, f32)) -> Runtime<Scene> {
    let dark = scene.dark;
    let mut rt = Runtime::new(scene, view, size, env(dark));
    rt.frame(0.0);
    rt
}

fn mount_with(
    scene: Scene,
    size: (f32, f32),
    view: fn(&Scene, &mut Cx<Scene>) -> El,
) -> Runtime<Scene> {
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
/// (`ElementInfo.label`); its visible text lives on a `label` child.
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
    format!("data-transfer-attached-job-{job_id}")
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

// ---- pure helpers: counter reading ----

#[test]
fn read_job_counter_accepts_numbers_and_text() {
    assert_eq!(read_job_counter(Some(&JobCounter::Number(8.0))), Some(8));
    // `Math.trunc`, not rounding.
    assert_eq!(read_job_counter(Some(&JobCounter::Number(8.9))), Some(8));
    assert_eq!(read_job_counter(Some(&JobCounter::Number(-0.0))), Some(0));
    assert_eq!(read_job_counter(Some(&JobCounter::Text("8".to_string()))), Some(8));
    // Leading zeros normalize, as `BigInt("007")` does.
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("007".to_string()))),
        Some(7)
    );
    // The largest safe integer survives as a string; one past it does not
    // (the string branch keeps React's `Number.isSafeInteger` gate).
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("9007199254740991".to_string()))),
        Some(9_007_199_254_740_991)
    );
    assert_eq!(
        read_job_counter(Some(&JobCounter::Number(9_007_199_254_740_991.0))),
        Some(9_007_199_254_740_991)
    );
    // The number branch has NO safe-integer gate (React: only
    // `Number.isFinite(value) && value >= 0`, then `Math.trunc`), so 2^53
    // — and 10^16 — read as themselves.
    assert_eq!(
        read_job_counter(Some(&JobCounter::Number(9_007_199_254_740_992.0))),
        Some(9_007_199_254_740_992)
    );
    assert_eq!(
        read_job_counter(Some(&JobCounter::Number(10_000_000_000_000_000.0))),
        Some(10_000_000_000_000_000)
    );
}

#[test]
fn read_job_counter_trims_js_whitespace_before_the_digit_check() {
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("  8\t".to_string()))),
        Some(8)
    );
    // U+FEFF is whitespace to JS `trim` but not to Rust `is_whitespace`.
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("\u{FEFF}8".to_string()))),
        Some(8)
    );
}

#[test]
fn read_job_counter_rejects_what_react_rejects() {
    assert_eq!(read_job_counter(None), None);
    assert_eq!(read_job_counter(Some(&JobCounter::Number(-1.0))), None);
    assert_eq!(read_job_counter(Some(&JobCounter::Number(f64::NAN))), None);
    assert_eq!(
        read_job_counter(Some(&JobCounter::Number(f64::INFINITY))),
        None
    );
    // The safe-integer gate lives on the string branch only.
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("9007199254740993".to_string()))),
        None
    );
    // Twenty-three digits pass the digit check but overflow u64, so the
    // parse itself refuses — still `None`, never a wrap or a panic.
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("99999999999999999999999".to_string()))),
        None
    );
    assert_eq!(read_job_counter(Some(&JobCounter::Text(String::new()))), None);
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("  ".to_string()))),
        None
    );
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("-3".to_string()))),
        None
    );
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("8.5".to_string()))),
        None
    );
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("8e3".to_string()))),
        None
    );
    assert_eq!(
        read_job_counter(Some(&JobCounter::Text("８".to_string()))),
        None,
        "full-width digits are not ASCII digits"
    );
}

#[test]
fn job_counter_or_zero_falls_back_to_zero() {
    assert_eq!(job_counter_or_zero(None), 0);
    assert_eq!(
        job_counter_or_zero(Some(&JobCounter::Text("nope".to_string()))),
        0
    );
    assert_eq!(job_counter_or_zero(Some(&JobCounter::Number(3.0))), 3);
}

#[test]
fn progress_counts_text_joins_five_buckets_in_order() {
    assert_eq!(progress_counts_text(&job(JOB_ID).progress), "8 / 6 / 4 / 2 / 0");
    let mut progress = JobProgress::default();
    progress.read = counter(1);
    progress.unknown = Some(JobCounter::Text("bad".to_string()));
    assert_eq!(progress_counts_text(&progress), "1 / 0 / 0 / 0 / 0");
    assert_eq!(progress_counts_text(&JobProgress::default()), "0 / 0 / 0 / 0 / 0");
}

// ---- pure helpers: state labels, filter, disposition ----

#[test]
fn job_state_round_trips_its_wire_spelling() {
    for (value, spelling) in [
        (JobState::Queued, "queued"),
        (JobState::Running, "running"),
        (JobState::Succeeded, "succeeded"),
        (JobState::Failed, "failed"),
        (JobState::Cancelled, "cancelled"),
    ] {
        assert_eq!(value.as_str(), spelling);
        assert_eq!(JobState::from_str(spelling), Some(value));
    }
    assert_eq!(JobState::from_str("done"), None);
    assert_eq!(JobState::from_str(""), None);
}

#[test]
fn job_state_label_appends_a_non_empty_stage() {
    assert_eq!(job_state_label(&job(JOB_ID)), "传输进行中 · data");
    let mut queued = job(JOB_ID);
    queued.state = JobState::Queued;
    queued.stage = None;
    assert_eq!(job_state_label(&queued), "传输已排队");
    let mut empty_stage = job(JOB_ID);
    empty_stage.stage = Some(String::new());
    assert_eq!(
        job_state_label(&empty_stage),
        "传输进行中",
        "an empty stage is falsy in React too"
    );
}

#[test]
fn active_filter_keeps_only_running_transfer_jobs() {
    let mut wrong_kind = job("job-kind");
    wrong_kind.kind = "dataSyncApply".to_string();
    let mut succeeded = job("job-done");
    succeeded.state = JobState::Succeeded;
    let mut failed = job("job-failed");
    failed.state = JobState::Failed;
    let mut cancelled = job("job-cancelled");
    cancelled.state = JobState::Cancelled;
    let mut queued = job("job-queued");
    queued.state = JobState::Queued;
    let jobs = vec![
        job(JOB_ID),
        wrong_kind,
        succeeded,
        failed,
        cancelled,
        queued,
    ];
    let active = active_attached_jobs(&jobs, None);
    assert_eq!(
        active.iter().map(|job| job.job_id.as_str()).collect::<Vec<_>>(),
        vec![JOB_ID, "job-queued"],
        "kind, terminal states and nothing else are filtered"
    );
    assert!(active_attached_jobs(&[], None).is_empty());
    let excluded = active_attached_jobs(&jobs, Some(JOB_ID));
    assert_eq!(
        excluded
            .iter()
            .map(|job| job.job_id.as_str())
            .collect::<Vec<_>>(),
        vec!["job-queued"],
        "excludeJobId removes only that job"
    );
    let missing = active_attached_jobs(&jobs, Some("no-such-job"));
    assert_eq!(missing.len(), 2, "an unknown exclusion removes nothing");
}

#[test]
fn cancel_disposition_prefers_the_backend_flag() {
    let state = AttachedJobsState::default();
    assert!(!cancel_sent(&job(JOB_ID), &state));
    assert!(!cancel_unknown(&job(JOB_ID), &state));

    let mut accepted = AttachedJobsState::default();
    accepted.finish_cancel(JOB_ID, true);
    assert!(cancel_sent(&job(JOB_ID), &accepted));
    assert!(!cancel_unknown(&job(JOB_ID), &accepted));

    let mut rejected = AttachedJobsState::default();
    rejected.finish_cancel(JOB_ID, false);
    assert!(!cancel_sent(&job(JOB_ID), &rejected));
    assert!(cancel_unknown(&job(JOB_ID), &rejected));

    // The backend flag wins even when a local cancel came back false:
    // React's ternary checks `cancelSent` first.
    let mut flagged = job(JOB_ID);
    flagged.cancel_requested = true;
    assert!(cancel_sent(&flagged, &rejected));
    assert!(cancel_unknown(&flagged, &rejected));
}

#[test]
fn every_attached_jobs_key_resolves() {
    for key in [
        "transfer.job.attachedTitle",
        "migration.progress.counts",
        "migration.cancel.requestedInFlight",
        "migration.cancel.unknownJob",
        "transfer.cancel",
    ] {
        assert!(has(key), "{key} is missing from the locale packs");
        assert_ne!(t(key), key, "{key} echoes instead of translating");
    }
}

// ---- state machine: the in-flight guard and the two outcomes ----

#[test]
fn begin_cancel_guards_reentry() {
    let mut state = AttachedJobsState::default();
    assert!(state.begin_cancel(JOB_ID));
    assert!(state.canceling.contains(JOB_ID));
    // The second entry changes nothing.
    assert!(!state.begin_cancel(JOB_ID));
    assert!(state.canceling.contains(JOB_ID));
    // Another job is unaffected by the guard.
    assert!(state.begin_cancel("job-2"));
}

#[test]
fn finish_cancel_clears_the_mark_and_records_the_outcome() {
    let mut state = AttachedJobsState::default();
    assert!(state.begin_cancel(JOB_ID));
    state.finish_cancel(JOB_ID, true);
    assert!(!state.canceling.contains(JOB_ID));
    assert_eq!(state.cancel_result.get(JOB_ID), Some(&true));
    // A settled job can be cancelled again (retry after a rejection).
    assert!(state.begin_cancel(JOB_ID));
    state.finish_cancel(JOB_ID, false);
    assert_eq!(state.cancel_result.get(JOB_ID), Some(&false));
    // A fresh `begin` never clears a previous outcome — React does not
    // clear `cancelResult` on entry either.
    assert!(state.begin_cancel(JOB_ID));
    assert_eq!(state.cancel_result.get(JOB_ID), Some(&false));
}

// ---- mounted strip: visibility, lines, notes, guarded button ----

#[test]
fn no_active_jobs_render_nothing() {
    let rt = mount(Scene::with(vec![]), (960.0, 600.0));
    assert!(!has_key(&rt, PREFIX));
    let mut terminal = job(JOB_ID);
    terminal.state = JobState::Cancelled;
    terminal.cancel_requested = true;
    let rt = mount(Scene::with(vec![terminal]), (960.0, 600.0));
    assert!(!has_key(&rt, PREFIX));
}

#[test]
fn running_job_shows_state_stage_and_counts() {
    let rt = mount(Scene::with(vec![job(JOB_ID)]), (960.0, 600.0));
    let row = row_key(JOB_ID);
    assert!(has_key(&rt, PREFIX));
    assert!(has_key(&rt, &row));
    assert_eq!(text_of(&rt, &format!("{row}-state")), "传输进行中 · data");
    assert_eq!(
        text_of(&rt, &format!("{row}-progress")),
        "已读取 / 已转换 / 已尝试 / 已提交 / 未知: 8 / 6 / 4 / 2 / 0"
    );
    assert!(!has_key(&rt, &format!("{row}-cancel-requested")));
    assert!(!has_key(&rt, &format!("{row}-cancel-unknown")));
    assert_eq!(
        label_of(&rt, &format!("{row}-cancel")),
        t("transfer.cancel")
    );
}

#[test]
fn queued_job_without_a_stage_shows_the_bare_state_label() {
    let mut queued = job(JOB_ID);
    queued.state = JobState::Queued;
    queued.stage = None;
    let rt = mount(Scene::with(vec![queued]), (960.0, 600.0));
    assert_eq!(
        text_of(&rt, &format!("{}-state", row_key(JOB_ID))),
        "传输已排队"
    );
}

#[test]
fn excluded_job_renders_nothing() {
    let mut scene = Scene::with(vec![job(JOB_ID)]);
    scene.exclude_job_id = Some(JOB_ID.to_string());
    let rt = mount(scene, (960.0, 600.0));
    assert!(!has_key(&rt, PREFIX));
}

#[test]
fn pressing_cancel_records_only_that_job_and_disables_the_button() {
    let mut rt = mount(
        Scene::with(vec![job(JOB_ID), job("job-2")]),
        (960.0, 600.0),
    );
    click(&mut rt, &format!("{}-cancel", row_key(JOB_ID)));
    assert_eq!(rt.state.cancels, vec![JOB_ID.to_string()]);
    assert!(rt.state.state.canceling.contains(JOB_ID));
    // No receipt yet: neither note shows while the cancel is in flight.
    assert!(!has_key(&rt, &format!("{}-cancel-requested", row_key(JOB_ID))));
    assert!(!has_key(&rt, &format!("{}-cancel-unknown", row_key(JOB_ID))));
    // The rebuilt strip leaves the press handler off while canceling, so
    // the second press at the same spot records nothing.
    click(&mut rt, &format!("{}-cancel", row_key(JOB_ID)));
    assert_eq!(rt.state.cancels, vec![JOB_ID.to_string()]);
    // The other job's button is untouched and still live.
    click(&mut rt, &format!("{}-cancel", row_key("job-2")));
    assert_eq!(
        rt.state.cancels,
        vec![JOB_ID.to_string(), "job-2".to_string()]
    );
}

#[test]
fn accepted_cancel_shows_the_requested_note_and_disables_the_button() {
    let mut scene = Scene::with(vec![job(JOB_ID)]);
    scene.state.begin_cancel(JOB_ID);
    scene.state.finish_cancel(JOB_ID, true);
    let mut rt = mount(scene, (960.0, 600.0));
    let row = row_key(JOB_ID);
    assert_eq!(
        text_of(&rt, &format!("{row}-cancel-requested")),
        t("migration.cancel.requestedInFlight")
    );
    assert!(!has_key(&rt, &format!("{row}-cancel-unknown")));
    click(&mut rt, &format!("{row}-cancel"));
    assert!(
        rt.state.cancels.is_empty(),
        "a cancel-sent job cannot be cancelled again"
    );
}

#[test]
fn rejected_cancel_shows_the_unknown_note_and_leaves_a_retry() {
    let mut scene = Scene::with(vec![job(JOB_ID)]);
    scene.state.begin_cancel(JOB_ID);
    scene.state.finish_cancel(JOB_ID, false);
    let mut rt = mount(scene, (960.0, 600.0));
    let row = row_key(JOB_ID);
    assert_eq!(
        text_of(&rt, &format!("{row}-cancel-unknown")),
        t("migration.cancel.unknownJob")
    );
    assert!(!has_key(&rt, &format!("{row}-cancel-requested")));
    // React leaves the button enabled after a rejection, so a retry works.
    click(&mut rt, &format!("{row}-cancel"));
    assert_eq!(rt.state.cancels, vec![JOB_ID.to_string()]);
}

#[test]
fn backend_cancel_requested_shows_the_note_without_a_press() {
    let mut flagged = job(JOB_ID);
    flagged.cancel_requested = true;
    let mut rt = mount(Scene::with(vec![flagged]), (960.0, 600.0));
    let row = row_key(JOB_ID);
    assert_eq!(
        text_of(&rt, &format!("{row}-cancel-requested")),
        t("migration.cancel.requestedInFlight")
    );
    click(&mut rt, &format!("{row}-cancel"));
    assert!(
        rt.state.cancels.is_empty(),
        "the button is disabled while cancelSent"
    );
}

#[test]
fn cancel_requested_wins_over_a_rejected_receipt_on_the_strip() {
    let mut flagged = job(JOB_ID);
    flagged.cancel_requested = true;
    let mut scene = Scene::with(vec![flagged]);
    scene.state.finish_cancel(JOB_ID, false);
    let rt = mount(scene, (960.0, 600.0));
    let row = row_key(JOB_ID);
    assert!(has_key(&rt, &format!("{row}-cancel-requested")));
    assert!(!has_key(&rt, &format!("{row}-cancel-unknown")));
}

#[test]
fn two_rows_fit_the_window_without_horizontal_overflow() {
    let rt = mount(
        Scene::with(vec![job(JOB_ID), job("job-2")]),
        (960.0, 600.0),
    );
    assert_no_horizontal_overflow(&rt, 960.0, 600.0, "two attached jobs");
}

#[test]
fn without_a_cancel_callback_the_button_renders_but_does_nothing() {
    let mut rt = mount_with(
        Scene::with(vec![job(JOB_ID)]),
        (960.0, 600.0),
        view_no_cancel,
    );
    let row = row_key(JOB_ID);
    assert!(has_key(&rt, &format!("{row}-cancel")));
    assert_eq!(label_of(&rt, &format!("{row}-cancel")), t("transfer.cancel"));
    click(&mut rt, &format!("{row}-cancel"));
    assert!(
        rt.state.cancels.is_empty(),
        "no host starter, so no cancel can be recorded"
    );
}

#[test]
fn a_stale_press_while_canceling_is_swallowed_by_the_guard() {
    // The strip was built while idle, so the button still carries a press
    // handler; the state flips to canceling before the next frame rebuilds
    // the tree. The press must still be a no-op via `begin_cancel`
    // returning false — the guard's second layer, not just the handler
    // being off. (T057's lesson: never claim this branch unreachable.)
    let mut rt = mount(Scene::with(vec![job(JOB_ID)]), (960.0, 600.0));
    assert!(rt.state.state.begin_cancel(JOB_ID));
    click(&mut rt, &format!("{}-cancel", row_key(JOB_ID)));
    assert!(rt.state.cancels.is_empty());
    assert!(rt.state.state.canceling.contains(JOB_ID));
}
