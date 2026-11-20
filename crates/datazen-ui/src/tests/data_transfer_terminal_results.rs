//! T113: the Data Transfer terminal-results strip — real probes against the
//! rendered element tree.
//!
//! The strip that restores the latest settled `dataTransferApply` verdict
//! from the durable Job details query. Every assertion observes runtime
//! elements (keys, texts, rects, the data-* mirror label), never a value
//! the component reports about itself:
//!
//! * the nothing-rendered gate (no applies / hidden / excluded / in flight);
//! * the explicitly-uncompleted surface while the details query runs, and
//!   its failed twin once the query rejects;
//! * the restored surface — the title line, the shared verdict panel under
//!   its `data-transfer-job` prefix, the derived verdict and the plan
//!   mirror — for a clean completed restore;
//! * the T561 placeholder verdict fold (`deriveMigrationJobVerdict` and
//!   friends, transliterated from `lib/migrationJobVerdict.ts`), including
//!   its fail-closed rules and the `pickSeverity` quirk;
//! * the host-facing load reducer: reset on identity change, the
//!   `subscribed` guard, the mismatch / in-flight / rejected settlements.

use rune::{Cx, El, ElementInfo, Env, Runtime};

use crate::components::migration::migration_job_verdict_panel::{
    CommitBoundary, JobProgress, UncertaintyReason, VerdictSeverity,
};
use crate::i18n::{has, t};
use crate::views::{
    derive_migration_job_verdict, is_migration_job_in_flight, latest_apply_job, terminal_job,
    DataTransferTerminalResults, JobRecoveryResult, JobState, MigrationJobVerdictInput,
    TerminalJobDetails, TerminalJobView, TerminalResultsState,
};

const DESIGN: (f32, f32) = (1440.0, 816.0);

/// Host state: the strip's props plus the two `useState` slots the host
/// copies out of [`TerminalResultsState`] each frame.
#[derive(Clone)]
struct Probe {
    jobs: Vec<TerminalJobView>,
    exclude_job_id: Option<String>,
    hide: bool,
    loaded: Option<(String, TerminalJobDetails)>,
    failed_job_id: Option<String>,
    dark: bool,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            jobs: vec![],
            exclude_job_id: None,
            hide: false,
            loaded: None,
            failed_job_id: None,
            dark: true,
        }
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    DataTransferTerminalResults::<Probe>::new(
        s.jobs.clone(),
        s.exclude_job_id.clone(),
        s.hide,
    )
    .loaded(s.loaded.clone())
    .failed_job_id(s.failed_job_id.clone())
    .build(s.dark)
}

