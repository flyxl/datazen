//! Independent audit for T183: `QueryPanelSession.ts` port.
//!
//! The coder's suite covers the React test cases. These tests fill the branches
//! the React source pins but those cases never reach: `aborted` (as well as
//! `active`) transaction gating, the non-transaction states that must NOT
//! gate, a non-conflict `setContext` error falling through to `failed`, the
//! conflict-path refresh fallback, and the exact React status strings.

use crate::session::editor_session_controller::{
    ApiErrorCode, AttachmentState, ContextConfidence, ExecutionTarget, NamespaceTarget, OwnerRef,
    SessionContext, SessionError, SessionState, SessionView, TransactionState,
};
use crate::session::query_panel_session::{
    DatabaseSwitch, SwitchEffect, SwitchError, SwitchInput, SwitchResult,
};

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
        owner: OwnerRef::Editor {
            client_instance_id: "client".to_owned(),
            editor_session_id: "a".to_owned(),
        },
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

/// React: `txState === 'active' || txState === 'aborted'`. The coder only drove
/// `active`; `aborted` must gate too, while `none`/`unknown`/`unsupported`
/// must go straight to setContext.
#[test]
fn only_active_and_aborted_transactions_gate_the_switch() {
    for tx in [TransactionState::Active, TransactionState::Aborted] {
        let (mut switch, _) = DatabaseSwitch::new(desired(), Some(session("1", tx)));
        let step = switch.step(SwitchInput::Refreshed(Some(session("1", tx))));
        assert_eq!(
            step.effects,
            vec![SwitchEffect::ConfirmTransaction {
                session: session("1", tx)
            }],
            "{tx:?} must ask for confirmation"
        );
        assert_eq!(switch.phase(), "awaitConfirm", "{tx:?} phase");
    }
    for tx in [
        TransactionState::None,
        TransactionState::Unknown,
        TransactionState::Unsupported,
    ] {
        let (mut switch, _) = DatabaseSwitch::new(desired(), Some(session("1", tx)));
        let step = switch.step(SwitchInput::Refreshed(Some(session("1", tx))));
        assert_eq!(
            step.effects,
            vec![SwitchEffect::SetContext],
            "{tx:?} must not gate the switch"
        );
    }
}

/// React rethrows a non-ContextConflict setContext error to the outer catch,
/// yielding `{ status: 'failed', error }`. Also pin the exact code strings the
/// T083 error enum maps to.
#[test]
fn a_non_conflict_context_error_fails_with_the_exact_error_code() {
    let (mut switch, _) = DatabaseSwitch::new(desired(), None);
    let _ = switch.step(SwitchInput::Refreshed(Some(session("1", TransactionState::None))));
    let step = switch.step(SwitchInput::Context(Err(SessionError {
        code: ApiErrorCode::SessionLost,
        message: "lost".to_owned(),
    })));
    assert_eq!(
        step.result,
        Some(SwitchResult::Failed {
            error: SwitchError {
                code: "SessionLost".to_owned(),
                message: "lost".to_owned(),
            }
        })
    );
    assert_eq!(switch.phase(), "done");

    for (code, expected) in [
        (ApiErrorCode::SessionNotFound, "SessionNotFound"),
        (ApiErrorCode::RuntimeEpochMismatch, "RuntimeEpochMismatch"),
        (ApiErrorCode::Other("Custom".to_owned()), "Custom"),
    ] {
        let mapped = SwitchError::from_session(&SessionError {
            code,
            message: "m".to_owned(),
        });
        assert_eq!(mapped.code, expected, "exact React ApiError code");
        assert_eq!(mapped.message, "m");
    }
}

/// React's conflict path is `await controller.refresh().catch(() => controller.snapshot)`.
/// When the conflict refresh fails (None), the port must fall back to the
/// session it was switching from, still carrying the original runtime handle.
#[test]
fn a_failed_conflict_refresh_falls_back_to_the_current_session() {
    let current = session("rev-current", TransactionState::None);
    let (mut switch, _) = DatabaseSwitch::new(desired(), Some(current.clone()));
    let _ = switch.step(SwitchInput::Refreshed(Some(current.clone())));
    let _ = switch.step(SwitchInput::Context(Err(SessionError {
        code: ApiErrorCode::Other("ContextConflict".to_owned()),
        message: "revision mismatch".to_owned(),
    })));
    assert_eq!(switch.phase(), "awaitConflictRefresh");

    let step = switch.step(SwitchInput::ConflictRefresh(None));
    assert_eq!(
        step.result,
        Some(SwitchResult::Conflict {
            session: Some(current.clone())
        }),
        "the conflict result uses the fallback session"
    );
    assert_eq!(
        step.effects,
        vec![SwitchEffect::RevisionConflict {
            session: Some(current)
        }],
        "onRevisionConflict sees the same fallback session"
    );
}

/// `SwitchResult.status()` must mirror the four React `status` literals.
#[test]
fn switch_result_status_strings_match_react() {
    assert_eq!(
        SwitchResult::Switched {
            session: session("1", TransactionState::None)
        }
        .status(),
        "switched"
    );
    assert_eq!(SwitchResult::Cancelled.status(), "cancelled");
    assert_eq!(SwitchResult::Conflict { session: None }.status(), "conflict");
    assert_eq!(
        SwitchResult::Failed {
            error: SwitchError {
                code: "X".to_owned(),
                message: "x".to_owned()
            }
        }
        .status(),
        "failed"
    );
}
