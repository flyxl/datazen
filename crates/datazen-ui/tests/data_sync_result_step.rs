//! T247: the Data Sync result step — the outcome derivation chain, the
//! verdict text/color branches, the skipped aside, the re-compare button,
//! and the embedded compare summary.

use rune::{col, Cx, El, Env, Runtime};

use datazen_ui::views::data_sync::data_sync_result_step::{
    DataSyncExecutionOutcome, DataSyncExecutionResult, DataSyncResultStep,
};
use datazen_ui::views::data_sync::mapping_view::CompareSummaryStats;

fn env(dark: bool) -> Env {
    Env {
        system_dark: dark,
        scale: 1.0,
    }
}

fn stats() -> CompareSummaryStats {
    CompareSummaryStats {
        inserts: 1,
        updates: 2,
        deletes: 3,
        unchanged_tables: 4,
        incompatible: 0,
    }
}

fn execution(
    outcome: Option<DataSyncExecutionOutcome>,
    rolled_back: bool,
    reason: Option<&str>,
    skipped: Option<usize>,
) -> DataSyncExecutionResult {
    DataSyncExecutionResult {
        applied: 9,
        rolled_back: rolled_back,
        rollback_reason: reason.map(str::to_string),
        affected_rows: Some(9),
        skipped,
        outcome,
    }
}

#[derive(Default)]
struct Host {
    execution: Option<DataSyncExecutionResult>,
    re_compares: usize,
    copies: usize,
    explains: usize,
}

fn view(host: &Host, _cx: &mut Cx<Host>) -> El {
    // The host's own execution result rides beside the counters.
    col().w_full().h_full().child(
        DataSyncResultStep::<Host>::new(
            host.execution.clone(),
            stats(),
        )
        .explain_loading(false)
        .on_re_compare(|s: &mut Host| s.re_compares += 1)
        .on_copy_report(|s: &mut Host| s.copies += 1)
        .on_explain_diff(|s: &mut Host| s.explains += 1)
        .build(true),
    )
}

fn mount(host: Host) -> Runtime<Host> {
    let mut rt = Runtime::new(host, view, (960.0, 640.0), env(true));
    rt.frame(0.0);
    rt
}

fn text_of(rt: &Runtime<Host>, key: &str) -> String {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text)
        .unwrap_or_default()
}

fn has_key(rt: &Runtime<Host>, key: &str) -> bool {
    rt.elements()
        .into_iter()
        .any(|e| e.key.as_deref() == Some(key) && !e.exiting)
}

fn color_of(rt: &Runtime<Host>, key: &str) -> Option<rune::Color> {
    rt.elements()
        .into_iter()
        .find(|e| e.key.as_deref() == Some(key) && !e.exiting)
        .and_then(|e| e.text_color)
}

fn click(rt: &mut Runtime<Host>, key: &str) {
    let (x, y) = rt.find(key).unwrap_or_else(|| panic!("{key} is laid out"));
    rt.pointer_down(x, y);
    rt.pointer_up(x, y);
    rt.frame(1.0 / 60.0);
}

#[test]
fn the_outcome_derivation_prefers_the_wire_field() {
    // Present outcome wins over the rollback flag.
    let result = execution(Some(DataSyncExecutionOutcome::Committed), true, None, None);
    assert_eq!(result.outcome_or(), DataSyncExecutionOutcome::Committed);
    // Absent outcome reads the rollback flag.
    let result = execution(None, true, None, None);
    assert_eq!(result.outcome_or(), DataSyncExecutionOutcome::RolledBack);
    let result = execution(None, false, None, None);
    assert_eq!(result.outcome_or(), DataSyncExecutionOutcome::Committed);
}

#[test]
fn no_result_renders_the_committed_face() {
    // `lastExecutionResult === null` → `outcome ?? ...` on null-null → the
    // `?.` chain yields undefined → the ?? branch runs with rolledBack
    // undefined → 'committed'.
    let rt = mount(Host::default());
    assert_eq!(
        text_of(&rt, "data-sync-execute-done-verdict"),
        datazen_ui::i18n::t("sync.executeDone").to_owned()
    );
}