fn runtime(probe: Probe) -> Runtime<Probe> {
    let dark = probe.dark;
    let mut rt = Runtime::new(
        probe,
        view,
        DESIGN,
        Env {
            system_dark: dark,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Probe>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn texts(rt: &Runtime<Probe>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

fn attrs_label(rt: &Runtime<Probe>, key: &str) -> String {
    element(rt, key).label.unwrap_or_default()
}

// -----------------------------------------------------------------------
// Fixtures.
// -----------------------------------------------------------------------

/// One `JobView` projection with the fields the caller overrides; the rest
/// are the "settled apply" defaults (`kind: 'dataTransferApply'`, no
/// outcome, no error, no progress).
fn job_view(job_id: &str, state: JobState, created_at: f64) -> TerminalJobView {
    TerminalJobView {
        job_id: job_id.to_owned(),
        kind: "dataTransferApply".to_owned(),
        state,
        created_at,
        updated_at: created_at,
        effect_outcome: None,
        cancel_requested: false,
        error: None,
        progress: JobProgress::default(),
        artifact_ids: Vec::new(),
    }
}

fn boundary(stage: &str, evidence: Vec<&str>) -> CommitBoundary {
    CommitBoundary {
        stage_id: stage.to_owned(),
        committed_at: "2026-11-19T00:00:00Z".to_owned(),
        evidence: evidence.into_iter().map(str::to_owned).collect(),
        operation_id: None,
    }
}

/// A clean completed restore: one verified boundary, 1200 committed rows,
/// plan identity, recovery that resumed after verification.
fn clean_completed_details(job_id: &str) -> TerminalJobDetails {
    TerminalJobDetails {
        job: TerminalJobView {
            job_id: job_id.to_owned(),
            kind: "dataTransferApply".to_owned(),
            state: JobState::Succeeded,
            created_at: 500.0,
            updated_at: 900.0,
            effect_outcome: Some("completed".to_owned()),
            cancel_requested: false,
            error: None,
            progress: JobProgress {
                committed: Some("1200".to_owned()),
                unknown: Some("0".to_owned()),
                ..JobProgress::default()
            },
            artifact_ids: vec!["artifact-1".to_owned()],
        },
        plan_id: Some("plan-7".to_owned()),
        plan_digest: Some("digest-9f2a".to_owned()),
        selection_revision: Some(3),
        commit_boundaries: vec![boundary("stage-1", vec!["EVIDENCE_ROWS"])],
        recovery: Some(JobRecoveryResult {
            verdict: "resumeAfterVerify".to_owned(),
            resume_through: Some(1200),
            reason_code: None,
        }),
    }
}

// -----------------------------------------------------------------------
// The nothing-rendered gate.
// -----------------------------------------------------------------------

#[test]
fn the_strip_renders_nothing_without_a_terminal_job() {
    // No applies at all.
    let mut probe = Probe::default();
    probe.jobs = vec![job_view("job-1", JobState::Succeeded, 500.0)];
    probe.jobs[0].kind = "dataTransferExport".to_owned();
    let rt = runtime(probe);
    assert!(!has_key(&rt, "data-transfer-result"), "no apply renders nothing");

    // The only apply is still in flight.
    let mut probe = Probe::default();
    probe.jobs = vec![job_view("job-1", JobState::Running, 500.0)];
    let rt = runtime(probe);
    assert!(
        !has_key(&rt, "data-transfer-result"),
        "an in-flight apply renders nothing"
    );

    // The apply is excluded by excludeJobId.
    let mut probe = Probe::default();
    probe.jobs = vec![job_view("job-1", JobState::Succeeded, 500.0)];
    probe.exclude_job_id = Some("job-1".to_owned());
    let rt = runtime(probe);
    assert!(!has_key(&rt, "data-transfer-result"), "an excluded apply renders nothing");

    // hide is set.
    let mut probe = Probe::default();
    probe.jobs = vec![job_view("job-1", JobState::Succeeded, 500.0)];
    probe.hide = true;
    let rt = runtime(probe);
    assert!(!has_key(&rt, "data-transfer-result"), "hide renders nothing");
}

#[test]
fn the_latest_apply_wins_and_ties_keep_the_first() {
    let jobs = vec![
        job_view("old-1", JobState::Succeeded, 100.0),
        job_view("new-1", JobState::Succeeded, 500.0),
        job_view("old-2", JobState::Failed, 200.0),
    ];
    assert_eq!(latest_apply_job(&jobs).map(|j| j.job_id.as_str()), Some("new-1"));
    // A tie on createdAt keeps the earlier job: React's strict `>` never
    // replaces the accumulator on equality.
    let tied = vec![
        job_view("first", JobState::Succeeded, 500.0),
        job_view("second", JobState::Succeeded, 500.0),
    ];
    assert_eq!(
        latest_apply_job(&tied).map(|j| j.job_id.as_str()),
        Some("first")
    );
    // terminal_job: an excluded latest falls to null, not to the runner-up.
    let jobs = vec![
        job_view("older", JobState::Succeeded, 100.0),
        job_view("newest", JobState::Succeeded, 500.0),
    ];
    assert_eq!(
        terminal_job(&jobs, Some("newest"), false).map(|j| j.job_id.as_str()),
        None,
        "excludeJobId removes the terminal job entirely"
    );
    assert_eq!(
        terminal_job(&jobs, None, false).map(|j| j.job_id.as_str()),
        Some("newest")
    );
}

// -----------------------------------------------------------------------
// The uncompleted surface.
// -----------------------------------------------------------------------

#[test]
fn while_details_load_the_surface_is_explicitly_uncompleted() {
    let mut probe = Probe::default();
    probe.jobs = vec![job_view("job-9", JobState::Succeeded, 500.0)];
    let rt = runtime(probe);

    let root = element(&rt, "data-transfer-result");
    let [x, y, w, h] = root.rect;
    assert!(w > 100.0 && w < DESIGN.0 + 0.5, "the section is laid out: {w}x{h} at {x},{y}");
    assert!(h >= 12.0, "px-4 py-3 gives the section body: {h}");

    let status = element(&rt, "data-transfer-result-status");
    assert_eq!(status.role, "status", "the p carries role=status");
    assert_eq!(
        status.text.as_deref(),
        Some(t("transfer.job.detailsLoading")),
        "loading text while the query runs"
    );
    assert!(
        !texts(&rt).iter().any(|text| text == t("transfer.job.detailsUnavailable")),
        "the failed text stays hidden"
    );

    let attrs = attrs_label(&rt, "data-transfer-result-attrs");
    assert!(attrs.contains("job_id=job-9"), "the terminal job id: {attrs}");
    assert!(attrs.contains("restored=true"), "{attrs}");
    assert!(attrs.contains("verdict_severity=uncertain"), "{attrs}");
    assert!(attrs.contains("completed=false"), "{attrs}");
    assert!(attrs.contains("replayed=false"), "{attrs}");
    assert!(attrs.contains("cancel_disposition=none"), "{attrs}");
    assert!(attrs.contains("requires_reconcile=true"), "{attrs}");
}

#[test]
fn a_failed_query_swaps_the_status_line_to_details_unavailable() {
    let mut probe = Probe::default();
    probe.jobs = vec![job_view("job-9", JobState::Succeeded, 500.0)];
    probe.failed_job_id = Some("job-9".to_owned());
    let rt = runtime(probe);

    let status = element(&rt, "data-transfer-result-status");
    assert_eq!(
        status.text.as_deref(),
        Some(t("transfer.job.detailsUnavailable")),
        "the failed branch text"
    );
    // The surface itself stays explicitly uncompleted.
    let attrs = attrs_label(&rt, "data-transfer-result-attrs");
    assert!(attrs.contains("verdict_severity=uncertain") && attrs.contains("completed=false"), "{attrs}");
}

#[test]
fn a_failed_marker_for_another_job_still_shows_loading() {
    let mut probe = Probe::default();
    probe.jobs = vec![job_view("job-9", JobState::Succeeded, 500.0)];
    probe.failed_job_id = Some("job-other".to_owned());
    let rt = runtime(probe);
    let status = element(&rt, "data-transfer-result-status");
    assert_eq!(
        status.text.as_deref(),
        Some(t("transfer.job.detailsLoading")),
        "React's failedJobId === terminalJob.jobId gate"
    );
}

#[test]
fn a_stale_loaded_job_still_shows_the_uncompleted_surface() {
    // loaded for another job id than the terminal one: React's
    // `loaded?.jobId === terminalJob.jobId ? loaded.details : null`.
    let mut probe = Probe::default();
    probe.jobs = vec![job_view("job-9", JobState::Succeeded, 500.0)];
    probe.loaded = Some(("job-older".to_owned(), clean_completed_details("job-older")));
    let rt = runtime(probe);
    let status = element(&rt, "data-transfer-result-status");
    assert_eq!(
        status.text.as_deref(),
        Some(t("transfer.job.detailsLoading")),
        "a loaded id that is not the terminal id is no details"
    );
}

// -----------------------------------------------------------------------
// The restored surface.
// -----------------------------------------------------------------------

#[test]
fn a_clean_completed_restore_renders_the_title_and_the_panel() {
    let mut probe = Probe::default();
    probe.jobs = vec![job_view("job-9", JobState::Succeeded, 500.0)];
    probe.loaded = Some(("job-9".to_owned(), clean_completed_details("job-9")));
    let rt = runtime(probe);

    let title = element(&rt, "data-transfer-result-title");
    assert_eq!(title.text.as_deref(), Some(t("transfer.job.restoredTitle")));

    // The shared panel mounts under its own prefix.
    let panel = element(&rt, "data-transfer-job-verdict");
    assert!(panel.rect[2] > 100.0, "the panel is laid out: {:?}", panel.rect);
    assert!(has_key(&rt, "data-transfer-job-verdict-status"), "status line");
    assert_eq!(
        element(&rt, "data-transfer-job-verdict-status").text.as_deref(),
        Some(t("migration.verdict.ok")),
        "the derived severity is ok"
    );
    assert!(has_key(&rt, "data-transfer-job-progress"), "the progress row mounts");
    assert!(
        texts(&rt).iter().any(|text| text.contains("1200")),
        "the committed count reaches the panel: texts={:?}",
        texts(&rt)
    );
    assert!(has_key(&rt, "data-transfer-job-artifact"), "the artifact row mounts");

    // The root mirror carries the derived verdict and the plan identity.
    let attrs = attrs_label(&rt, "data-transfer-result-attrs");
    assert!(attrs.contains("job_id=job-9"), "{attrs}");
    assert!(attrs.contains("restored=true"), "{attrs}");
    assert!(attrs.contains("verdict_severity=ok"), "{attrs}");
    assert!(attrs.contains("completed=true"), "{attrs}");
    assert!(attrs.contains("replayed=false"), "{attrs}");
    assert!(attrs.contains("cancel_disposition=none"), "{attrs}");
    assert!(attrs.contains("requires_reconcile=false"), "{attrs}");
    assert!(attrs.contains("plan_id=plan-7"), "{attrs}");

    // The panel's own mirror carries the same verdict facts.
    let panel_attrs = attrs_label(&rt, "data-transfer-job-verdict-attrs");
    assert!(
        panel_attrs.contains("severity=ok") && panel_attrs.contains("cancel=none"),
        "{panel_attrs}"
    );

    // No uncompleted status line on the restored branch.
    assert!(
        !has_key(&rt, "data-transfer-result-status"),
        "the uncompleted p is gone"
    );
}

#[test]
fn the_restored_surface_stays_inside_the_window_in_both_themes() {
    for dark in [true, false] {
        let mut probe = Probe::default();
        probe.dark = dark;
        probe.jobs = vec![job_view("job-9", JobState::Succeeded, 500.0)];
        probe.loaded = Some(("job-9".to_owned(), clean_completed_details("job-9")));
        let rt = runtime(probe);
        let outside: Vec<String> = rt
            .elements()
            .iter()
            .filter(|e| {
                let r = e.rect;
                r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > DESIGN.0 + 0.5 || r[1] + r[3] > DESIGN.1 + 0.5
            })
            .map(|e| e.key.clone().unwrap_or_else(|| e.path.clone()))
            .collect();
        assert!(
            outside.is_empty(),
            "dark={dark}: {} node(s) outside {}x{}: {outside:?}",
            outside.len(),
            DESIGN.0,
            DESIGN.1
        );
    }
}

// -----------------------------------------------------------------------
// The T561 placeholder verdict fold (lib/migrationJobVerdict.ts).
// -----------------------------------------------------------------------

fn verdict_input<'a>(
    state: JobState,
    effect_outcome: Option<&'a str>,
    boundaries: &'a [CommitBoundary],
) -> MigrationJobVerdictInput<'a> {
    MigrationJobVerdictInput {
        state,
        effect_outcome,
        cancel_requested: false,
        commit_boundaries: Some(boundaries),
        recovery_verdict: None,
        recovery_reason: None,
        error: None,
        committed_rows: None,
        unknown_rows: None,
    }
}

#[test]
fn a_completed_outcome_alone_is_not_a_completed_verdict() {
    // Rows without a boundary behind them are the canonical evidence gap:
    // completed outcome + committed rows + no boundaries fails closed.
    let verdict = derive_migration_job_verdict(&MigrationJobVerdictInput {
        state: JobState::Succeeded,
        effect_outcome: Some("completed"),
        cancel_requested: false,
        commit_boundaries: Some(&[]),
        recovery_verdict: None,
        recovery_reason: None,
        error: None,
        committed_rows: Some(1200),
        unknown_rows: Some(0),
    });
    assert_eq!(verdict.severity, VerdictSeverity::Uncertain);
    assert!(!verdict.completed, "uncertainty blocks the clean success");
    assert_eq!(verdict.uncertainty, UncertaintyReason::MissingEvidence);
    assert!(verdict.requires_reconcile);
    assert_eq!(verdict.verified_boundaries, 0);
    assert_eq!(verdict.unverified_boundaries, 0);
}

#[test]
fn an_unverified_boundary_fails_closed_even_on_a_completed_outcome() {
    let boundaries = vec![boundary("stage-1", vec![])];
    let verdict = derive_migration_job_verdict(&verdict_input(
        JobState::Succeeded,
        Some("completed"),
        &boundaries,
    ));
    assert_eq!(verdict.uncertainty, UncertaintyReason::MissingEvidence);
    assert!(!verdict.completed);
    assert_eq!(verdict.unverified_boundaries, 1);
    assert_eq!(verdict.verified_boundaries, 0);
    assert_eq!(verdict.severity, VerdictSeverity::Uncertain);
}

#[test]
fn unknown_rows_without_a_boundary_are_missing_evidence() {
    let verdict = derive_migration_job_verdict(&MigrationJobVerdictInput {
        state: JobState::Failed,
        effect_outcome: Some("rolledBack"),
        cancel_requested: false,
        commit_boundaries: Some(&[]),
        recovery_verdict: None,
        recovery_reason: None,
        error: None,
        committed_rows: Some(0),
        unknown_rows: Some(7),
    });
    assert_eq!(verdict.uncertainty, UncertaintyReason::MissingEvidence);
    assert!(verdict.requires_reconcile);
}

#[test]
fn effect_outcome_unknown_and_recovery_verdicts_fail_closed() {
    let out = derive_migration_job_verdict(&verdict_input(
        JobState::Failed,
        Some("unknown"),
        &[boundary("s", vec!["EVIDENCE_ROWS"])],
    ));
    assert_eq!(out.uncertainty, UncertaintyReason::EffectOutcomeUnknown);

    // requireManualReview outranks the evidence-only branches.
    let mut input = verdict_input(JobState::Failed, Some("rolledBack"), &[]);
    input.recovery_verdict = Some("requireManualReview");
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.uncertainty, UncertaintyReason::ManualReviewRequired);
    assert_eq!(out.severity, VerdictSeverity::Uncertain);

    // A plain reject.
    let mut input = verdict_input(JobState::Failed, Some("rolledBack"), &[]);
    input.recovery_verdict = Some("reject");
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.uncertainty, UncertaintyReason::RecoveryRejected);

    // An unknown verdict string is neither resumeAfterVerify nor notExecuted.
    let mut input = verdict_input(JobState::Failed, Some("rolledBack"), &[]);
    input.recovery_verdict = Some("somethingNew");
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.uncertainty, UncertaintyReason::RecoveryRejected);
}

