//! Independent tester audit for T106 (`MigrationJobVerdictPanel`).
//!
//! The port's own tests are broad; these probes target the branches its
//! `rendered == t(key)` comparisons cannot see and the geometry its short
//! fixture strings cannot stress:
//!
//! 1. every i18n key reachable from a verdict state resolves to localized text
//!    (a `rendered == t(key)` assertion is vacuous when `t` echo-falls-back);
//! 2. per-boundary verdict tone + evidence-gap marker polarity are observed on
//!    runtime nodes, not on helper return values;
//! 3. recovery resume-through is gated on a recovery verdict being present
//!    (React: `recoveryVerdict && through != null`), not just on `through`;
//! 4. rect-based geometry with long adversarial content at 960×640 and
//!    1440×816 in both themes — wrap_anywhere on the error/artifact rows is the
//!    only thing keeping an unbroken token inside the window.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::migration::migration_job_verdict_panel::{
    CancelDisposition, CommitBoundary, JobProgress, MigrationJobVerdict, MigrationJobVerdictPanel,
    PlanInfo, UncertaintyReason, VerdictPanelVm, VerdictSeverity,
};
use crate::i18n::{has, t};
use crate::ui::kit::kit;

const PREFIX: &str = "migration-job";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

#[derive(Clone)]
struct AuditProbe {
    vm: VerdictPanelVm,
    dark: bool,
}

impl Default for AuditProbe {
    fn default() -> Self {
        Self {
            vm: VerdictPanelVm::default(),
            dark: true,
        }
    }
}

fn view(s: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    col()
        .w_full()
        .h_full()
        .padding(8.0)
        .child(MigrationJobVerdictPanel::<AuditProbe>::new(s.vm.clone()).build(s.dark))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn is_cjk(ch: char) -> bool {
    (0x4E00..=0x9FFF).contains(&(ch as u32))
}

/// Rect-based observation independent of any value the component reports.
fn assert_inside<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
    let outside: Vec<String> = rt
        .elements()
        .iter()
        .filter(|e| {
            let r = e.rect;
            r[0] < -0.5 || r[1] < -0.5 || r[0] + r[2] > w + 0.5 || r[1] + r[3] > h + 0.5
        })
        .map(|e| format!("{} {:?}", e.key.as_deref().unwrap_or(&e.path), e.rect))
        .collect();
    assert!(
        outside.is_empty(),
        "{label}: {} node(s) outside {w}x{h}: {outside:?}",
        outside.len()
    );
}

#[test]
fn every_verdict_state_i18n_key_resolves_to_localized_text_not_a_key_echo() {
    // Keys returned from `key()` methods are NOT scanned by the crate's
    // `every_key_the_ui_asks_for_exists` test (it only matches direct quoted
    // call sites), and the port's own equality checks are vacuous under the
    // translator's echo fallback.
    let keys = [
        "migration.verdict.ok",
        "migration.verdict.partial",
        "migration.verdict.uncertain",
        "migration.verdict.failed",
        "migration.verdict.committedRows",
        "migration.verdict.replayedReceipt",
        "migration.verdict.noBoundaries",
        "migration.verdict.recoveryLabel",
        "migration.verdict.notExecuted",
        "migration.verdict.notDispatchedAfterRestart",
        "migration.verdict.resumeThrough",
        "migration.verdict.requiresReconcile",
        "migration.progress.counts",
        "migration.boundary.verified",
        "migration.boundary.unverified",
        "migration.cancel.none",
        "migration.cancel.notStarted",
        "migration.cancel.requestedInFlight",
        "migration.cancel.settledPartially",
        "migration.cancel.settledRolledBack",
        "migration.cancel.settledCompleted",
        "migration.cancel.settledUnknown",
        "migration.cancel.unknownJob",
        "migration.uncertainty.noCheckpoint",
        "migration.uncertainty.recoveryRejected",
        "migration.uncertainty.effectOutcomeUnknown",
        "migration.uncertainty.missingEvidence",
        "migration.uncertainty.manualReviewRequired",
    ];
    for key in keys {
        assert!(has(key), "{key} missing from zh-CN locale table");
        let text = t(key);
        assert_ne!(text, key, "{key} echoes the raw key (missing translation)");
        assert!(text.chars().any(is_cjk), "{key} did not resolve to localized text: {text:?}");
    }

    // The key() mirrors must agree with the exact React spellings, including
    // the two states that share the warning class but NOT the same key.
    assert_eq!(VerdictSeverity::Ok.key(), "migration.verdict.ok");
    assert_eq!(VerdictSeverity::Partial.key(), "migration.verdict.partial");
    assert_eq!(VerdictSeverity::Uncertain.key(), "migration.verdict.uncertain");
    assert_eq!(VerdictSeverity::Failed.key(), "migration.verdict.failed");
    assert_eq!(CancelDisposition::SettledUnknown.key(), "migration.cancel.settledUnknown");
    assert_eq!(
        UncertaintyReason::ManualReviewRequired.key(),
        Some("migration.uncertainty.manualReviewRequired")
    );
}

