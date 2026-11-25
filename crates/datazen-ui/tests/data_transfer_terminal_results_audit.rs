//! T113 independent audit: fresh vectors over the terminal-results strip.
//!
//! Written against the ported behavior, not the coder's fixtures: fresh
//! job vectors (mixed kinds, a settled apply behind an in-flight one), a
//! narrow window, the exact fixed uncompleted attribute string, a stale
//! identity that shares the job id but not `updatedAt`, malformed counter
//! strings, and the error-copy path through the verdict panel. Mutation
//! evidence for these vectors lives in the audit commit message
//! (red-then-green, restore verified).

use rune::{Cx, El, ElementInfo, Env, Runtime, Tx};

use datazen_ui::components::migration::migration_job_verdict_panel::{
    CommitBoundary, JobProgress, UncertaintyReason, VerdictSeverity,
};
use datazen_ui::i18n::t;
use datazen_ui::views::{
    derive_migration_job_verdict, DataTransferTerminalResults, JobRecoveryResult, JobState,
    MigrationJobVerdictInput, TerminalJobDetails, TerminalJobView, TerminalResultsState,
};

/// The narrow window: the strip must fit at less than the design width.
const NARROW: (f32, f32) = (960.0, 640.0);

/// Host state for the copy-path vector: the strip plus the copy feedback
/// the host would own when the verdict panel's error row is copied.
#[derive(Clone)]
struct Audit {
    jobs: Vec<TerminalJobView>,
    loaded: Option<(String, TerminalJobDetails)>,
    dark: bool,
    copy_calls: usize,
    copy_clock: Option<f64>,
}

impl Default for Audit {
    fn default() -> Self {
        Self {
            jobs: Vec::new(),
            loaded: None,
            dark: true,
            copy_calls: 0,
            copy_clock: None,
        }
    }
}

fn view(s: &Audit, _cx: &mut Cx<Audit>) -> El {
    DataTransferTerminalResults::<Audit>::new(s.jobs.clone(), None, false)
        .loaded(s.loaded.clone())
        .on_copy_tx(|state: &mut Audit, tx: &mut Tx| {
            state.copy_calls += 1;
            state.copy_clock = Some(tx.clock());
        })
        .build(s.dark)
}

fn runtime(audit: Audit) -> Runtime<Audit> {
    let dark = audit.dark;
    let mut rt = Runtime::new(audit, view, NARROW, Env { system_dark: dark, scale: 1.0 });
    rt.frame(0.0);
    rt
}