#[test]
fn no_checkpoint_is_matched_case_insensitively() {
    let mut input = verdict_input(JobState::Succeeded, Some("completed"), &[]);
    input.recovery_verdict = Some("resumeAfterVerify");
    input.recovery_reason = Some("NO CHECKPOINT was recorded for this job");
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.uncertainty, UncertaintyReason::NoCheckpoint);
    assert!(!out.completed);
}

#[test]
fn not_executed_clears_the_fence_only_for_the_two_proofless_cases() {
    // Restart before dispatch: notStarted + notDispatchedAfterRestart.
    let mut input = verdict_input(JobState::Cancelled, Some("notStarted"), &[]);
    input.recovery_verdict = Some("notExecuted");
    input.recovery_reason = Some("notDispatchedAfterRestart");
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.uncertainty, UncertaintyReason::None);
    assert!(!out.completed, "notStarted is not a completion");
    assert_eq!(out.severity, VerdictSeverity::Partial, "the pickSeverity quirk: none -> partial");

    // A confirmed no-write rollback.
    let mut input = verdict_input(JobState::Cancelled, Some("rolledBack"), &[]);
    input.recovery_verdict = Some("notExecuted");
    input.recovery_reason = Some("noCommittedWrites");
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.uncertainty, UncertaintyReason::None);

    // notExecuted with any other reason is a rejection.
    let mut input = verdict_input(JobState::Cancelled, Some("partiallyApplied"), &[]);
    input.recovery_verdict = Some("notExecuted");
    input.recovery_reason = Some("anythingElse");
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.uncertainty, UncertaintyReason::RecoveryRejected);

    // ...and write evidence contradicts even the two proofless cases.
    let mut input = verdict_input(JobState::Cancelled, Some("notStarted"), &[]);
    input.recovery_verdict = Some("notExecuted");
    input.recovery_reason = Some("notDispatchedAfterRestart");
    input.committed_rows = Some(1);
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.uncertainty, UncertaintyReason::RecoveryRejected);
}