#[test]
fn the_verdict_branches_read_their_pack_words() {
    for (outcome, word_key) in [
        (DataSyncExecutionOutcome::Unknown, "sync.executionUnknown"),
        (DataSyncExecutionOutcome::PartiallyApplied, "migration.verdict.partial"),
        (DataSyncExecutionOutcome::Committed, "sync.executeDone"),
    ] {
        let host = Host {
            execution: Some(execution(Some(outcome), false, None, None)),
            ..Host::default()
        };
        let rt = mount(host);
        assert_eq!(
            text_of(&rt, "data-sync-execute-done-verdict"),
            datazen_ui::i18n::t(word_key).to_owned(),
            "{outcome:?}"
        );
    }
}

#[test]
fn a_rolled_back_execution_prefers_its_reason() {
    let host = Host {
        execution: Some(execution(
            Some(DataSyncExecutionOutcome::RolledBack),
            true,
            Some("死锁，事务已回滚"),
            None,
        )),
        ..Host::default()
    };
    let rt = mount(host);
    assert_eq!(text_of(&rt, "data-sync-execute-done-verdict"), "死锁，事务已回滚");
}

#[test]
fn a_rolled_back_execution_without_a_reason_reads_the_pack_word() {
    // And an empty reason string reads as absent.
    for reason in [None, Some("")] {
        let host = Host {
            execution: Some(execution(
                Some(DataSyncExecutionOutcome::RolledBack),
                true,
                reason,
                None,
            )),
            ..Host::default()
        };
        let rt = mount(host);
        assert_eq!(
            text_of(&rt, "data-sync-execute-done-verdict"),
            datazen_ui::i18n::t("sync.rolledBack").to_owned()
        );
    }
}

#[test]
fn the_verdict_color_splits_amber_from_green() {
    let amber = mount(Host {
        execution: Some(execution(Some(DataSyncExecutionOutcome::Unknown), false, None, None)),
        ..Host::default()
    });
    let green = mount(Host {
        execution: Some(execution(Some(DataSyncExecutionOutcome::Committed), false, None, None)),
        ..Host::default()
    });
    assert_eq!(
        color_of(&amber, "data-sync-execute-done-verdict"),
        color_of(&amber, "data-sync-execute-done-verdict")
    );
    assert_ne!(
        color_of(&amber, "data-sync-execute-done-verdict"),
        color_of(&green, "data-sync-execute-done-verdict"),
        "unknown reads amber, committed reads green"
    );
}

#[test]
fn the_skipped_aside_mounts_with_its_count() {
    let host = Host {
        execution: Some(execution(
            Some(DataSyncExecutionOutcome::Committed),
            false,
            None,
            Some(3),
        )),
        ..Host::default()
    };
    let rt = mount(host);
    assert_eq!(text_of(&rt, "data-sync-execute-done-skipped"), "已跳过 3 个并发冲突");
}

#[test]
fn zero_skipped_mounts_no_aside() {
    let host = Host {
        execution: Some(execution(
            Some(DataSyncExecutionOutcome::Committed),
            false,
            None,
            Some(0),
        )),
        ..Host::default()
    };
    let rt = mount(host);
    assert!(!has_key(&rt, "data-sync-execute-done-skipped"));
}

#[test]
fn the_re_compare_button_fires_and_the_summary_is_embedded() {
    let mut rt = mount(Host {
        execution: Some(execution(Some(DataSyncExecutionOutcome::Committed), false, None, None)),
        ..Host::default()
    });
    // The compare summary strip is embedded with its own counters.
    assert!(has_key(&rt, "data-sync-summary"));
    assert_eq!(text_of(&rt, "data-sync-summary-inserts"), "+1 插入");
    click(&mut rt, "data-sync-re-compare");
    assert_eq!(rt.state.re_compares, 1);
    // The summary's own buttons work through the step's wiring.
    click(&mut rt, "data-sync-copy-report");
    assert_eq!(rt.state.copies, 1);
    click(&mut rt, "data-sync-explain-diff");
    assert_eq!(rt.state.explains, 1);
}
