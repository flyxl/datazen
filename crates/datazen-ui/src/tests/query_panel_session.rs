//! T183: `lib/session/QueryPanelSession.ts` port — real branch tests.
//!
//! Drives the pure registry and switch machines with the pinned values from
//! `queryPanelSession.test.ts` ('grant-1'-style ids here are the session/rev
//! strings 's', 'epoch', '1'/'2', owners 'a'/'b', rollback execution 'e1').
//! The registry scenarios use the real T083 controller effects; the switch
//! scenarios feed host answers directly, with no async and no fake timers.

use crate::session::editor_session_controller::{
    ApiErrorCode, AttachmentState, ContextConfidence, EditorSessionController, ExecutionReceipt,
    ExecutionState, ExecutionTarget, NamespaceTarget, OwnerRef, SessionContext, SessionEffect,
    SessionError, SessionState, SessionView, SubmissionToken, TransactionState,
};
use crate::session::query_panel_session::{
    is_pane_key_of_panel, panel_id_of_pane_key, DatabaseSwitch, QueryPanelSessionRegistry,
    SwitchEffect, SwitchExecutionView, SwitchInput, SwitchResult,
};

fn owner(id: &str) -> OwnerRef {
    OwnerRef::Editor {
        client_instance_id: "client".to_owned(),
        editor_session_id: id.to_owned(),
    }
}

fn target() -> ExecutionTarget {
    ExecutionTarget {
        connection_id: "c".to_owned(),
        namespace: NamespaceTarget {
            database: Some("A".to_owned()),
            ..Default::default()
        },
        object: None,
    }
}

fn desired() -> NamespaceTarget {
    NamespaceTarget {
        database: Some("B".to_owned()),
        ..Default::default()
    }
}

fn session(revision: &str, tx: TransactionState) -> SessionView {
    SessionView {
        handle: crate::session::editor_session_controller::SessionHandle {
            db_session_id: "s".to_owned(),
            runtime_epoch: "epoch".to_owned(),
        },
        connection_id: "c".to_owned(),
        config_revision: "1".to_owned(),
        owner: owner("a"),
        initial_target: target(),
        observed_context: SessionContext {
            namespace: target().namespace,
            search_path: None,
            effective_identity: None,
            transaction_state: tx,
            autocommit: Some(true),
            confidence: ContextConfidence::Confirmed,
        },
        context_revision: revision.to_owned(),
        state: SessionState::Ready,
        attachment_state: AttachmentState::Attached,
        active_execution_id: None,
        expires_at: None,
    }
}

fn receipt(id: &str) -> ExecutionReceipt {
    ExecutionReceipt {
        execution_id: id.to_owned(),
        stream_id: format!("stream-{id}"),
        state: ExecutionState::Running,
    }
}

fn issue_token(effects: &[SessionEffect]) -> u64 {
    match effects.first() {
        Some(SessionEffect::IssueToken { call_id, .. }) => *call_id,
        other => panic!("expected IssueToken, got {other:?}"),
    }
}

/// Drive a fresh controller to `Open` and return the emitted request's owner.
fn opened_owner(controller: &mut EditorSessionController) -> OwnerRef {
    let effects = controller.ensure_session().expect("no session starts an open");
    let effects = controller.on_token(
        issue_token(&effects),
        Ok(SubmissionToken {
            idempotency_key: "token".to_owned(),
            expires_at: 999,
        }),
    );
    match effects.first() {
        Some(SessionEffect::Open { request, .. }) => request.owner.clone(),
        other => panic!("expected Open, got {other:?}"),
    }
}

#[test]
fn pane_keys_split_on_the_double_colon_separator() {
    assert_eq!(panel_id_of_pane_key("panel-query-1"), "panel-query-1");
    assert_eq!(
        panel_id_of_pane_key("panel-query-1::pane-b"),
        "panel-query-1"
    );
    assert!(is_pane_key_of_panel("panel-query-1", "panel-query-1"));
    assert!(is_pane_key_of_panel(
        "panel-query-1::pane-b",
        "panel-query-1"
    ));
    assert!(!is_pane_key_of_panel(
        "panel-query-2::pane-b",
        "panel-query-1"
    ));
}