#[test]
fn cancel_request_dispositions_settle_the_four_ways() {
    // Requested while in flight: the race has not landed.
    let mut input = verdict_input(JobState::Running, Some("completed"), &[]);
    input.cancel_requested = true;
    let out = derive_migration_job_verdict(&input);
    assert_eq!(
        out.cancel_disposition.as_str(),
        "requestedInFlight",
        "{:?}",
        out.cancel_disposition
    );
    assert_eq!(out.severity, VerdictSeverity::Partial);

    // The write finished before the cancel landed: settledCompleted.
    let mut input = verdict_input(JobState::Cancelled, Some("completed"), &[]);
    input.cancel_requested = true;
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.cancel_disposition.as_str(), "settledCompleted");
    assert_eq!(out.severity, VerdictSeverity::Ok, "settledCompleted is not a failure");

    // Rolled back under cancel: a failed severity.
    let mut input = verdict_input(JobState::Cancelled, Some("rolledBack"), &[]);
    input.cancel_requested = true;
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.cancel_disposition.as_str(), "settledRolledBack");
    assert_eq!(out.severity, VerdictSeverity::Failed);

    // An undecidable outcome under cancel: uncertain and needs reconcile.
    let mut input = verdict_input(JobState::Cancelled, Some("unknown"), &[]);
    input.cancel_requested = true;
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.cancel_disposition.as_str(), "settledUnknown");
    assert!(out.requires_reconcile, "settledUnknown forces a reconcile");
    assert_eq!(out.severity, VerdictSeverity::Uncertain);

    // notStarted with no boundary and no outcome at all.
    let mut input = verdict_input(JobState::Cancelled, None, &[]);
    input.cancel_requested = true;
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.cancel_disposition.as_str(), "notStarted");
    assert_eq!(out.severity, VerdictSeverity::Failed);

    // The same absent outcome but a committed boundary contradicts it.
    let boundaries = vec![boundary("stage-1", vec!["EVIDENCE_ROWS"])];
    let mut input = verdict_input(JobState::Cancelled, None, &boundaries);
    input.cancel_requested = true;
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.cancel_disposition.as_str(), "settledPartially");
}

