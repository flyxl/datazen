//! T140: MigrationRunHistoryDialog — real, branch-complete tests.
//!
//! Observations come from the laid-out element tree (keys, labels, text and
//! runtime rects), never from the component's own report. The component is a
//! pure builder, so a small probe state supplies the host-owned fetch/loading
//! inputs and records callback calls.

use rune::{col, Cx, El, Env, Runtime};

use crate::components::migration::migration_run_history_dialog::{
    rollback_display, MigrationOperation, MigrationRunHistoryDialog, MigrationRunRecord,
};
use crate::i18n::t;

const RUN_ID: &str = "sync-unknown-run";

#[derive(Clone)]
struct Probe {
    dark: bool,
    operation: MigrationOperation,
    open: bool,
    loading: bool,
    error: Option<String>,
    items: Vec<MigrationRunRecord>,
    selected: Option<String>,
    open_calls: usize,
    close_calls: usize,
    selected_id: Option<String>,
    reconciled_id: Option<String>,
}

impl Default for Probe {
    fn default() -> Self {
        Self {
            dark: true,
            operation: MigrationOperation::DataSync,
            open: false,
            loading: false,
            error: None,
            items: Vec::new(),
            selected: None,
            open_calls: 0,
            close_calls: 0,
            selected_id: None,
            reconciled_id: None,
        }
    }
}

fn view(s: &Probe, _cx: &mut Cx<Probe>) -> El {
    let dialog = MigrationRunHistoryDialog::<Probe>::new(s.operation)
        .open(s.open)
        .open_epoch(1)
        .loading(s.loading)
        .error(s.error.clone())
        .items(s.items.clone())
        .selected(s.selected.clone())
        .on_open(|st: &mut Probe| {
            st.open_calls += 1;
            st.open = true;
        })
        .on_close(|st: &mut Probe| {
            st.close_calls += 1;
            st.open = false;
        })
        .on_select(|st: &mut Probe, id| {
            st.selected = Some(id.clone());
            st.selected_id = Some(id);
        })
        .on_reconcile(|st: &mut Probe, id| st.reconciled_id = Some(id));
    col().w_full().h_full().children(dialog.build(s.dark))
}

fn runtime() -> Runtime<Probe> {
    let mut rt = Runtime::new(
        Probe::default(),
        view,
        (1440.0, 816.0),
        Env {
            system_dark: true,
            scale: 1.0,
        },
    );
    rt.frame(0.0);
    rt
}

fn settle(rt: &mut Runtime<Probe>) {
    rt.invalidate();
    rt.frame(0.0);
    rt.settle_all();
    rt.frame(0.0);
}

fn has_key(rt: &Runtime<Probe>, key: &str) -> bool {
    rt.elements().iter().any(|e| e.key.as_deref() == Some(key))
}

fn element(rt: &Runtime<Probe>, key: &str) -> rune::ElementInfo {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key))
        .unwrap_or_else(|| panic!("{key} is laid out"))
}

fn texts(rt: &Runtime<Probe>) -> Vec<String> {
    rt.elements()
        .iter()
        .filter_map(|e| e.text.clone())
        .collect()
}

fn has_text(rt: &Runtime<Probe>, needle: &str) -> bool {
    texts(rt).iter().any(|text| text.contains(needle))
}

fn click(rt: &mut Runtime<Probe>, key: &str) {
    settle(rt);
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    settle(rt);
}

fn run(operation: MigrationOperation, outcome: &str, rollback: &str) -> MigrationRunRecord {
    MigrationRunRecord {
        id: RUN_ID.to_owned(),
        operation,
        status: "failed".to_owned(),
        outcome: outcome.to_owned(),
        phase: "finished".to_owned(),
        profile_id: Some("profile-1".to_owned()),
        profile_revision: Some("2026-09-23T00:00:00Z".to_owned()),
        started_at: "2026-09-23T12:00:00Z".to_owned(),
        selected_count: 1,
        committed_count: 0,
        failed_count: 2,
        conflict_count: 3,
        rollback_outcome: rollback.to_owned(),
        error_summary: Some("Commit or rollback could not be confirmed.".to_owned()),
    }
}