#[test]
fn registry_lazily_creates_one_controller_per_key_and_duplicates_with_a_fresh_owner() {
    let mut registry = QueryPanelSessionRegistry::new();
    let first_ptr = registry.get("panel-1", &target(), &owner("a")) as *const EditorSessionController;
    let first_id = registry
        .begin_remove("panel-1")
        .expect("source controller")
        .controller_id;
    let first_again_ptr =
        registry.get("panel-1", &target(), &owner("a")) as *const EditorSessionController;
    assert_eq!(
        first_ptr, first_again_ptr,
        "get returns the same controller for the same key"
    );

    let duplicate_ptr = registry
        .duplicate("panel-1", "panel-1-copy", &target(), &owner("b"))
        .expect("a fresh destination key is accepted") as *const EditorSessionController;
    assert_ne!(
        first_ptr, duplicate_ptr,
        "duplicate must never reuse the source controller"
    );
    let duplicate_id = registry
        .begin_remove("panel-1-copy")
        .expect("duplicate controller")
        .controller_id;
    assert!(
        duplicate_id > first_id,
        "the duplicate gets its own id (and therefore its own session)"
    );

    // One open per controller, with the two distinct owners in insertion order.
    let owner_a = opened_owner(registry.peek_mut("panel-1").expect("source controller"));
    let owner_b = opened_owner(
        registry
            .peek_mut("panel-1-copy")
            .expect("duplicate controller"),
    );
    assert_eq!(owner_a, owner("a"));
    assert_eq!(owner_b, owner("b"));
}

#[test]
fn duplicate_into_an_existing_key_is_a_context_conflict() {
    let mut registry = QueryPanelSessionRegistry::new();
    let _ = registry.get("panel-1", &target(), &owner("a"));
    let error = registry
        .duplicate("panel-1", "panel-1", &target(), &owner("b"))
        .err()
        .expect("the destination already owns a session");
    assert_eq!(
        error,
        ApiErrorCode::Other("ContextConflict".to_owned()),
        "exact React ApiError code"
    );
    assert_eq!(registry.keys_of_panel("panel-1"), ["panel-1"]);
}

#[test]
fn keys_of_panel_includes_panes_that_never_executed() {
    let mut registry = QueryPanelSessionRegistry::new();
    let _ = registry.get("panel-1", &target(), &owner("a"));
    let _ = registry.get("panel-1::pane-b", &target(), &owner("a"));
    let _ = registry.get("panel-2", &target(), &owner("a"));
    assert_eq!(
        registry.keys_of_panel("panel-1"),
        ["panel-1", "panel-1::pane-b"],
        "the registry, not the exec table, is the source of truth"
    );
    assert_eq!(registry.keys_of_panel("panel-2"), ["panel-2"]);
}

#[test]
fn remove_keeps_ownership_until_close_and_survives_a_replacement() {
    let mut registry = QueryPanelSessionRegistry::new();
    let _ = registry.get("panel-1", &target(), &owner("a"));
    let step = registry.begin_remove("panel-1").expect("controller exists");
    assert!(step.controller_id > 0, "the close targets a real controller");
    assert!(
        registry.peek("panel-1").is_some(),
        "ownership stays reachable until the close completes"
    );
    assert!(
        !registry.finish_remove("panel-1", step.controller_id + 99),
        "a stale id cannot remove the controller"
    );
    assert!(registry.peek("panel-1").is_some());

    // The original close completes, then a later `get` binds a fresh controller
    // (with its own owner/id) to the same pane key.
    assert!(registry.finish_remove("panel-1", step.controller_id));
    let _ = registry.get("panel-1", &target(), &owner("b"));
    let replacement = registry.begin_remove("panel-1").expect("replacement");
    assert_ne!(replacement.controller_id, step.controller_id);

    // A stale completion from the first controller cannot evict the replacement.
    assert!(
        !registry.finish_remove("panel-1", step.controller_id),
        "the replacement owns the key now"
    );
    assert!(registry.peek("panel-1").is_some(), "it was not removed");
    assert!(
        registry.finish_remove("panel-1", replacement.controller_id),
        "the replacement can be removed by its own id"
    );
    assert!(registry.peek("panel-1").is_none());
    assert!(registry.begin_remove("panel-1").is_none(), "nothing left to remove");
}