#[test]
fn a_failed_state_or_a_terminal_error_is_the_failed_severity() {
    let out = derive_migration_job_verdict(&verdict_input(JobState::Failed, Some("rolledBack"), &[]));
    assert_eq!(out.severity, VerdictSeverity::Failed);

    // An error with a non-completed outcome is the failed severity too.
    let mut input = verdict_input(JobState::Succeeded, Some("rolledBack"), &[]);
    input.error = Some("boom");
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.severity, VerdictSeverity::Failed, "an error outranks a non-completed outcome");

    // React's pickSeverity order: `completed` returns 'ok' *before* the
    // hasError check, so an error on a certifiably-completed write does not
    // fail the verdict — the write finished, the error text is not the
    // outcome. Transliterated as-is.
    let mut input = verdict_input(JobState::Succeeded, Some("completed"), &[]);
    input.error = Some("boom");
    let out = derive_migration_job_verdict(&input);
    assert_eq!(out.severity, VerdictSeverity::Ok, "React: completed outranks hasError");
}

// -----------------------------------------------------------------------
// The host-facing load reducer (the useEffect as a state machine).
// -----------------------------------------------------------------------

#[test]
fn begin_load_resets_both_state_slots() {
    let mut state = TerminalResultsState::default();
    state.loaded = Some(("job-1".to_owned(), clean_completed_details("job-1")));
    state.failed_job_id = Some("job-1".to_owned());
    state.begin_load();
    assert_eq!(state, TerminalResultsState::default(), "both useState slots clear");
}

