//! Independent audit for T140: `MigrationRunHistoryDialog` port.
//!
//! The coder's suite covers the headline paths. These tests fill three real
//! gaps: the two known Transfer rollback outcomes it never exercised
//! (`notStarted` / `rolledBack`), the `on_reconcile.is_some()` false branch,
//! and the absent optional profile/error lines in the detail card. A final
//! geometry probe reads laid-out rects (not the component's own report) in
//! both window sizes and palettes.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::components::migration::migration_run_history_dialog::{
    rollback_display, MigrationOperation, MigrationRunHistoryDialog, MigrationRunRecord,
    RollbackDisplay,
};
use datazen_ui::i18n::t;

const RUN_ID: &str = "audit-run";

#[derive(Clone)]
struct AuditProbe {
    dark: bool,
    operation: MigrationOperation,
    open: bool,
    items: Vec<MigrationRunRecord>,
    selected: Option<String>,
    with_reconcile: bool,
    reconcile_calls: usize,
}

impl Default for AuditProbe {
    fn default() -> Self {
        Self {
            dark: true,
            operation: MigrationOperation::DataSync,
            open: false,
            items: Vec::new(),
            selected: None,
            with_reconcile: true,
            reconcile_calls: 0,
        }
    }
}

fn audit_view(s: &AuditProbe, _cx: &mut Cx<AuditProbe>) -> El {
    let mut dialog = MigrationRunHistoryDialog::<AuditProbe>::new(s.operation)
        .open(s.open)
        .open_epoch(1)
        .items(s.items.clone())
        .selected(s.selected.clone())
        .on_open(|st: &mut AuditProbe| st.open = true);
    if s.with_reconcile {
        dialog = dialog.on_reconcile(|st: &mut AuditProbe, _id| {
            st.reconcile_calls += 1;
        });
    }
    col().w_full().h_full().children(dialog.build(s.dark))
}