#[test]
fn switch_blocks_on_active_transaction_until_confirmed_and_keeps_handle_on_conflict() {
    // Declined confirmation: cancelled with the original handle untouched.
    let (mut declined, step) = DatabaseSwitch::new(desired(), Some(session("1", TransactionState::Active)));
    assert_eq!(step.effects, [SwitchEffect::Refresh]);
    let step = declined.step(SwitchInput::Refreshed(Some(session("1", TransactionState::Active))));
    assert_eq!(
        step.effects,
        [SwitchEffect::ConfirmTransaction {
            session: session("1", TransactionState::Active)
        }],
        "an active transaction asks for confirmation"
    );
    let step = declined.step(SwitchInput::Confirm(false));
    assert_eq!(step.result, Some(SwitchResult::Cancelled));
    assert_eq!(declined.phase(), "done");

    // Confirmed: rollback, poll to terminal, refresh, setContext -> conflict.
    let (mut switch, _) = DatabaseSwitch::new(desired(), Some(session("1", TransactionState::Active)));
    let _ = switch.step(SwitchInput::Refreshed(Some(session("1", TransactionState::Active))));
    let step = switch.step(SwitchInput::Confirm(true));
    assert_eq!(step.effects, [SwitchEffect::Rollback]);
    let step = switch.step(SwitchInput::Rollback(Ok(receipt("e1"))));
    assert_eq!(step.effects, [SwitchEffect::PollExecution {
        execution_id: "e1".to_owned()
    }]);
    let step = switch.step(SwitchInput::Execution(SwitchExecutionView {
        state: ExecutionState::Succeeded,
        error_code: None,
    }));
    assert_eq!(step.effects, [SwitchEffect::Refresh]);
    // The refresh after the rollback skips the transaction gate even when the
    // backend still reports one — confirmation already produced the rollback.
    let step = switch.step(SwitchInput::Refreshed(Some(session("1", TransactionState::Active))));
    assert_eq!(step.effects, [SwitchEffect::SetContext]);
    let step = switch.step(SwitchInput::Context(Err(SessionError {
        code: ApiErrorCode::Other("ContextConflict".to_owned()),
        message: "revision mismatch".to_owned(),
    })));
    assert_eq!(step.effects, [SwitchEffect::Refresh]);
    let step = switch.step(SwitchInput::ConflictRefresh(Some(session("1", TransactionState::Active))));
    assert_eq!(step.result, Some(SwitchResult::Conflict {
        session: Some(session("1", TransactionState::Active))
    }));
    assert_eq!(step.effects, [SwitchEffect::RevisionConflict {
        session: Some(session("1", TransactionState::Active))
    }]);
    assert_eq!(
        step.result.as_ref().and_then(|result| match result {
            SwitchResult::Conflict { session } => session.as_ref().map(|s| s.handle.db_session_id.as_str()),
            _ => None,
        }),
        Some("s"),
        "the previous runtime handle survives the conflict"
    );
}

#[test]
fn switch_updates_the_session_on_success() {
    let (mut switch, _) = DatabaseSwitch::new(desired(), None);
    let step = switch.step(SwitchInput::Refreshed(Some(session("1", TransactionState::None))));
    assert_eq!(step.effects, [SwitchEffect::SetContext]);
    let step = switch.step(SwitchInput::Context(Ok(session("2", TransactionState::None))));
    assert_eq!(
        step.result,
        Some(SwitchResult::Switched {
            session: session("2", TransactionState::None)
        })
    );
    assert_eq!(switch.phase(), "done");
    assert_eq!(step.result.as_ref().map(SwitchResult::status), Some("switched"));
}

