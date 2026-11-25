//! T106: MigrationJobVerdictPanel — the shared P5 verdict surface.
//!
//! Real probes against the rendered element tree: every verdict state/copy key,
//! counter arithmetic, boundary/artifact/recovery rows, the conditional blocks,
//! the data-* mirrors rune cannot set natively, copy feedback with the pinned
//! 1500ms wake deadline, and rect-based geometry at both sizes in both themes.
//! Every assertion observes runtime elements, never a value the component
//! reports about itself.

use rune::{clipboard, col, row, Cx, El, Env, Runtime, Tx};

use datazen_ui::components::migration::migration_job_verdict_panel::{
    read_job_counter, sum_job_counters, CancelDisposition, CommitBoundary, JobProgress,
    MigrationJobVerdict, MigrationJobVerdictPanel, PlanInfo, UncertaintyReason, VerdictPanelVm,
    VerdictSeverity,
};
use datazen_ui::i18n::{has, t};
use datazen_ui::ui::kit::kit;

const PREFIX: &str = "migration-job";

fn env() -> Env {
    Env {
        system_dark: true,
        scale: 1.0,
    }
}

/// Host state. `copied_until`/`now` mirror React's `useCopyFeedback` hook; the
/// callback records `tx.clock() + 1.5` exactly as the future view will.
#[derive(Clone)]
struct Probe {
    vm: VerdictPanelVm,
    dark: bool,
    copied_until: Option<f64>,
    now: f64,
    press_clock: Option<f64>,
    copy_calls: usize,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            vm: VerdictPanelVm::default(),
            dark: true,
            copied_until: None,
            now: 0.0,
            press_clock: None,
            copy_calls: 0,
        }
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let copied = s.copied_until.map_or(false, |u| s.now < u);
    let panel = MigrationJobVerdictPanel::<Probe>::new(s.vm.clone())
        .copied(copied)
        .on_copy_tx(|st: &mut Probe, tx: &mut Tx| {
            st.copy_calls += 1;
            st.press_clock = Some(tx.clock());
            st.now = tx.clock();
            st.copied_until = Some(tx.clock() + 1.5);
        });
    let sync = row()
        .key("sync")
        .role("button")
        .label("sync")
        .w(1.0)
        .h(1.0)
        .on_press(|_s: &mut Probe| {});
    col()
        .w_full()
        .h_full()
        .padding(8.0)
        .child(sync)
        .child(panel.build(s.dark))
}

fn element<S>(rt: &Runtime<S>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn has_key<S>(rt: &Runtime<S>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn keys_with_prefix<S>(rt: &Runtime<S>, prefix: &str) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.key.clone())
        .filter(|k| k.starts_with(prefix))
        .collect()
}

fn key_count<S>(rt: &Runtime<S>, key: &str) -> usize {
    rt.elements()
        .iter()
        .filter(|e| e.key.as_deref() == Some(key))
        .count()
}

fn click<S>(rt: &mut Runtime<S>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

/// Force a rebuild after mutating `state.now` (the runtime only re-renders a
/// view on input, not on a bare frame — same technique as the copyable audit).
fn sync<S: 'static>(rt: &mut Runtime<S>) {
    click(rt, "sync");
}

