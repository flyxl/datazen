//! T518: pure `useQueryExecutionGate` logic — every gate decision and state
//! transition ported from the 485-line React test, with no runtime view or IPC.

use crate::views::query::query_execution_gate::{
    decide, is_production_group, is_stale, post_execution, reduce, ExecutionMode, ExecutionSnapshot,
    GateAction, GateEvent, GateInput, LiveContext, QueryExecutionGateState,
};
use crate::views::query::query_editor_section::ExecutionStrategy;

fn input(sql: &str) -> GateInput {
    GateInput::full(sql)
}

fn production(mut value: GateInput) -> GateInput {
    value.snapshot.is_production = true;
    value
}

fn read_only(mut value: GateInput) -> GateInput {
    value.snapshot.read_only = true;
    value
}

fn live(value: &ExecutionSnapshot) -> LiveContext {
    LiveContext {
        sql: value.sql.clone(),
        read_only: value.read_only,
        safe_mode: value.safe_mode,
        confirm_dangerous: value.confirm_dangerous,
    }
}

fn first(actions: &[GateAction]) -> &GateAction {
    actions.first().unwrap_or_else(|| panic!("at least one gate action"))
}

#[test]
fn executes_a_full_query_with_the_frozen_snapshot() {
    let value = input("SELECT 1");
    let (actions, pending) = decide(&value, 0.0);
    assert!(pending.is_none(), "a read query needs no confirmation");
    match first(&actions) {
        GateAction::Execute { mode, sql, snapshot } => {
            assert_eq!(*mode, ExecutionMode::Query);
            assert_eq!(sql, "SELECT 1");
            assert_eq!(snapshot, &value.snapshot, "the snapshot is frozen before execution");
        }
        other => panic!("expected Execute, got {other:?}"),
    }
}

#[test]
fn unclosed_begin_opens_the_dialog_and_cancel_clears_it() {
    let mut state = QueryExecutionGateState::default();
    let actions = reduce(
        &mut state,
        GateEvent::RequestExecute {
            input: input("BEGIN;"),
            now: 0.0,
        },
    );
    assert!(matches!(first(&actions), GateAction::OpenUnclosedTransaction { .. }), "BEGIN; triggers the dialog");
    assert!(state.tx_unclosed_open);
    assert!(!actions.iter().any(|a| matches!(a, GateAction::Execute { .. })), "nothing executes yet");

    let cleared = reduce(&mut state, GateEvent::CancelUnclosed);
    assert!(cleared.is_empty());
    assert!(!state.tx_unclosed_open, "cancel closes without executing");
}

#[test]
fn suspicious_postgres_double_quote_is_blocked_before_anything_else() {
    let mut value = input("SELECT * FROM t WHERE name = \"John\"");
    value.database_type = "postgresql".to_owned();
    let (actions, _) = decide(&value, 0.0);
    assert_eq!(
        first(&actions),
        &GateAction::Message { key: "query.postgresDoubleQuoteHint", token: None }
    );
}

#[test]
fn quoted_identifiers_are_not_suspicious_literals() {
    let mut value = input("SELECT t.\"name\" FROM t WHERE t.\"id\" = 1");
    value.database_type = "postgresql".to_owned();
    let (actions, _) = decide(&value, 0.0);
    assert!(matches!(first(&actions), GateAction::Execute { .. }), "identifier quoting is fine");
}

#[test]
fn read_only_blocks_writes_but_allows_selects() {
    let (blocked, _) = decide(&read_only(input("DELETE FROM t WHERE id = 1")), 0.0);
    assert_eq!(
        first(&blocked),
        &GateAction::Message { key: "query.editor.executionConfirm.blockedReadOnly", token: None }
    );
    let (allowed, _) = decide(&read_only(input("SELECT 1")), 0.0);
    assert!(matches!(first(&allowed), GateAction::Execute { .. }));
}

#[test]
fn safe_mode_blocks_drop_and_truncate() {
    for sql in ["DROP TABLE t", "TRUNCATE t"] {
        let (actions, _) = decide(&input(sql), 0.0);
        assert_eq!(
            first(&actions),
            &GateAction::Message { key: "query.editor.executionConfirm.blockedSafeMode", token: None },
            "safe mode blocks {sql}"
        );
    }
}

