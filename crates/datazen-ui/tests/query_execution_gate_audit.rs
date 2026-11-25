//! T518 independent audit (tester, not coder).
//!
//! Green probes of gate decisions the shipped 14 tests do not pin, checked
//! against the React source/test: case-insensitive write classification, the
//! high-risk confirm copy when Safe Mode is off, declined confirms, AskCancel,
//! staleness per field, and empty-error post-execution.
//!
//! Three further probes found real divergences from React (findingUnknown on a
//! finding-less mutation, Ask→entire using Selection mode, and the Postgres
//! quote guard over-blocking a non-target statement); they are reported as
//! defects with their exact RED failure text and are intentionally not kept as
//! committed red tests.

use datazen_ui::views::query::query_execution_gate::{
    decide, is_stale, post_execution, reduce, GateAction, GateEvent, GateInput,
    LiveContext, QueryExecutionGateState,
};
use datazen_ui::views::query::query_editor_section::ExecutionStrategy;

fn first(actions: &[GateAction]) -> &GateAction {
    actions.first().unwrap_or_else(|| panic!("at least one gate action"))
}

fn live(value: &datazen_ui::views::query::query_execution_gate::ExecutionSnapshot) -> LiveContext {
    LiveContext {
        sql: value.sql.clone(),
        read_only: value.read_only,
        safe_mode: value.safe_mode,
        confirm_dangerous: value.confirm_dangerous,
    }
}

#[test]
fn lower_case_write_verbs_are_still_blocked_on_read_only() {
    let mut value = GateInput::full("delete from t where id = 1");
    value.snapshot.read_only = true;
    let (actions, _) = decide(&value, 0.0);
    assert_eq!(
        first(&actions),
        &GateAction::Message { key: "query.editor.executionConfirm.blockedReadOnly", token: None },
        "classification is case-insensitive like the React scanner"
    );
}

#[test]
fn high_risk_confirm_when_safe_mode_is_off_carries_drop_finding() {
    let mut value = GateInput::full("DROP TABLE t");
    value.snapshot.safe_mode = false;
    let (actions, _) = decide(&value, 0.0);
    let spec = match first(&actions) {
        GateAction::ConfirmDangerous { spec } => spec,
        other => panic!("expected ConfirmDangerous, got {other:?}"),
    };
    assert_eq!(spec.title_key, "query.editor.executionConfirm.highRiskTitle");
    assert_eq!(spec.badge_key, Some("query.editor.executionConfirm.badgeHighRisk"));
    assert!(
        spec.finding_keys.contains(&"query.editor.executionConfirm.findingDrop"),
        "DROP carries the drop finding: {:?}", spec.finding_keys
    );
}

#[test]
fn a_declined_confirm_executes_nothing() {
    let mut value = GateInput::full("DELETE FROM t");
    value.snapshot.is_production = true;
    let mut state = QueryExecutionGateState::default();
    reduce(&mut state, GateEvent::RequestExecute { input: value, now: 0.0 });
    let actions = reduce(
        &mut state,
        GateEvent::ConfirmResult { confirmed: false, live: LiveContext { sql: String::new(), read_only: false, safe_mode: false, confirm_dangerous: false } },
    );
    assert!(actions.is_empty(), "a declined confirmation submits nothing");
    assert!(state.confirm_pending.is_none(), "the pending confirm is consumed");
}

#[test]
fn ask_cancel_clears_the_modal_and_context() {
    let doc = "SELECT 1;\nSELECT 2;";
    let mut value = GateInput::full(doc);
    value.cursor_offset = Some(0);
    value.strategy = ExecutionStrategy::Ask;
    let mut state = QueryExecutionGateState::default();
    reduce(&mut state, GateEvent::RequestExecute { input: value, now: 0.0 });
    assert!(state.ask_modal.is_some());
    let actions = reduce(&mut state, GateEvent::AskCancel);
    assert!(actions.is_empty());
    assert!(state.ask_modal.is_none() && state.ask_context.is_none());
}

#[test]
fn staleness_checks_each_frozen_field() {
    let snapshot = GateInput::full("SELECT 1").snapshot;
    let base = live(&snapshot);
    assert!(!is_stale(&snapshot, &base));
    let mut changed = base.clone();
    changed.read_only = true;
    assert!(is_stale(&snapshot, &changed), "readOnly change is stale");
    let mut changed = base.clone();
    changed.safe_mode = false;
    assert!(is_stale(&snapshot, &changed), "safeMode change is stale");
    let mut changed = base.clone();
    changed.confirm_dangerous = false;
    assert!(is_stale(&snapshot, &changed), "confirmDangerous change is stale");
}

#[test]
fn post_execution_treats_an_empty_error_as_success() {
    assert_eq!(
        post_execution(Some("")),
        GateAction::PostExecution { had_error: false },
        "React's `error ?? null` empty string means no error"
    );
    assert_eq!(
        post_execution(Some("25P02")),
        GateAction::PostExecution { had_error: true }
    );
}