#[test]
fn the_trigger_opens_the_dialog_with_contract_keys() {
    let mut rt = runtime();
    assert!(has_key(&rt, "migration-history-open"), "trigger rendered");
    assert!(
        !has_key(&rt, "migration-run-history-dialog"),
        "dialog is closed initially"
    );
    click(&mut rt, "migration-history-open");
    assert_eq!(rt.state.open_calls, 1, "trigger callback fired once");
    for key in [
        "migration-run-history-dialog",
        "migration-run-history-dialog-title",
        "migration-run-history-dialog-close",
        "migration-run-history",
    ] {
        assert!(has_key(&rt, key), "dialog key {key} is composed");
    }
    assert_eq!(
        element(&rt, "migration-run-history-dialog-title").text.as_deref(),
        Some(t("migrationHistory.title")),
        "dialog title uses the exact i18n key"
    );
    click(&mut rt, "migration-run-history-dialog-close");
    assert_eq!(rt.state.close_calls, 1, "close callback fired once");
    assert!(!has_key(&rt, "migration-run-history"), "dialog dismissed");
}

#[test]
fn loading_error_and_empty_states_are_mutually_exclusive_as_in_react() {
    let mut rt = runtime();
    rt.state.open = true;
    rt.state.loading = true;
    settle(&mut rt);
    assert!(
        has_key(&rt, "migration-run-loading-spinner"),
        "loading mounts the spinner"
    );
    assert!(has_text(&rt, t("common.loading")), "loading label rendered");
    assert!(!has_key(&rt, "migration-run-empty"), "no empty state while loading");

    rt.state.loading = false;
    rt.state.error = Some("history fetch failed".to_owned());
    settle(&mut rt);
    let banner = element(&rt, "migration-run-error");
    assert_eq!(banner.role, "alert", "error uses an announced banner");
    assert!(has_text(&rt, "history fetch failed"), "error text rendered");
    assert!(
        !has_key(&rt, "migration-run-empty"),
        "no empty state while errored"
    );

    rt.state.error = None;
    settle(&mut rt);
    assert_eq!(
        element(&rt, "migration-run-empty").text.as_deref(),
        Some(t("migrationHistory.empty")),
        "empty state uses the exact i18n key"
    );
    assert!(!has_key(&rt, "migration-run-loading-spinner"), "spinner gone");
}

#[test]
fn rows_show_status_phase_timestamp_and_counts_and_select_a_run() {
    let mut rt = runtime();
    rt.state.open = true;
    rt.state.items = vec![run(MigrationOperation::DataSync, "unknown", "unknown")];
    settle(&mut rt);

    assert!(has_key(&rt, "migration-run-sync-unknown-run"), "row key rendered");
    assert!(has_text(&rt, "failed · finished"), "status and phase rendered");
    assert_eq!(
        element(&rt, "migration-run-count-sync-unknown-run").text.as_deref(),
        Some("0/1"),
        "committed/selected counts rendered"
    );
    let time = element(&rt, "migration-run-time-sync-unknown-run")
        .text
        .unwrap_or_default();
    assert!(
        time.contains("2026-09-23"),
        "startedAt is formatted for display: {time}"
    );
    assert!(!has_key(&rt, "migration-run-detail"), "no detail before selection");

    click(&mut rt, "migration-run-sync-unknown-run");
    assert_eq!(
        rt.state.selected_id.as_deref(),
        Some(RUN_ID),
        "row press passes the run id"
    );
    assert!(has_key(&rt, "migration-run-detail"), "selection opens the detail");
}

#[test]
fn detail_shows_outcome_counts_profile_and_error_summary() {
    let mut rt = runtime();
    rt.state.open = true;
    rt.state.items = vec![run(MigrationOperation::DataSync, "unknown", "unknown")];
    rt.state.selected = Some(RUN_ID.to_owned());
    settle(&mut rt);

    let outcome = element(&rt, "migration-run-detail-outcome")
        .text
        .unwrap_or_default();
    assert_eq!(
        outcome,
        format!("{}: unknown", t("migrationHistory.outcome")),
        "outcome line uses the exact i18n key"
    );
    let counts = element(&rt, "migration-run-detail-counts")
        .text
        .unwrap_or_default();
    assert_eq!(
        counts,
        format!("{}: 0 / 2 / 3", t("migrationHistory.counts")),
        "committed / failed / conflict counts in order"
    );
    let profile = element(&rt, "migration-run-detail-profile")
        .text
        .unwrap_or_default();
    assert_eq!(
        profile,
        format!("{}: profile-1 @ 2026-09-23T00:00:00Z", t("migrationHistory.profile")),
        "profile id and revision rendered when present"
    );
    assert_eq!(
        element(&rt, "migration-run-detail-error").role,
        "alert",
        "error summary uses a banner"
    );
}