#[test]
fn switch_falls_back_to_snapshot_and_fails_on_terminal_rollback_outcome() {
    // refresh returns null -> the caller's snapshot is used.
    let (mut switch, _) = DatabaseSwitch::new(desired(), Some(session("9", TransactionState::None)));
    let step = switch.step(SwitchInput::Refreshed(None));
    assert_eq!(step.effects, [SwitchEffect::SetContext]);

    // A failed rollback receipt fails the switch immediately.
    let (mut failed, _) = DatabaseSwitch::new(desired(), Some(session("1", TransactionState::Active)));
    let _ = failed.step(SwitchInput::Refreshed(Some(session("1", TransactionState::Active))));
    let _ = failed.step(SwitchInput::Confirm(true));
    let step = failed.step(SwitchInput::Rollback(Err(SessionError {
        code: ApiErrorCode::Other("HostRejected".to_owned()),
        message: "host refused".to_owned(),
    })));
    assert_eq!(
        step.result,
        Some(SwitchResult::Failed {
            error: crate::session::query_panel_session::SwitchError {
                code: "HostRejected".to_owned(),
                message: "host refused".to_owned(),
            }
        })
    );

    // Polling re-emits while queued, then fails with the execution error code.
    let (mut polled, _) = DatabaseSwitch::new(desired(), Some(session("1", TransactionState::Active)));
    let _ = polled.step(SwitchInput::Refreshed(Some(session("1", TransactionState::Active))));
    let _ = polled.step(SwitchInput::Confirm(true));
    let _ = polled.step(SwitchInput::Rollback(Ok(receipt("e1"))));
    let step = polled.step(SwitchInput::Execution(SwitchExecutionView {
        state: ExecutionState::Running,
        error_code: None,
    }));
    assert_eq!(step.effects, [SwitchEffect::PollExecution {
        execution_id: "e1".to_owned()
    }]);
    assert_eq!(step.result, None, "a queued execution does not settle the switch");
    let step = polled.step(SwitchInput::Execution(SwitchExecutionView {
        state: ExecutionState::Failed,
        error_code: Some("PipelineAborted".to_owned()),
    }));
    assert_eq!(step.result, Some(SwitchResult::Failed {
        error: crate::session::query_panel_session::SwitchError {
            code: "PipelineAborted".to_owned(),
            message: "PipelineAborted".to_owned(),
        }
    }));

    // A terminal failure without an error code uses React's fallback sentence.
    let (mut fallback, _) = DatabaseSwitch::new(desired(), Some(session("1", TransactionState::Active)));
    let _ = fallback.step(SwitchInput::Refreshed(Some(session("1", TransactionState::Active))));
    let _ = fallback.step(SwitchInput::Confirm(true));
    let _ = fallback.step(SwitchInput::Rollback(Ok(receipt("e1"))));
    let step = fallback.step(SwitchInput::Execution(SwitchExecutionView {
        state: ExecutionState::Cancelled,
        error_code: None,
    }));
    assert_eq!(step.result, Some(SwitchResult::Failed {
        error: crate::session::query_panel_session::SwitchError {
            code: "Rollback outcome unavailable".to_owned(),
            message: "Rollback outcome unavailable".to_owned(),
        }
    }));
}

#[test]
fn stale_inputs_do_not_move_the_switch() {
    let (mut switch, _) = DatabaseSwitch::new(desired(), None);
    let step = switch.step(SwitchInput::Confirm(true));
    assert_eq!(step.effects, Vec::new());
    assert_eq!(step.result, None);
    assert_eq!(switch.phase(), "awaitRefresh", "out-of-phase input is ignored");
    let step = switch.step(SwitchInput::Refreshed(Some(session("1", TransactionState::None))));
    assert_eq!(step.effects, [SwitchEffect::SetContext]);
}