fn texts<S>(rt: &Runtime<S>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

fn assert_in_window<S>(rt: &Runtime<S>, w: f32, h: f32, label: &str) {
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

/// A fully populated VM: every conditional block mounted at once.
fn loaded_vm() -> VerdictPanelVm {
    VerdictPanelVm {
        verdict: MigrationJobVerdict {
            severity: VerdictSeverity::Uncertain,
            cancel_disposition: CancelDisposition::SettledUnknown,
            completed: false,
            verified_boundaries: 2,
            unverified_boundaries: 1,
            uncertainty: UncertaintyReason::MissingEvidence,
            requires_reconcile: true,
        },
        progress: Some(JobProgress {
            read: Some("11".into()),
            converted: Some("10".into()),
            attempted: Some("9".into()),
            committed: Some("8".into()),
            unknown: Some("2".into()),
        }),
        commit_boundaries: vec![
            CommitBoundary {
                stage_id: "write-a".into(),
                committed_at: "2026-11-19T00:00:00Z".into(),
                evidence: vec!["EVIDENCE_ROWS".into()],
                operation_id: Some("op-1".into()),
            },
            CommitBoundary {
                stage_id: "write-b".into(),
                committed_at: "2026-11-19T00:00:01Z".into(),
                evidence: vec!["EVIDENCE_CHECKSUM".into()],
                operation_id: None,
            },
            CommitBoundary {
                stage_id: "write-c".into(),
                committed_at: "2026-11-19T00:00:02Z".into(),
                evidence: Vec::new(),
                operation_id: None,
            },
        ],
        artifact_ids: vec!["transfer-sql-aaaa".into(), "transfer-sql-bbbb".into()],
        cancel_requested: true,
        cancel_acknowledged: false,
        replayed: true,
        recovery_verdict: Some("notExecuted".into()),
        recovery_reason: Some("notDispatchedAfterRestart".into()),
        recovery_resume_through: Some(4),
        error: Some("boom: the exact backend detail · with unicode".into()),
        plan: Some(PlanInfo {
            plan_id: Some("plan-42".into()),
            plan_digest: Some("sha-abc".into()),
            selection_revision: Some(7),
        }),
        test_id_prefix: PREFIX.into(),
    }
}

#[test]
fn all_four_severities_render_the_exact_status_text_and_tone() {
    let k = kit(true);
    let cases = [
        (VerdictSeverity::Ok, "migration.verdict.ok", "ok"),
        (VerdictSeverity::Partial, "migration.verdict.partial", "partial"),
        (VerdictSeverity::Uncertain, "migration.verdict.uncertain", "uncertain"),
        (VerdictSeverity::Failed, "migration.verdict.failed", "failed"),
    ];
    for (severity, key, name) in cases {
        let mut p = Probe::default();
        p.vm.verdict.severity = severity;
        let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
        rt.frame(0.0);

        assert!(has_key(&rt, &format!("{PREFIX}-verdict")), "{name}: root");
        let status = element(&rt, &format!("{PREFIX}-verdict-status"));
        assert_eq!(status.role, "status", "{name}: role");
        assert_eq!(status.text.as_deref(), Some(t(key)), "{name}: exact status key text");
        assert_ne!(status.text.as_deref(), Some(key), "{name}: no key echo");
        assert!(has(key), "{name}: {key} in locale table");

        let expected = match severity {
            VerdictSeverity::Ok => k.c.success.fg,
            VerdictSeverity::Partial | VerdictSeverity::Uncertain => k.c.warning.fg,
            VerdictSeverity::Failed => k.c.danger.fg,
        };
        assert_eq!(status.text_color, Some(expected), "{name}: severity tone");

        let attrs = element(&rt, &format!("{PREFIX}-verdict-attrs"))
            .label
            .unwrap_or_default();
        assert!(
            attrs.contains(&format!("severity={name}")),
            "{name}: data-severity mirror: {attrs}"
        );
        assert_eq!(severity.as_str(), name, "{name}: as_str mirror");
        assert_eq!(severity.key(), key, "{name}: key mirror");
    }
}

#[test]
fn cancel_dispositions_render_exact_keys_and_unknown_job_is_conditional() {
    let cases = [
        (CancelDisposition::None, "migration.cancel.none", "none"),
        (CancelDisposition::NotStarted, "migration.cancel.notStarted", "notStarted"),
        (CancelDisposition::RequestedInFlight, "migration.cancel.requestedInFlight", "requestedInFlight"),
        (CancelDisposition::SettledPartially, "migration.cancel.settledPartially", "settledPartially"),
        (CancelDisposition::SettledRolledBack, "migration.cancel.settledRolledBack", "settledRolledBack"),
        (CancelDisposition::SettledCompleted, "migration.cancel.settledCompleted", "settledCompleted"),
        (CancelDisposition::SettledUnknown, "migration.cancel.settledUnknown", "settledUnknown"),
    ];
    for (disposition, key, name) in cases {
        let mut p = Probe::default();
        p.vm.verdict.cancel_disposition = disposition;
        let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
        rt.frame(0.0);

        let line = element(&rt, &format!("{PREFIX}-cancel"));
        assert_eq!(line.text.as_deref(), Some(t(key)), "{name}: cancel key text");
        assert_ne!(line.text.as_deref(), Some(key), "{name}: no key echo");
        assert!(!has_key(&rt, &format!("{PREFIX}-cancel-unknown-job")), "{name}: no unknown-job line by default");
        assert_eq!(disposition.as_str(), name, "{name}: as_str mirror");
        assert_eq!(disposition.key(), key, "{name}: key mirror");
    }

    // The unknown-job sentence is a separate *intent* sentence, shown only for
    // `cancelRequested && cancelAcknowledged === false` (React's exact gate).
    let mut p = Probe::default();
    p.vm.cancel_requested = true;
    p.vm.cancel_acknowledged = false;
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert_eq!(
        element(&rt, &format!("{PREFIX}-cancel-unknown-job")).text.as_deref(),
        Some(t("migration.cancel.unknownJob")),
        "unknown-job line uses its i18n key"
    );
    let attrs = element(&rt, &format!("{PREFIX}-verdict-attrs")).label.unwrap_or_default();
    assert!(attrs.contains("cancel_requested=true") && attrs.contains("cancel_acknowledged=false"), "cancel flags mirrored: {attrs}");

    let mut p = Probe::default();
    p.vm.cancel_requested = true;
    p.vm.cancel_acknowledged = true;
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(!has_key(&rt, &format!("{PREFIX}-cancel-unknown-job")), "acknowledged cancel has no unknown-job line");
}

#[test]
fn counter_reader_and_row_counts_obey_the_react_wire_rules() {
    // readJobCounter: decimal string only, safe integer, non-negative.
    assert_eq!(read_job_counter(&Some("42".into())), Some(42));
    assert_eq!(read_job_counter(&Some(" 7 ".into())), Some(7));
    assert_eq!(read_job_counter(&None), None);
    assert_eq!(read_job_counter(&Some("abc".into())), None);
    assert_eq!(read_job_counter(&Some("-1".into())), None);
    assert_eq!(read_job_counter(&Some("9007199254740992".into())), None, "above Number.MAX_SAFE_INTEGER");
    assert_eq!(sum_job_counters(&[Some("6".into()), Some("7".into())]), Some(13));
    assert_eq!(sum_job_counters(&[Some("bad".into()), Some("7".into())]), None, "one unreadable bucket fails the sum");

    let mut p = Probe::default();
    p.vm.progress = Some(JobProgress {
        read: Some("3".into()),
        converted: Some("4".into()),
        attempted: Some("5".into()),
        committed: Some("6".into()),
        unknown: Some("7".into()),
    });
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert_eq!(
        element(&rt, &format!("{PREFIX}-progress")).text.as_deref(),
        Some(format!("{}: 3 / 4 / 5 / 6 / 7", t("migration.progress.counts")).as_str()),
        "five buckets in React order with decimal-string values"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-rows")).text.as_deref(),
        Some(format!("{}: 13", t("migration.verdict.committedRows")).as_str()),
        "committed + unknown summed, not folded"
    );

    // Unreadable counters fall back to 0 on the progress line; an unreadable
    // committed/unknown bucket leaves the rows count blank (React renders null).
    let mut p = Probe::default();
    p.vm.progress = Some(JobProgress {
        read: Some("oops".into()),
        converted: Some("-2".into()),
        attempted: None,
        committed: Some("bad".into()),
        unknown: Some("7".into()),
    });
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert_eq!(
        element(&rt, &format!("{PREFIX}-progress")).text.as_deref(),
        Some(format!("{}: 0 / 0 / 0 / 0 / 7", t("migration.progress.counts")).as_str()),
        "invalid/absent buckets read as zero"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-rows")).text.as_deref(),
        Some(format!("{}: ", t("migration.verdict.committedRows")).as_str()),
        "unreadable committed bucket leaves the count blank"
    );

    // Without a progress prop there is no progress line; React's
    // sumJobCounters(undefined, undefined) is null, so the count is blank.
    let mut rt = Runtime::new(Probe::default(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(!has_key(&rt, &format!("{PREFIX}-progress")), "no progress prop, no line");
    assert_eq!(
        element(&rt, &format!("{PREFIX}-rows")).text.as_deref(),
        Some(format!("{}: ", t("migration.verdict.committedRows")).as_str()),
        "null sum renders a blank count, matching React"
    );
}

#[test]
fn boundaries_artifacts_recovery_uncertainty_and_reconcile_render_from_the_vm() {
    let mut p = Probe::default();
    p.vm = loaded_vm();
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);

    // Three per-boundary rows with the exact React testid suffix.
    assert!(has_key(&rt, &format!("{PREFIX}-boundaries")), "boundaries list");
    for (stage, verified, key) in [
        ("write-a", true, "migration.boundary.verified"),
        ("write-b", true, "migration.boundary.verified"),
        ("write-c", false, "migration.boundary.unverified"),
    ] {
        assert!(
            has_key(&rt, &format!("{PREFIX}-boundary-{stage}")),
            "{stage}: per-stage testid key mounted"
        );
        assert!(texts(&rt).iter().any(|x| x.as_str() == t(key)), "{stage}: verdict key rendered as locale text");
        assert!(
            has_key(&rt, &format!("{PREFIX}-boundary-{stage}-verified-{verified}")),
            "{stage}: data-boundary-verified mirror"
        );
    }

    // Artifacts: React puts the same testid on every row; count by prefix,
    // excluding the `-artifacts` list container.
    let artifact_prefix = format!("{PREFIX}-artifact");
    let artifacts_key = format!("{PREFIX}-artifacts");
    let artifact_rows: Vec<_> = keys_with_prefix(&rt, &artifact_prefix)
        .into_iter()
        .filter(|k| k.as_str() != artifacts_key.as_str())
        .collect();
    assert_eq!(artifact_rows.len(), 2, "one row per artifact id");
    assert!(texts(&rt).iter().any(|x| x == "transfer-sql-aaaa"));
    assert!(texts(&rt).iter().any(|x| x == "transfer-sql-bbbb"));

    // Recovery verdict/reason/resume lines map backend codes to i18n text.
    let verdict = element(&rt, &format!("{PREFIX}-recovery-verdict"));
    let verdict_text = verdict.text.clone().unwrap_or_default();
    assert!(verdict_text.contains(t("migration.verdict.recoveryLabel")), "recovery label");
    assert!(verdict_text.contains(t("migration.verdict.notExecuted")), "notExecuted mapped to i18n, not the raw code");
    assert_eq!(
        element(&rt, &format!("{PREFIX}-recovery-reason")).text.as_deref(),
        Some(t("migration.verdict.notDispatchedAfterRestart")),
        "restart-before-dispatch reason mapped to i18n"
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-recovery-resume-through")).text.as_deref(),
        Some(format!("{}: 4", t("migration.verdict.resumeThrough")).as_str())
    );

    // Uncertainty badge + reconcile + replayed are all mounted.
    let badge = element(&rt, &format!("{PREFIX}-uncertainty"));
    assert_eq!(badge.label.as_deref(), Some(t("migration.uncertainty.missingEvidence")), "warning badge text");
    assert_eq!(
        element(&rt, &format!("{PREFIX}-reconcile")).text.as_deref(),
        Some(t("migration.verdict.requiresReconcile"))
    );
    assert_eq!(
        element(&rt, &format!("{PREFIX}-replayed")).text.as_deref(),
        Some(t("migration.verdict.replayedReceipt"))
    );
    assert_eq!(UncertaintyReason::MissingEvidence.key(), Some("migration.uncertainty.missingEvidence"));
    assert_eq!(UncertaintyReason::None.key(), None, "no badge for no uncertainty");
}

#[test]
fn raw_recovery_strings_and_the_uncertainty_none_path_do_not_fabricate_copy() {
    let mut p = Probe::default();
    p.vm.recovery_verdict = Some("customVerdict".into());
    p.vm.recovery_reason = Some("customReason".into());
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    let verdict_text = element(&rt, &format!("{PREFIX}-recovery-verdict")).text.unwrap_or_default();
    assert!(verdict_text.contains("customVerdict"), "unrecognised verdict rendered verbatim");
    assert_eq!(
        element(&rt, &format!("{PREFIX}-recovery-reason")).text.as_deref(),
        Some("customReason"),
        "unrecognised reason rendered verbatim"
    );
    assert!(!has_key(&rt, &format!("{PREFIX}-uncertainty")), "no uncertainty badge");
    assert!(!has_key(&rt, &format!("{PREFIX}-recovery-resume-through")), "no resume line without a through value");

    // Empty boundary list + no uncertainty: the no-boundaries sentence exists,
    // but no `data-evidence-gap` marker.
    assert_eq!(
        element(&rt, &format!("{PREFIX}-boundaries-empty")).text.as_deref(),
        Some(t("migration.verdict.noBoundaries"))
    );
    assert!(!has_key(&rt, &format!("{PREFIX}-boundaries-empty-attrs-true")), "no evidence gap");

    // Missing-evidence with an empty boundary list DOES mark the gap.
    let mut p = Probe::default();
    p.vm.verdict.uncertainty = UncertaintyReason::MissingEvidence;
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(has_key(&rt, &format!("{PREFIX}-boundaries-empty-attrs-true")), "evidence gap marked");
    let empty = element(&rt, &format!("{PREFIX}-boundaries-empty"));
    assert_eq!(empty.text_color, Some(kit(true).c.warning.fg), "gap uses warning tone");
}

#[test]
fn custom_prefix_and_plan_data_attributes_are_observable() {
    let mut p = Probe::default();
    p.vm = loaded_vm();
    p.vm.test_id_prefix = "sync-diff".into();
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.frame(0.0);

    for key in [
        "sync-diff-verdict",
        "sync-diff-verdict-status",
        "sync-diff-cancel",
        "sync-diff-cancel-unknown-job",
        "sync-diff-progress",
        "sync-diff-rows",
        "sync-diff-replayed",
        "sync-diff-boundaries",
        "sync-diff-artifacts",
        "sync-diff-recovery-verdict",
        "sync-diff-recovery-reason",
        "sync-diff-recovery-resume-through",
        "sync-diff-uncertainty",
        "sync-diff-reconcile",
        "sync-diff-error",
        "sync-diff-verdict-attrs",
    ] {
        assert!(has_key(&rt, key), "custom-prefix key: {key}");
    }
    assert!(!has_key(&rt, &format!("{PREFIX}-verdict")), "default prefix not mounted alongside custom");

    let attrs = element(&rt, "sync-diff-verdict-attrs").label.unwrap_or_default();
    for expected in [
        "severity=uncertain",
        "cancel=settledUnknown",
        "uncertainty=missingEvidence",
        "verified=2",
        "unverified=1",
        "plan_id=plan-42",
        "plan_digest=sha-abc",
        "selection_revision=7",
    ] {
        assert!(attrs.contains(expected), "data-* mirror missing {expected}: {attrs}");
    }
    assert_eq!(VerdictSeverity::Uncertain.as_str(), "uncertain");
    assert_eq!(CancelDisposition::SettledUnknown.as_str(), "settledUnknown");
    assert_eq!(UncertaintyReason::MissingEvidence.as_str(), "missingEvidence");
}

#[test]
fn error_block_copies_the_exact_message_and_schedules_the_1500ms_feedback_wake() {
    let clipboard = clipboard::memory();
    let mut p = Probe::default();
    p.vm.error = Some("boom: exact backend detail".into());
    let mut rt = Runtime::new(p, view, (960.0, 640.0), env());
    rt.use_clipboard(&clipboard);
    rt.frame(0.0);

    assert_eq!(
        element(&rt, &format!("{PREFIX}-error-text")).text.as_deref(),
        Some("boom: exact backend detail"),
        "error message is the exact backend string"
    );
    let copy = element(&rt, &format!("{PREFIX}-error-copy"));
    assert!(copy.focusable, "copy control takes focus");
    assert!(
        copy.label.as_deref().is_some_and(|l| l.contains("copy") || l.contains("复制")),
        "copy control starts on common.copy: {:?}",
        copy.label
    );

    click(&mut rt, &format!("{PREFIX}-error-copy"));
    assert_eq!(
        clipboard.text().as_deref().map(String::as_str),
        Some("boom: exact backend detail"),
        "exact message copied to runtime clipboard"
    );
    assert_eq!(rt.state.copy_calls, 1, "copy callback reached host exactly once");
    let press_clock = rt.state.press_clock.expect("callback observed tx.clock()");

    // Pin the real timed wake: the runtime must produce a frame ~1500ms out.
    let deadline = rt.wake_deadline().expect("copy press schedules a feedback wake");
    assert!(
        (deadline - press_clock - 1.5).abs() <= 0.01,
        "wake deadline must be 1500ms after the press: press={press_clock} deadline={deadline}"
    );
    let until = rt.state.copied_until.expect("host recorded copied_until");
    assert!(
        (until - press_clock - 1.5).abs() <= 1e-9,
        "copied_until must be press_clock + 1.5s: press={press_clock} until={until}"
    );

    // Feedback is visible immediately, holds just before expiry, reverts at it.
    let label_of = |rt: &Runtime<Probe>| element(rt, &format!("{PREFIX}-error-copy")).label.unwrap_or_default();
    let shown = label_of(&rt);
    assert!(shown.contains("copied") || shown.contains("已复制"), "immediately copied: {shown}");
    rt.state.now = until - 0.001;
    sync(&mut rt);
    let shown = label_of(&rt);
    assert!(shown.contains("copied") || shown.contains("已复制"), "just before expiry still copied: {shown}");
    rt.state.now = until;
    sync(&mut rt);
    let shown = label_of(&rt);
    assert!(shown.contains("copy") || shown.contains("复制"), "at expiry reverted to copy: {shown}");
    assert!(!(shown.contains("copied") || shown.contains("已复制")), "no copied label at expiry: {shown}");

    // No error prop => no error block at all (no silent empty alert).
    let mut rt = Runtime::new(Probe::default(), view, (960.0, 640.0), env());
    rt.frame(0.0);
    assert!(!has_key(&rt, &format!("{PREFIX}-error")), "no error block without an error prop");
}

#[test]
fn geometry_holds_at_both_sizes_in_both_themes_with_every_block_mounted() {
    let owned = [
        format!("{PREFIX}-verdict"),
        format!("{PREFIX}-verdict-status"),
        format!("{PREFIX}-cancel"),
        format!("{PREFIX}-cancel-unknown-job"),
        format!("{PREFIX}-progress"),
        format!("{PREFIX}-rows"),
        format!("{PREFIX}-replayed"),
        format!("{PREFIX}-boundaries"),
        format!("{PREFIX}-artifacts"),
        format!("{PREFIX}-recovery-verdict"),
        format!("{PREFIX}-recovery-reason"),
        format!("{PREFIX}-recovery-resume-through"),
        format!("{PREFIX}-uncertainty"),
        format!("{PREFIX}-reconcile"),
        format!("{PREFIX}-error"),
    ];

    for (w, h) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut p = Probe::default();
            p.vm = loaded_vm();
            p.dark = dark;
            let mut rt = Runtime::new(p, view, (w, h), env());
            rt.frame(0.0);

            assert_in_window(&rt, w, h, &format!("verdict {w}x{h} dark={dark}"));
            for key in &owned {
                assert_eq!(
                    key_count(&rt, key),
                    1,
                    "{w}x{h} dark={dark}: owned key appears once: {key}"
                );
            }
            // The only legitimate duplicate keys are the repeated artifact
            // testid rows (React mounts the same testid on each artifact).
            for entry in rt.elements() {
                if let Some(key) = entry.key.as_deref() {
                    if key.contains("#dup") {
                        assert!(
                            key.starts_with(&format!("{PREFIX}-artifact")),
                            "unexpected duplicate key: {key}"
                        );
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Reverse verification (mutation -> observed failure text -> restored):
//
// 1. Changing `VerdictSeverity::Ok` to return `"migration.verdict.failed"`
//    from `key()` made `all_four_severities_render_the_exact_status_text_and_tone`
//    fail with: `assertion `left == right` failed: ok: exact status key text
//    left: Some("未应用") right: Some("已完成")`. Restored the key mapping.
// 2. Changing `CopyableError::copied(copied)` to `copied(false)` in the panel's
//    error block made `error_block_copies_the_exact_message_and_schedules_the_1500ms_feedback_wake`
//    fail with: `immediately copied: 复制`. Restored the `copied` wiring.
//
// Both assertions observe the rendered tree, so neither mutation could pass.