#[test]
fn production_write_confirms_then_executes_and_a_stale_snapshot_refuses() {
    let mut state = QueryExecutionGateState::default();
    let value = production(input("DELETE FROM t WHERE id = 1"));
    let actions = reduce(&mut state, GateEvent::RequestExecute { input: value.clone(), now: 0.0 });
    let spec = match first(&actions) {
        GateAction::ConfirmDangerous { spec } => spec,
        other => panic!("expected ConfirmDangerous, got {other:?}"),
    };
    assert_eq!(spec.title_key, "query.editor.executionConfirm.productionTitle");
    assert_eq!(spec.message_key, "query.editor.executionConfirm.productionMessage");
    assert_eq!(spec.badge_key, Some("query.editor.executionConfirm.badgeProduction"));

    let mut changed = live(&value.snapshot);
    changed.sql = "SELECT 1".to_owned();
    let stale = reduce(
        &mut state,
        GateEvent::ConfirmResult { confirmed: true, live: changed },
    );
    assert_eq!(
        first(&stale),
        &GateAction::Message { key: "query.editor.executionConfirm.staleMessage", token: None }
    );

    let mut state = QueryExecutionGateState::default();
    reduce(&mut state, GateEvent::RequestExecute { input: value.clone(), now: 0.0 });
    let confirmed = reduce(
        &mut state,
        GateEvent::ConfirmResult { confirmed: true, live: live(&value.snapshot) },
    );
    assert!(matches!(first(&confirmed), GateAction::Execute { .. }), "fresh confirm executes");
}

#[test]
fn production_select_never_confirms() {
    let (actions, _) = decide(&production(input("SELECT 1")), 0.0);
    assert!(matches!(first(&actions), GateAction::Execute { .. }), "production read executes directly");
}

#[test]
fn missing_bind_parameter_blocks_and_records_the_focus_deadline() {
    let mut state = QueryExecutionGateState::default();
    let value = input("SELECT * FROM users WHERE id = :id");
    let actions = reduce(&mut state, GateEvent::RequestExecute { input: value, now: 10.0 });
    assert_eq!(
        first(&actions),
        &GateAction::Message { key: "query.editor.param.missingValue", token: Some(":id".to_owned()) }
    );
    assert_eq!(state.param_focus_deadline, Some(10.05), "the 50ms focus deadline is recorded, not fired here");
}

#[test]
fn provided_bind_parameter_substitutes_and_uses_execute_selection() {
    let mut value = input("SELECT * FROM users WHERE id = :id");
    value.values.insert("named:id".to_owned(), "123".to_owned());
    let (actions, _) = decide(&value, 0.0);
    match first(&actions) {
        GateAction::Execute { mode, sql, .. } => {
            assert_eq!(*mode, ExecutionMode::Selection, "substituted SQL goes through executeSelection");
            assert_eq!(sql, "SELECT * FROM users WHERE id = 123");
        }
        other => panic!("expected Execute, got {other:?}"),
    }
}

#[test]
fn current_statement_strategy_targets_the_statement_at_the_cursor() {
    let doc = "SELECT 1;\nSELECT 2;\nSELECT 3;";
    let mut value = input(doc);
    value.cursor_offset = Some(doc.find("SELECT 2;").map_or(0, |at| at));
    let (actions, _) = decide(&value, 0.0);
    match first(&actions) {
        GateAction::Execute { mode, sql, .. } => {
            assert_eq!(*mode, ExecutionMode::Selection);
            assert_eq!(sql, "SELECT 2;");
        }
        other => panic!("expected targeted statement, got {other:?}"),
    }
}

#[test]
fn entire_script_strategy_executes_the_full_text() {
    let doc = "SELECT 1;\nSELECT 2;\nSELECT 3;";
    let mut value = input(doc);
    value.cursor_offset = Some(0);
    value.strategy = ExecutionStrategy::EntireScript;
    let (actions, _) = decide(&value, 0.0);
    match first(&actions) {
        GateAction::Execute { mode, sql, .. } => {
            assert_eq!(*mode, ExecutionMode::Query);
            assert_eq!(sql, doc);
        }
        other => panic!("expected full script, got {other:?}"),
    }
}

