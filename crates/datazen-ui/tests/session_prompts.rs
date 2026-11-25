//! T273: `lib/session/sessionPrompts.ts` port — real branch tests.
//!
//! Pinned values and cases come from `sessionPrompts.test.ts`. The helpers are
//! pure, so no runtime is involved; every assertion pins the exact React key.

use datazen_ui::session::editor_session_controller::{
    ApiErrorCode, AttachmentState, ContextConfidence, ExecutionTarget, NamespaceTarget, OwnerRef,
    SessionContext, SessionError, SessionHandle, SessionState, SessionView, TransactionState,
};
use datazen_ui::session::query_panel_session::{SwitchError, SwitchResult};
use datazen_ui::session::session_prompts::{
    needs_transaction_switch_confirm, projection_error_prompt_key, session_error_prompt_key,
    switch_result_prompt_key, PromptError, PROMPT_CANCELLED_RETRY, PROMPT_DISCONNECTED,
    PROMPT_EXECUTION_FAILED, PROMPT_REVISION_CONFLICT, PROMPT_SWITCH_FAILED,
};

fn view(state: SessionState) -> SessionView {
    SessionView {
        handle: SessionHandle {
            db_session_id: "s".to_owned(),
            runtime_epoch: "epoch".to_owned(),
        },
        connection_id: "c".to_owned(),
        config_revision: "1".to_owned(),
        owner: OwnerRef::Editor {
            client_instance_id: "client".to_owned(),
            editor_session_id: "pane".to_owned(),
        },
        initial_target: ExecutionTarget {
            connection_id: "c".to_owned(),
            namespace: NamespaceTarget::default(),
            object: None,
        },
        observed_context: SessionContext {
            namespace: NamespaceTarget::default(),
            search_path: None,
            effective_identity: None,
            transaction_state: TransactionState::None,
            autocommit: Some(true),
            confidence: ContextConfidence::Confirmed,
        },
        context_revision: "1".to_owned(),
        state,
        attachment_state: AttachmentState::Attached,
        active_execution_id: None,
        expires_at: None,
    }
}

fn failed(code: &str) -> SwitchResult {
    SwitchResult::Failed {
        error: SwitchError {
            code: code.to_owned(),
            message: "error".to_owned(),
        },
    }
}

#[test]
fn stays_silent_for_a_completed_or_user_cancelled_switch() {
    assert_eq!(
        switch_result_prompt_key(&SwitchResult::Switched {
            session: view(SessionState::Ready)
        }),
        None
    );
    assert_eq!(switch_result_prompt_key(&SwitchResult::Cancelled), None);
}

#[test]
fn maps_a_revision_conflict_and_a_lost_session_to_their_own_copy() {
    assert_eq!(
        switch_result_prompt_key(&SwitchResult::Conflict { session: None }),
        Some(PROMPT_REVISION_CONFLICT)
    );
    assert_eq!(
        switch_result_prompt_key(&failed("SessionLost")),
        Some(PROMPT_DISCONNECTED)
    );
    assert_eq!(
        switch_result_prompt_key(&failed("ResourceBusy")),
        Some(PROMPT_SWITCH_FAILED)
    );
    assert_eq!(
        switch_result_prompt_key(&failed("Other")),
        Some(PROMPT_SWITCH_FAILED)
    );
}

#[test]
fn maps_execution_and_projection_failures_to_the_right_copy() {
    assert_eq!(
        session_error_prompt_key(&PromptError::api("ResourceBusy", "busy")),
        PROMPT_CANCELLED_RETRY
    );
    assert_eq!(
        session_error_prompt_key(&PromptError::api("ContextConflict", "stale")),
        PROMPT_REVISION_CONFLICT
    );
    assert_eq!(
        session_error_prompt_key(&PromptError::api("SessionLost", "gone")),
        PROMPT_DISCONNECTED
    );
    assert_eq!(
        session_error_prompt_key(&PromptError::error("boom")),
        PROMPT_EXECUTION_FAILED
    );

    assert_eq!(
        projection_error_prompt_key(&PromptError::error("socket disconnected")),
        PROMPT_DISCONNECTED
    );
    assert_eq!(
        projection_error_prompt_key(&PromptError::error("network timeout")),
        PROMPT_DISCONNECTED
    );
    assert_eq!(
        projection_error_prompt_key(&PromptError::api("SessionLost", "gone")),
        PROMPT_DISCONNECTED
    );
    assert_eq!(
        projection_error_prompt_key(&PromptError::error("syntax error")),
        PROMPT_EXECUTION_FAILED
    );
}

#[test]
fn confirms_a_transaction_switch_only_while_the_session_is_alive() {
    for state in [
        SessionState::Ready,
        SessionState::Executing,
        SessionState::Reconfiguring,
        SessionState::New,
        SessionState::Opening,
        SessionState::Closing,
    ] {
        assert!(
            needs_transaction_switch_confirm(&view(state)),
            "{state:?} still owns a transaction"
        );
    }
    assert!(!needs_transaction_switch_confirm(&view(SessionState::Closed)));
    assert!(!needs_transaction_switch_confirm(&view(SessionState::Lost)));
}

#[test]
fn adapts_the_t083_session_error_mirror() {
    let lost = SessionError {
        code: ApiErrorCode::SessionLost,
        message: "gone".to_owned(),
    };
    assert_eq!(
        session_error_prompt_key(&PromptError::from_session(&lost)),
        PROMPT_DISCONNECTED
    );
    let conflict = SessionError {
        code: ApiErrorCode::Other("ContextConflict".to_owned()),
        message: "stale".to_owned(),
    };
    assert_eq!(
        session_error_prompt_key(&PromptError::from_session(&conflict)),
        PROMPT_REVISION_CONFLICT
    );
    let busy = SessionError {
        code: ApiErrorCode::Other("ResourceBusy".to_owned()),
        message: "busy".to_owned(),
    };
    assert_eq!(
        session_error_prompt_key(&PromptError::from_session(&busy)),
        PROMPT_CANCELLED_RETRY
    );
}