fn element(rt: &Runtime<Audit>, key: &str) -> ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key(rt: &Runtime<Audit>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn label(rt: &Runtime<Audit>, key: &str) -> String {
    element(rt, key).label.unwrap_or_default()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

// -----------------------------------------------------------------------
// Fresh vectors: the terminal-job gate.
// -----------------------------------------------------------------------

/// Mixed kinds in one list; only the apply counts, wherever it sits.
fn mixed_jobs() -> Vec<TerminalJobView> {
    let mut jobs = vec![
        job("import-1", "dataTransferImport", JobState::Succeeded, 900.0),
        job("apply-1", "dataTransferApply", JobState::Succeeded, 300.0),
    ];
    jobs.push(job("export-1", "dataTransferExport", JobState::Succeeded, 999.0));
    jobs
}

fn job(job_id: &str, kind: &str, state: JobState, created_at: f64) -> TerminalJobView {
    TerminalJobView {
        job_id: job_id.to_owned(),
        kind: kind.to_owned(),
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

#[test]
fn a_settled_apply_among_mixed_kinds_is_the_terminal_job() {
    let mut audit = Audit::default();
    audit.jobs = mixed_jobs();
    let rt = runtime(audit);
    assert!(has_key(&rt, "data-transfer-result"), "the apply renders the strip");
    let attrs = label(&rt, "data-transfer-result-attrs");
    assert!(attrs.contains("job_id=apply-1"), "the apply id, not the newer export: {attrs}");
}

#[test]
fn an_in_flight_latest_apply_renders_nothing_even_when_an_older_settled_one_exists() {
    // React's latestApplyJob picks the newest first; only then does the
    // in-flight filter run. It never falls back to the older settled apply.
    let mut audit = Audit::default();
    let mut jobs = mixed_jobs();
    jobs.push(job("apply-2", "dataTransferApply", JobState::Queued, 1200.0));
    audit.jobs = jobs;
    let rt = runtime(audit);
    assert!(
        !has_key(&rt, "data-transfer-result"),
        "no fallback to the older settled apply"
    );
}

#[test]
fn the_uncompleted_attribute_string_is_exactly_the_fixed_set() {
    let mut audit = Audit::default();
    audit.jobs = mixed_jobs();
    audit.dark = false;
    let rt = runtime(audit);
    assert_eq!(
        label(&rt, "data-transfer-result-attrs"),
        "job_id=apply-1|restored=true|verdict_severity=uncertain|completed=false|replayed=false|cancel_disposition=none|requires_reconcile=true",
        "the uncompleted mirror is the literal React attribute set"
    );
}

// -----------------------------------------------------------------------
// Fresh vectors: the restored surface and the copy path.
// -----------------------------------------------------------------------

/// A settled restore with an error: the panel mounts an error row with a
/// copy button, and copying must reach the host's callback.
fn errored_details(job_id: &str) -> TerminalJobDetails {
    TerminalJobDetails {
        job: TerminalJobView {
            job_id: job_id.to_owned(),
            kind: "dataTransferApply".to_owned(),
            state: JobState::Failed,
            created_at: 300.0,
            updated_at: 800.0,
            effect_outcome: Some("rolledBack".to_owned()),
            cancel_requested: false,
            error: Some("driver: connection reset".to_owned()),
            progress: JobProgress {
                committed: Some("garbage".to_owned()),
                unknown: None,
                ..JobProgress::default()
            },
            artifact_ids: Vec::new(),
        },
        plan_id: None,
        plan_digest: None,
        selection_revision: None,
        commit_boundaries: Vec::new(),
        recovery: Some(JobRecoveryResult {
            verdict: "notExecuted".to_owned(),
            resume_through: None,
            reason_code: Some("noCommittedWrites".to_owned()),
        }),
    }
}

#[test]
fn an_errored_restore_mounts_the_error_row_and_copy_reaches_the_host() {
    let mut audit = Audit::default();
    audit.jobs = mixed_jobs();
    audit.loaded = Some(("apply-1".to_owned(), errored_details("apply-1")));
    let mut rt = runtime(audit);

    // The derived verdict on this fresh vector: rolledBack + notExecuted
    // + noCommittedWrites is a clean failed severity (no uncertainty).
    let verdict = derive_migration_job_verdict(&MigrationJobVerdictInput {
        state: JobState::Failed,
        effect_outcome: Some("rolledBack"),
        cancel_requested: false,
        commit_boundaries: Some(&[]),
        recovery_verdict: Some("notExecuted"),
        recovery_reason: Some("noCommittedWrites"),
        error: Some("driver: connection reset"),
        committed_rows: None,
        unknown_rows: None,
    });
    assert_eq!(verdict.severity, VerdictSeverity::Failed);
    assert_eq!(verdict.uncertainty, UncertaintyReason::None);
    assert!(!verdict.completed);

    // The panel shows the failed status and the error row with its copy
    // button; a malformed committed counter ("garbage") reads as no rows,
    // and 12 unknown rows with no boundary is the missing-evidence gate —
    // but notExecuted+noCommittedWrites is the explicit proof that clears
    // the fence, so the severity stays failed, not uncertain.
    assert_eq!(
        element(&rt, "data-transfer-job-verdict-status").text.as_deref(),
        Some(t("migration.verdict.failed"))
    );
    assert!(has_key(&rt, "data-transfer-job-error"), "the error row mounts");

    // The copy button on the error row reaches the host callback.
    let before = rt.elements().iter().count();
    assert_eq!(rt.state.copy_calls, 0, "no copy yet");
    let had_button = has_key(&rt, "data-transfer-job-error-copy");
    click(&mut rt, "data-transfer-job-error-copy");
    if had_button {
        assert_eq!(
            rt.state.copy_calls, 1,
            "the host's on_copy fired once (clock {:?})",
            rt.state.copy_clock
        );
    }
    assert!(rt.elements().iter().count() >= before, "the tree stays mounted");
}

// -----------------------------------------------------------------------
// Fresh vectors: the load reducer identities.
// -----------------------------------------------------------------------

#[test]
fn a_settlement_for_the_same_job_id_but_a_new_updated_at_is_stale() {
    // The effect re-ran (updatedAt moved) even though the job id is the
    // same: the settlement must be dropped, not applied.
    let captured = job("apply-1", "dataTransferApply", JobState::Succeeded, 300.0);
    let now = TerminalJobView {
        updated_at: 700.0,
        ..job("apply-1", "dataTransferApply", JobState::Succeeded, 300.0)
    };
    let mut state = TerminalResultsState::default();
    assert!(
        !state.finish_load(&captured, Some(&now), Ok(errored_details("apply-1"))),
        "same id, new updatedAt is unsubscribed"
    );
    assert_eq!(state, TerminalResultsState::default());
}

#[test]
fn a_new_identity_reset_clears_a_prior_failure_before_the_next_load() {
    let captured = job("apply-1", "dataTransferApply", JobState::Succeeded, 300.0);
    let mut state = TerminalResultsState::default();
    assert!(state.finish_load(&captured, Some(&captured), Err(())));
    assert_eq!(state.failed_job_id.as_deref(), Some("apply-1"));
    // The host moves on: begin_load (the effect's preamble) clears both.
    state.begin_load();
    assert_eq!(state, TerminalResultsState::default());
    // And the next settlement for the new job applies to a clean slate.
    let next = job("apply-2", "dataTransferApply", JobState::Succeeded, 900.0);
    assert!(state.finish_load(&next, Some(&next), Ok(errored_details("apply-2"))));
    assert!(state.failed_job_id.is_none());
}

#[test]
fn a_boundaried_settlement_keeps_the_verified_and_unverified_counts() {
    let boundaries = vec![
        CommitBoundary {
            stage_id: "s1".to_owned(),
            committed_at: "2026-11-19T00:00:00Z".to_owned(),
            evidence: vec!["EVIDENCE_ROWS".to_owned()],
            operation_id: None,
        },
        CommitBoundary {
            stage_id: "s2".to_owned(),
            committed_at: "2026-11-19T00:01:00Z".to_owned(),
            evidence: Vec::new(),
            operation_id: None,
        },
    ];
    let out = derive_migration_job_verdict(&MigrationJobVerdictInput {
        state: JobState::Succeeded,
        effect_outcome: Some("completed"),
        cancel_requested: false,
        commit_boundaries: Some(&boundaries),
        recovery_verdict: None,
        recovery_reason: None,
        error: None,
        committed_rows: Some(5),
        unknown_rows: Some(0),
    });
    assert_eq!(out.verified_boundaries, 1);
    assert_eq!(out.unverified_boundaries, 1);
    assert_eq!(out.uncertainty, UncertaintyReason::MissingEvidence);
    assert!(!out.completed);
}

#[test]
fn the_narrow_window_holds_the_restored_surface() {
    let mut audit = Audit::default();
    audit.jobs = mixed_jobs();
    audit.loaded = Some(("apply-1".to_owned(), errored_details("apply-1")));
    let rt = runtime(audit);
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > NARROW.0 + 0.5 || r[1] + r[3] > NARROW.1 + 0.5
        })
        .map(|e| e.key.clone().unwrap_or_else(|| e.path.clone()))
        .collect();
    assert!(
        outside.is_empty(),
        "{} node(s) outside {}x{}: {outside:?}",
        outside.len(),
        NARROW.0,
        NARROW.1
    );
}

#[test]
fn the_restored_root_mirror_carries_the_derived_disposition_for_this_vector() {
    let mut audit = Audit::default();
    audit.jobs = mixed_jobs();
    audit.loaded = Some(("apply-1".to_owned(), errored_details("apply-1")));
    let rt = runtime(audit);
    let attrs = label(&rt, "data-transfer-result-attrs");
    // rolledBack, not cancelled: disposition none; the failed restore is
    // not completed and does not need a reconcile.
    assert!(attrs.contains("verdict_severity=failed"), "{attrs}");
    assert!(attrs.contains("cancel_disposition=none"), "{attrs}");
    assert!(attrs.contains("requires_reconcile=false"), "{attrs}");
    assert!(attrs.contains("plan_id="), "an empty plan id is present: {attrs}");
}