#[test]
fn rollback_display_branches_exactly_like_react() {
    // (operation, rollback outcome, expected label, uncertain warning, localized)
    let cases = [
        (
            MigrationOperation::DataTransfer,
            "unknown",
            t("transfer.historyOutcome.unknown").to_owned(),
            true,
            true,
        ),
        (
            MigrationOperation::DataTransfer,
            "partiallyApplied",
            t("transfer.historyOutcome.partiallyApplied").to_owned(),
            true,
            true,
        ),
        (
            MigrationOperation::DataTransfer,
            "notRequired",
            t("transfer.historyOutcome.notRequired").to_owned(),
            false,
            true,
        ),
        (MigrationOperation::DataTransfer, "legacyOutcome", "legacyOutcome".to_owned(), false, false),
        (MigrationOperation::DataSync, "unknown", "unknown".to_owned(), false, false),
        (MigrationOperation::SchemaDiff, "partiallyApplied", "partiallyApplied".to_owned(), false, false),
    ];
    for (operation, rollback, expected, uncertain, localized) in cases {
        assert_eq!(
            rollback_display(&run(operation, "failed", rollback)),
            crate::components::migration::migration_run_history_dialog::RollbackDisplay {
                label: expected.clone(),
                uncertain,
                localized,
            },
            "{operation:?}/{rollback}: pure display classification"
        );

        let mut rt = runtime();
        rt.state.open = true;
        rt.state.items = vec![run(operation, "failed", rollback)];
        rt.state.selected = Some(RUN_ID.to_owned());
        settle(&mut rt);
        assert_eq!(
            element(&rt, "migration-history-rollback-label").text.as_deref(),
            Some(expected.as_str()),
            "{operation:?}/{rollback}: rendered rollback label"
        );
        assert_eq!(
            element(&rt, "migration-history-rollback-outcome").label.as_deref(),
            Some(rollback),
            "data-outcome mirror"
        );
        assert!(
            has_key(
                &rt,
                &format!("migration-history-rollback-attrs-{rollback}-{uncertain}")
            ),
            "{operation:?}/{rollback}: warning-class marker"
        );
    }
}

#[test]
fn reconcile_is_only_offered_for_unknown_data_sync_runs() {
    let mut rt = runtime();
    rt.state.open = true;
    rt.state.items = vec![run(MigrationOperation::DataSync, "unknown", "unknown")];
    rt.state.selected = Some(RUN_ID.to_owned());
    settle(&mut rt);
    assert!(has_key(&rt, "migration-run-reconcile"), "reconcile offered");
    assert!(
        has_text(&rt, t("migrationHistory.syncUnknownHint")),
        "hint uses the exact i18n key"
    );
    click(&mut rt, "migration-run-reconcile");
    assert_eq!(
        rt.state.reconciled_id.as_deref(),
        Some(RUN_ID),
        "reconcile callback receives the selected run id"
    );

    // Another operation or outcome never exposes the action.
    for (operation, outcome) in [
        (MigrationOperation::DataTransfer, "unknown"),
        (MigrationOperation::SchemaDiff, "unknown"),
        (MigrationOperation::DataSync, "partiallyApplied"),
    ] {
        let mut rt = runtime();
        rt.state.operation = operation;
        rt.state.open = true;
        rt.state.items = vec![run(operation, outcome, "unknown")];
        rt.state.selected = Some(RUN_ID.to_owned());
        settle(&mut rt);
        assert!(
            !has_key(&rt, "migration-run-reconcile"),
            "{operation:?}/{outcome} must not offer reconciliation"
        );
    }
}

#[test]
fn the_dialog_fits_both_windows_in_both_themes() {
    for (width, height) in [(960.0, 640.0), (1440.0, 816.0)] {
        for dark in [true, false] {
            let mut rt = Runtime::new(
                Probe {
                    dark,
                    open: true,
                    items: vec![run(MigrationOperation::DataSync, "unknown", "unknown")],
                    selected: Some(RUN_ID.to_owned()),
                    ..Default::default()
                },
                view,
                (width, height),
                Env {
                    system_dark: dark,
                    scale: 1.0,
                },
            );
            settle(&mut rt);
            for key in ["migration-run-history-dialog", "migration-run-history"] {
                assert!(
                    has_key(&rt, key),
                    "{width}x{height} dark={dark}: dialog key {key} missing"
                );
            }
            for element in rt.elements() {
                let [x, _, w, _] = element.rect;
                assert!(
                    x >= -0.5 && x + w <= width + 0.5,
                    "{width}x{height} dark={dark}: {:?} {:?} hangs past the window",
                    element.key,
                    element.rect
                );
            }
        }
    }
}