#[test]
fn boundary_verdict_tone_and_gap_marker_follow_evidence_presence() {
    let k = kit(true);
    let mut p = AuditProbe::default();
    p.vm = VerdictPanelVm {
        verdict: MigrationJobVerdict {
            // A missing-evidence verdict must NOT mark the gap when boundaries
            // are actually present — the marker belongs to the empty branch.
            uncertainty: UncertaintyReason::MissingEvidence,
            ..Default::default()
        },
        commit_boundaries: vec![
            CommitBoundary {
                stage_id: "verified".into(),
                committed_at: "t0".into(),
                evidence: vec!["EVIDENCE_ROWS".into()],
                operation_id: None,
            },
            CommitBoundary {
                stage_id: "unverified".into(),
                committed_at: "t1".into(),
                evidence: Vec::new(),
                operation_id: None,
            },
        ],
        ..Default::default()
    };
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);

    // Observe the rendered verdict-text leaf nodes by their localized text
    // (the row container itself has no text). This is independent of any
    // helper return value.
    let verified_text = t("migration.boundary.verified");
    let unverified_text = t("migration.boundary.unverified");
    let tone_of = |needle: &str| -> Option<_> {
        rt.elements()
            .into_iter()
            .find(|e| e.text.as_deref() == Some(needle))
            .and_then(|e| e.text_color)
    };
    assert_eq!(tone_of(verified_text), Some(k.c.success.fg), "verified row uses success tone");
    assert_eq!(tone_of(unverified_text), Some(k.c.warning.fg), "unverified row uses warning tone");
    assert!(has_key(&rt, &format!("{PREFIX}-boundary-verified-verified-true")));
    assert!(has_key(&rt, &format!("{PREFIX}-boundary-unverified-verified-false")));
    assert!(
        !has_key(&rt, &format!("{PREFIX}-boundaries-empty-attrs-true")),
        "a populated boundary list never carries the empty-list evidence-gap marker"
    );
}

#[test]
fn resume_through_is_gated_on_a_recovery_verdict_being_present() {
    // React: `recoveryVerdict && through !== null && through !== undefined`.
    // A through value with no verdict must mount no resume line.
    let mut p = AuditProbe::default();
    p.vm.recovery_resume_through = Some(3);
    p.vm.recovery_reason = Some("something".into());
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(!has_key(&rt, &format!("{PREFIX}-recovery-resume-through")));
    assert!(has_key(&rt, &format!("{PREFIX}-recovery-reason")), "reason still renders on its own");

    // Adding the verdict mounts the line with the exact key + value.
    let mut p = AuditProbe::default();
    p.vm.recovery_verdict = Some("resumeAfterVerify".into());
    p.vm.recovery_resume_through = Some(3);
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert_eq!(
        element(&rt, &format!("{PREFIX}-recovery-resume-through")).text.as_deref(),
        Some(format!("{}: 3", t("migration.verdict.resumeThrough")).as_str())
    );
}

#[test]
fn long_unbroken_content_stays_inside_window_at_both_sizes_in_both_themes() {
    // The error message and artifact ids are single unbroken tokens — they
    // rely on wrap_anywhere + min_w(0), exactly where a forced-overflow bug
    // would surface. Everything else stays short so the test isolates
    // horizontal wrap rather than a legitimately tall, scrollable page.
    let long_error = "E".repeat(420);
    let long_artifact = format!("transfer-sql-{}", "F".repeat(300));
    let reason = "restart before dispatch";

    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut p = AuditProbe::default();
            p.dark = dark;
            p.vm = VerdictPanelVm {
                verdict: MigrationJobVerdict {
                    severity: VerdictSeverity::Failed,
                    cancel_disposition: CancelDisposition::SettledUnknown,
                    uncertainty: UncertaintyReason::ManualReviewRequired,
                    requires_reconcile: true,
                    ..Default::default()
                },
                progress: Some(JobProgress {
                    read: Some("11".into()),
                    converted: None,
                    attempted: Some("9".into()),
                    committed: Some("8".into()),
                    unknown: Some("2".into()),
                }),
                commit_boundaries: vec![CommitBoundary {
                    stage_id: "stage".into(),
                    committed_at: "t0".into(),
                    evidence: vec!["EVIDENCE_ROWS".into()],
                    operation_id: Some("op-1".into()),
                }],
                artifact_ids: vec![long_artifact.clone()],
                cancel_requested: true,
                cancel_acknowledged: false,
                replayed: true,
                recovery_verdict: Some("notExecuted".into()),
                recovery_reason: Some(reason.into()),
                recovery_resume_through: Some(4),
                error: Some(long_error.clone()),
                plan: Some(PlanInfo {
                    plan_id: Some("plan-42".into()),
                    ..Default::default()
                }),
                test_id_prefix: PREFIX.into(),
            };
            let mut rt = Runtime::new(p, view, (w, h), env());
            rt.frame(0.0);

            assert_inside(&rt, w, h, &format!("adversarial dark={dark} {w}x{h}"));
            assert!(has_key(&rt, &format!("{PREFIX}-error-text")));
            assert!(has_key(&rt, &format!("{PREFIX}-artifact")));
            // The exact unbroken strings must be the observable message text.
            assert_eq!(element(&rt, &format!("{PREFIX}-error-text")).text.as_deref(), Some(long_error.as_str()));
            assert_eq!(element(&rt, &format!("{PREFIX}-artifact")).text.as_deref(), Some(long_artifact.as_str()));
        }
    }
}