fn new_rt(state: AuditProbe, size: (f32, f32)) -> Runtime<AuditProbe> {
    let mut rt = Runtime::new(
        state,
        audit_view,
        size,
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.invalidate();
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
    rt
}

fn keys(rt: &Runtime<AuditProbe>) -> std::collections::HashSet<String> {
    rt.elements()
        .into_iter()
        .filter(|e| !e.exiting)
        .filter_map(|e| e.key)
        .collect()
}

fn element(rt: &Runtime<AuditProbe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn run(operation: MigrationOperation, outcome: &str, rollback: &str) -> MigrationRunRecord {
    MigrationRunRecord {
        id: RUN_ID.to_owned(),
        operation,
        status: "failed".into(),
        outcome: outcome.into(),
        phase: "finished".into(),
        profile_id: None,
        profile_revision: None,
        started_at: "2026-09-23T12:00:00Z".into(),
        selected_count: 1,
        committed_count: 1,
        failed_count: 0,
        conflict_count: 0,
        rollback_outcome: rollback.into(),
        error_summary: None,
    }
}

/// The coder's rollback matrix tests `unknown`, `partiallyApplied`,
/// `notRequired`, a legacy value and non-Transfer cases — but not the two
/// other *known* Transfer outcomes. Both must be localized (no raw key on
/// screen) and must NOT carry the uncertain warning.
#[test]
fn not_started_and_rolled_back_are_localized_without_warning() {
    for (outcome, key) in [
        ("notStarted", "transfer.historyOutcome.notStarted"),
        ("rolledBack", "transfer.historyOutcome.rolledBack"),
    ] {
        let expected = t(key).to_owned();
        assert_ne!(expected, key, "{key} must exist in the translation table");
        assert_eq!(
            rollback_display(&run(MigrationOperation::DataTransfer, "failed", outcome)),
            RollbackDisplay {
                label: expected.clone(),
                uncertain: false,
                localized: true,
            },
            "{outcome}: known Transfer outcome is localized and not warned"
        );

        let rt = new_rt(
            AuditProbe {
                open: true,
                operation: MigrationOperation::DataTransfer,
                items: vec![run(MigrationOperation::DataTransfer, "failed", outcome)],
                selected: Some(RUN_ID.to_owned()),
                ..Default::default()
            },
            (1440.0, 816.0),
        );
        assert_eq!(
            element(&rt, "migration-history-rollback-label")
                .text
                .as_deref(),
            Some(expected.as_str()),
            "{outcome}: rendered label is localized"
        );
        assert!(
            keys(&rt).contains(&format!(
                "migration-history-rollback-attrs-{outcome}-false"
            )),
            "{outcome}: warning marker is false"
        );
    }
}

/// The `can_reconcile` predicate also requires `on_reconcile.is_some()`. The
/// coder's probe always wires the callback, so this guard was never exercised.
#[test]
fn reconcile_action_is_absent_without_a_callback() {
    let rt = new_rt(
        AuditProbe {
            open: true,
            operation: MigrationOperation::DataSync,
            items: vec![run(MigrationOperation::DataSync, "unknown", "unknown")],
            selected: Some(RUN_ID.to_owned()),
            with_reconcile: false,
            ..Default::default()
        },
        (1440.0, 816.0),
    );
    assert!(keys(&rt).contains("migration-run-detail"), "detail still renders");
    assert!(
        !keys(&rt).contains("migration-run-reconcile"),
        "no reconcile button without an on_reconcile callback"
    );
    assert_eq!(rt.state.reconcile_calls, 0);
}

/// `profile_id` and `error_summary` are optional in React. The coder's `run()`
/// helper always sets both, so the omission branches were untested.
#[test]
fn detail_omits_optional_profile_and_error_lines_when_absent() {
    let rt = new_rt(
        AuditProbe {
            open: true,
            items: vec![run(MigrationOperation::DataSync, "unknown", "unknown")],
            selected: Some(RUN_ID.to_owned()),
            ..Default::default()
        },
        (1440.0, 816.0),
    );
    let keys = keys(&rt);
    for key in [
        "migration-run-detail",
        "migration-run-detail-outcome",
        "migration-run-detail-counts",
        "migration-history-rollback-outcome",
    ] {
        assert!(keys.contains(key), "detail key {key} is present");
    }
    for key in ["migration-run-detail-profile", "migration-run-detail-error"] {
        assert!(!keys.contains(key), "optional key {key} is omitted when absent");
    }
}

/// Observation channel independent of any self-report: rects from the laid-out
/// tree. Beyond "nothing overflows", assert the dialog and its scroll surface
/// are actually on-screen and sized — a zero-height body would pass a pure
/// no-overflow check while drawing nothing.
#[test]
fn the_dialog_and_scroll_surface_are_sized_and_on_screen() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut rt = Runtime::new(
                AuditProbe {
                    dark,
                    open: true,
                    operation: MigrationOperation::DataSync,
                    items: vec![run(MigrationOperation::DataSync, "unknown", "unknown")],
                    selected: Some(RUN_ID.to_owned()),
                    ..Default::default()
                },
                audit_view,
                (width, height),
                Env {
                    system_dark: dark,
                    scale: 1.0,
                },
            );
            rt.invalidate();
            rt.frame(0.0);
            rt.settle_all();
            rt.frame(0.0);

            let dialog = element(&rt, "migration-run-history-dialog").rect;
            assert!(
                dialog[0] >= -0.5
                    && dialog[1] >= -0.5
                    && dialog[0] + dialog[2] <= width + 0.5
                    && dialog[1] + dialog[3] <= height + 0.5,
                "{width}x{height} dark={dark}: dialog {dialog:?} is off-screen"
            );
            assert!(
                dialog[2] > 100.0 && dialog[3] > 100.0,
                "{width}x{height} dark={dark}: dialog {dialog:?} is collapsed"
            );

            let surface = element(&rt, "migration-run-history").rect;
            assert!(
                surface[2] > 50.0 && surface[3] > 50.0,
                "{width}x{height} dark={dark}: scroll surface {surface:?} is collapsed"
            );

            for e in rt.elements().into_iter().filter(|e| !e.exiting) {
                let [x, _, w, _] = e.rect;
                assert!(
                    x >= -0.5 && x + w <= width + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} hangs past the window",
                    e.key,
                    e.rect
                );
            }
        }
    }
}