#[test]
fn finish_load_applies_a_settled_details_reply_for_the_same_identity() {
    let captured = job_view("job-9", JobState::Succeeded, 500.0);
    let mut state = TerminalResultsState::default();
    let details = clean_completed_details("job-9");
    let applied = state.finish_load(&captured, Some(&captured), Ok(details.clone()));
    assert!(applied);
    let (job_id, loaded) = state.loaded.as_ref().expect("loaded is set");
    assert_eq!(job_id, "job-9");
    assert_eq!(loaded, &details);
    assert_eq!(state.failed_job_id, None);
}

#[test]
fn finish_load_rejects_a_reply_for_a_different_job_id() {
    let captured = job_view("job-9", JobState::Succeeded, 500.0);
    let mut state = TerminalResultsState::default();
    let wrong_job = clean_completed_details("job-other");
    let applied = state.finish_load(&captured, Some(&captured), Ok(wrong_job));
    assert!(applied, "the settlement is applied — to the failed slot");
    assert_eq!(state.loaded, None);
    assert_eq!(state.failed_job_id.as_deref(), Some("job-9"));
}

#[test]
fn finish_load_rejects_a_reply_whose_details_job_is_in_flight() {
    let captured = job_view("job-9", JobState::Succeeded, 500.0);
    let mut state = TerminalResultsState::default();
    let mut details = clean_completed_details("job-9");
    details.job.state = JobState::Running;
    let applied = state.finish_load(&captured, Some(&captured), Ok(details));
    assert!(applied);
    assert_eq!(state.loaded, None);
    assert_eq!(state.failed_job_id.as_deref(), Some("job-9"));
}