#[test]
fn ask_strategy_opens_the_modal_and_both_choices_reenter_the_gate() {
    let doc = "SELECT 1;\nSELECT 2;\nSELECT 3;";
    let mut value = input(doc);
    value.cursor_offset = Some(0);
    value.strategy = ExecutionStrategy::Ask;
    let mut state = QueryExecutionGateState::default();
    let actions = reduce(&mut state, GateEvent::RequestExecute { input: value, now: 0.0 });
    match first(&actions) {
        GateAction::OpenStrategyAsk { target } => {
            assert_eq!(target.statement_count, 3);
            assert_eq!(target.current_statement.as_deref(), Some("SELECT 1;"));
            assert_eq!(target.entire_script, doc);
        }
        other => panic!("expected ask modal, got {other:?}"),
    }
    assert!(state.ask_modal.is_some());

    let current = reduce(&mut state, GateEvent::AskExecuteCurrent);
    match first(&current) {
        GateAction::Execute { sql, .. } => assert_eq!(sql, "SELECT 1;"),
        other => panic!("current choice executes: {other:?}"),
    }
    let mut state = QueryExecutionGateState::default();
    reduce(&mut state, GateEvent::RequestExecute {
        input: {
            let mut value = input(doc);
            value.cursor_offset = Some(0);
            value.strategy = ExecutionStrategy::Ask;
            value
        },
        now: 0.0,
    });
    let entire = reduce(&mut state, GateEvent::AskExecuteEntire);
    assert!(
        matches!(first(&entire), GateAction::Execute { mode: ExecutionMode::Query, .. }),
        "entire choice executes via executeQuery: got {:?}",
        first(&entire)
    );
    assert!(state.ask_modal.is_none());
}

#[test]
fn snapshot_staleness_and_post_execution_are_explicit() {
    let snapshot = ExecutionSnapshot {
        sql: "SELECT 1".to_owned(),
        ..input("SELECT 1").snapshot
    };
    assert!(!is_stale(&snapshot, &live(&snapshot)));
    assert!(is_production_group(Some("preset:production")));
    assert!(!is_production_group(Some("team-a")));
    assert_eq!(post_execution(None), GateAction::PostExecution { had_error: false });
    assert_eq!(post_execution(Some("25P02")), GateAction::PostExecution { had_error: true });
}

// --- regression pins for the confirmed React-parity fixes --------------------

#[test]
fn classified_mutation_without_findings_has_no_unknown_finding_key() {
    // `DELETE ... WHERE` is a classified mutation with no high-risk finding;
    // React pushes findingUnknown only for classification === 'unknown'.
    let mut state = QueryExecutionGateState::default();
    let value = production(input("DELETE FROM t WHERE id = 1"));
    let actions = reduce(&mut state, GateEvent::RequestExecute { input: value, now: 0.0 });
    match first(&actions) {
        GateAction::ConfirmDangerous { spec } => {
            assert!(
                !spec.finding_keys.contains(&"query.editor.executionConfirm.findingUnknown"),
                "no findingUnknown for a classified mutation: got {:?}",
                spec.finding_keys
            );
            assert!(spec.finding_keys.is_empty(), "no findings at all: {:?}", spec.finding_keys);
        }
        other => panic!("expected ConfirmDangerous, got {other:?}"),
    }
}

#[test]
fn ask_execute_entire_runs_in_query_mode_on_the_entire_script() {
    let doc = "SELECT 1;\nSELECT 2;\nSELECT 3;";
    let mut value = input(doc);
    value.cursor_offset = Some(0);
    value.strategy = ExecutionStrategy::Ask;
    let mut state = QueryExecutionGateState::default();
    reduce(&mut state, GateEvent::RequestExecute { input: value, now: 0.0 });
    let actions = reduce(&mut state, GateEvent::AskExecuteEntire);
    match first(&actions) {
        GateAction::Execute { mode, sql, .. } => {
            assert_eq!(*mode, ExecutionMode::Query, "entire script uses executeQuery, got {mode:?}");
            assert_eq!(sql, doc, "entire script text is executed");
        }
        other => panic!("expected Execute, got {other:?}"),
    }
}

#[test]
fn pg_double_quote_guard_checks_only_the_targeted_statement() {
    let doc = "SELECT * FROM t WHERE name = \"John\";\nSELECT 2;";
    let mut value = input(doc);
    value.database_type = "postgresql".to_owned();
    let second = doc.find("SELECT 2;").map_or(0, |at| at);
    value.cursor_offset = Some(second);

    let (targeted, _) = decide(&value, 0.0);
    assert!(
        !matches!(first(&targeted), GateAction::Message { key: "query.postgresDoubleQuoteHint", .. }),
        "a quoted literal in a non-target statement must not block the target: got {:?}",
        first(&targeted)
    );

    value.cursor_offset = Some(0);
    let (in_target, _) = decide(&value, 0.0);
    assert_eq!(
        first(&in_target),
        &GateAction::Message { key: "query.postgresDoubleQuoteHint", token: None },
        "a quoted literal inside the target still blocks"
    );
}

// Reverse verification (mutation -> observed failure text -> restored):
// 1. Removing the readOnly hard block made `read_only_blocks_writes_but_allows_selects`
//    fail because the decision became Execute instead of the blocked message.
// 2. Treating the wrong statement as current made `current_statement_strategy_targets_the_statement_at_the_cursor`
//    fail on `SELECT 1;` vs `SELECT 2;`.