#[test]
fn finish_load_records_a_rejection() {
    let captured = job_view("job-9", JobState::Succeeded, 500.0);
    let mut state = TerminalResultsState::default();
    assert!(state.finish_load(&captured, Some(&captured), Err(())));
    assert_eq!(state.loaded, None);
    assert_eq!(state.failed_job_id.as_deref(), Some("job-9"));
}

#[test]
fn a_stale_settlement_is_dropped_untouched() {
    let captured = job_view("job-9", JobState::Succeeded, 500.0);

    // The terminal job moved on (same id, new updatedAt): the effect
    // re-ran, so this settlement is unsubscribed.
    let moved_on = job_view("job-9", JobState::Succeeded, 900.0);
    let mut state = TerminalResultsState::default();
    assert!(!state.finish_load(&captured, Some(&moved_on), Ok(clean_completed_details("job-9"))));
    assert_eq!(state, TerminalResultsState::default(), "nothing is recorded");

    // The terminal job went away entirely.
    let mut state = TerminalResultsState::default();
    assert!(!state.finish_load(&captured, None, Err(())));
    assert_eq!(state, TerminalResultsState::default());
}

#[test]
fn is_migration_job_in_flight_covers_only_queued_and_running() {
    assert!(is_migration_job_in_flight(&JobState::Queued));
    assert!(is_migration_job_in_flight(&JobState::Running));
    for state in [JobState::Succeeded, JobState::Failed, JobState::Cancelled] {
        assert!(!is_migration_job_in_flight(&state));
    }
}

// -----------------------------------------------------------------------
// Locale coverage (the useLocaleDomains(['sync']) gate, as a table check).
// -----------------------------------------------------------------------

#[test]
fn the_three_locale_keys_the_component_reads_exist() {
    assert!(has("transfer.job.restoredTitle"));
    assert!(has("transfer.job.detailsLoading"));
    assert!(has("transfer.job.detailsUnavailable"));
}
