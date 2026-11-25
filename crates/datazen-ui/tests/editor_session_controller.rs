//! T083: `packages/frontend/src/lib/session/EditorSessionController.ts` port.
//!
//! Every scenario below is a React unit-test shape driven through the emitted
//! [`SessionEffect`]s and the host `on_*` result feeds — no fake async, no
//! weakened assertions. State is pinned after each transition.

use datazen_ui::session::editor_session_controller::*;

fn handle(id: &str, epoch: &str) -> SessionHandle {
    SessionHandle { db_session_id: id.to_owned(), runtime_epoch: epoch.to_owned() }
}

fn owner() -> OwnerRef {
    OwnerRef::Editor {
        client_instance_id: "client-1".to_owned(),
        editor_session_id: "editor-1".to_owned(),
    }
}

fn target() -> ExecutionTarget {
    ExecutionTarget {
        connection_id: "conn-1".to_owned(),
        namespace: NamespaceTarget { database: Some("db0".to_owned()), ..Default::default() },
        object: None,
    }
}

fn ns(name: &str) -> NamespaceTarget {
    NamespaceTarget { database: Some(name.to_owned()), ..Default::default() }
}

fn context(confidence: ContextConfidence, namespace: &str) -> SessionContext {
    SessionContext {
        namespace: ns(namespace),
        search_path: None,
        effective_identity: None,
        transaction_state: TransactionState::None,
        autocommit: Some(true),
        confidence,
    }
}

fn session(
    id: &str,
    epoch: &str,
    revision: &str,
    state: SessionState,
    confidence: ContextConfidence,
    namespace: &str,
) -> SessionView {
    SessionView {
        handle: handle(id, epoch),
        connection_id: "conn-1".to_owned(),
        config_revision: "1".to_owned(),
        owner: owner(),
        initial_target: target(),
        observed_context: context(confidence, namespace),
        context_revision: revision.to_owned(),
        state,
        attachment_state: AttachmentState::Attached,
        active_execution_id: None,
        expires_at: None,
    }
}

fn token(key: &str) -> SubmissionToken {
    SubmissionToken { idempotency_key: key.to_owned(), expires_at: 0 }
}

fn receipt(id: &str) -> ExecutionReceipt {
    ExecutionReceipt {
        execution_id: id.to_owned(),
        stream_id: format!("stream-{id}"),
        state: ExecutionState::Running,
    }
}

fn call(command: &str) -> CommandCall {
    CommandCall { command: command.to_owned(), input: "{}".to_owned() }
}

fn other(message: &str) -> SessionError {
    SessionError { code: ApiErrorCode::Other(message.to_owned()), message: message.to_owned() }
}

fn session_error(code: ApiErrorCode) -> SessionError {
    SessionError { code, message: "session rejected".to_owned() }
}

fn controller() -> EditorSessionController {
    EditorSessionController::new(7, target(), owner())
}

fn token_id(effects: &[SessionEffect]) -> u64 {
    match effects.first() {
        Some(SessionEffect::IssueToken { call_id, .. }) => *call_id,
        other => panic!("expected IssueToken, got {other:?}"),
    }
}

fn open_id(effects: &[SessionEffect]) -> u64 {
    match effects.first() {
        Some(SessionEffect::Open { call_id, .. }) => *call_id,
        other => panic!("expected Open, got {other:?}"),
    }
}

/// Drive a fresh controller through `ensureSession` to an open session.
fn open_with(controller: &mut EditorSessionController, view: SessionView) {
    let effects = controller.ensure_session().expect("no session starts an open");
    let id = token_id(&effects);
    let effects = controller.on_token(id, Ok(token("tok-open")));
    let id = open_id(&effects);
    let _ = controller.on_open(id, Ok(OpenSessionReceipt {
        session: view,
        attachment_token: "att-1".to_owned(),
    }));
}

/// Drive ready session + successful execute, leaving an active execution.
fn execute_ok(controller: &mut EditorSessionController, id: &str) {
    let effects = controller.execute(call(id));
    let effects = controller.on_token(token_id(&effects), Ok(token(&format!("tok-{id}"))));
    let exe_id = match effects.first() {
        Some(SessionEffect::Execute { call_id, .. }) => *call_id,
        other => panic!("expected Execute, got {other:?}"),
    };
    let _ = controller.on_execute(exe_id, Ok(receipt(id)));
}

// --- 1. ensureSession: dedup / refusal / confirmed namespace / token payload -

#[test]
fn ensure_session_dedups_concurrent_opens_and_carries_the_exact_token_payload() {
    let mut c = controller();
    let effects = c.ensure_session().expect("starts an open");
    assert_eq!(effects.len(), 1);
    let open_token = match &effects[0] {
        SessionEffect::IssueToken { call_id, operation, handle, context } => {
            assert_eq!(operation, "openSession");
            assert!(handle.is_none(), "no session yet, so the token handle is null");
            assert_eq!(context.connection_id, "conn-1");
            assert_eq!(context.owner, owner());
            *call_id
        }
        other => panic!("expected IssueToken, got {other:?}"),
    };

    // A second caller joins the in-flight `opening`: no new effect, no session.
    assert!(c.ensure_session().expect("dedup").is_empty());
    assert!(c.snapshot().is_none());

    let effects = c.on_token(open_token, Ok(token("key-open")));
    let open_call = match &effects[0] {
        SessionEffect::Open { call_id, request, token } => {
            assert_eq!(request.initial_target, target());
            assert_eq!(request.owner, owner());
            assert_eq!(request.idempotency_key, "key-open");
            assert_eq!(token.idempotency_key, "key-open");
            *call_id
        }
        other => panic!("expected Open, got {other:?}"),
    };

    let view = session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns-open");
    let effects = c.on_open(open_call, Ok(OpenSessionReceipt { session: view.clone(), attachment_token: "att-1".into() }));
    assert!(effects.contains(&SessionEffect::RegisterPageCloser { controller_id: 7 }));
    assert_eq!(c.snapshot(), Some(&view));
    assert_eq!(c.latest_target().namespace.database.as_deref(), Some("ns-open"));
    // `opening` cleared: a third call now reuses the ready session silently.
    assert!(c.ensure_session().expect("ready session").is_empty());
}

#[test]
fn ensure_session_refuses_lost_closed_and_closing_sessions() {
    for state in [SessionState::Lost, SessionState::Closed, SessionState::Closing] {
        let mut c = controller();
        open_with(&mut c, session("s-1", "e-1", "1", state, ContextConfidence::Confirmed, "ns"));
        let error = c.ensure_session().expect_err("refuses reuse");
        assert_eq!(error.code, ApiErrorCode::SessionLost);
        assert_eq!(error.message, "Explicitly open a new session to continue.");
        assert!(c.snapshot().is_some(), "refusal must not replace the handle");
    }
}

#[test]
fn confirmations_move_the_namespace_but_partial_observations_do_not() {
    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Partial, "ns-partial"));
    assert_eq!(c.latest_target().namespace.database.as_deref(), Some("db0"));
    assert!(c.accept_session(session("s-1", "e-1", "6", SessionState::Ready, ContextConfidence::Confirmed, "ns-confirmed")));
    assert_eq!(c.latest_target().namespace.database.as_deref(), Some("ns-confirmed"));
}

// --- 2. serial queue: FIFO, error isolation, stale results -------------------

#[test]
fn the_serial_queue_runs_in_fifo_order_and_one_failure_does_not_poison_it() {
    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));

    let effects = c.execute(call("a"));
    assert!(matches!(effects.first(), Some(SessionEffect::IssueToken { .. })));
    let effects = c.on_token(token_id(&effects), Ok(token("tok-a")));
    let exe_a = match effects.first() {
        Some(SessionEffect::Execute { call_id, request, .. }) => {
            assert_eq!(request.handle, handle("s-1", "e-1"));
            assert_eq!(request.expected_context_revision, "1");
            assert_eq!(request.call, call("a"));
            *call_id
        }
        other => panic!("expected Execute, got {other:?}"),
    };

    // The second call is queued: it emits nothing until the first settles.
    assert!(c.execute(call("b")).is_empty());

    // A non-session failure marks nothing, but the queue advances to op B.
    let effects = c.on_execute(exe_a, Err(other("boom")));
    assert_eq!(effects.len(), 1, "the queued op emits its token effect now");
    let tok_b = token_id(&effects);
    let effects = c.on_token(tok_b, Ok(token("tok-b")));
    let exe_b = match effects.first() {
        Some(SessionEffect::Execute { call_id, .. }) => *call_id,
        other => panic!("expected Execute, got {other:?}"),
    };
    assert_eq!(c.active_execution(), None, "op A failed, no execution recorded");
    assert_eq!(c.snapshot().unwrap().state, SessionState::Ready, "a non-session error leaves the handle usable");

    // An out-of-order duplicate of op A's result must be ignored now.
    assert!(c.on_execute(exe_a, Ok(receipt("exec-a-late"))).is_empty());
    assert_eq!(c.active_execution(), None);

    let effects = c.on_execute(exe_b, Ok(receipt("exec-b")));
    assert!(effects.is_empty());
    assert_eq!(c.active_execution().map(|e| e.execution_id.as_str()), Some("exec-b"));
    assert_eq!(c.last_result, Some(Ok(OpResult::Execution(receipt("exec-b")))));
}

#[test]
fn a_session_failure_marks_lost_and_still_releases_the_queue() {
    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));
    let effects = c.execute(call("a"));
    let effects = c.on_token(token_id(&effects), Ok(token("tok-a")));
    let exe_a = match effects.first() { Some(SessionEffect::Execute { call_id, .. }) => *call_id, _ => panic!("Execute") };

    assert!(c.execute(call("b")).is_empty());
    // A third operation proves the tail kept moving: close() does not need a
    // usable session, so it still emits its call after B fails silently.
    let _ = c.close(CloseMode::RequireNoTransaction);
    let effects = c.on_execute(exe_a, Err(session_error(ApiErrorCode::SessionLost)));
    assert_eq!(c.snapshot().unwrap().state, SessionState::Lost);
    // B's `ensureSession` refuses the lost handle (no effect), then C runs and
    // emits a close — the rejection did not poison the queue.
    assert_eq!(effects.len(), 1, "op C emits its close after op B fails");
    assert!(matches!(effects.first(), Some(SessionEffect::Close { mode: CloseMode::RequireNoTransaction, .. })));
}

// --- 3. setContext: replacement release / execution / token ------------------

#[test]
fn set_context_replacement_clears_execution_and_releases_the_previous_handle() {
    let mut c = controller();
    open_with(&mut c, session("s-old", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns-old"));
    execute_ok(&mut c, "exec-old");
    assert_eq!(c.active_execution().map(|e| e.execution_id.as_str()), Some("exec-old"));

    let effects = c.set_context(ns("ns-new"));
    let effects = c.on_token(token_id(&effects), Ok(token("tok-ctx")));
    let ctx_call = match effects.first() {
        Some(SessionEffect::SetContext { call_id, request, token }) => {
            assert_eq!(request.handle, handle("s-old", "e-1"));
            assert_eq!(request.expected_context_revision, "1");
            assert_eq!(request.desired, ns("ns-new"));
            assert_eq!(token.idempotency_key, "tok-ctx");
            *call_id
        }
        other => panic!("expected SetContext, got {other:?}"),
    };

    let effects = c.on_set_context(ctx_call, Ok(ContextChangeReceipt {
        session: session("s-new", "e-1", "2", SessionState::Ready, ContextConfidence::Confirmed, "ns-new"),
        replaced_session_id: Some("s-old".to_owned()),
        attachment_token: Some("att-2".to_owned()),
    }));
    assert_eq!(c.snapshot().unwrap().handle, handle("s-new", "e-1"));
    assert_eq!(c.latest_target().namespace.database.as_deref(), Some("ns-new"));
    assert_eq!(c.active_execution(), None, "the old execution is unreachable after replacement");
    assert_eq!(c.attachment_token(), Some("att-2"));

    let (close_id, close_handle, mode, purpose) = effects.iter().find_map(|effect| match effect {
        SessionEffect::Close { call_id, handle, mode, purpose, .. } => {
            Some((*call_id, handle.clone(), *mode, *purpose))
        }
        _ => None,
    }).expect("a release effect for the replaced handle");
    assert_eq!(close_handle, handle("s-old", "e-1"));
    assert_eq!(mode, CloseMode::RequireNoTransaction);
    assert_eq!(purpose, ClosePurpose::ReleaseReplaced);

    // The release is best effort: a failure is swallowed, queue advances.
    assert!(c.on_close(close_id, Err(other("already gone"))).is_empty());
    assert_eq!(c.on_close(close_id, Ok(CloseReceipt { db_session_id: "s-old".into(), state: CloseState::Closed })), Vec::new());
    assert!(matches!(c.last_result, Some(Ok(OpResult::Session(_)))));
}

#[test]
fn set_context_keeps_the_token_when_the_receipt_is_null_and_keeps_state_on_failure() {
    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));
    let effects = c.detach();
    assert!(matches!(effects.first(), Some(SessionEffect::Detach { token, .. }) if token == "att-1"));

    let effects = c.set_context(ns("ns-next"));
    let effects = c.on_token(token_id(&effects), Ok(token("tok-ctx")));
    let ctx_call = match effects.first() { Some(SessionEffect::SetContext { call_id, .. }) => *call_id, _ => panic!("SetContext") };
    let effects = c.on_set_context(ctx_call, Ok(ContextChangeReceipt {
        session: session("s-1", "e-1", "2", SessionState::Ready, ContextConfidence::Confirmed, "ns-next"),
        replaced_session_id: None,
        attachment_token: None,
    }));
    assert!(effects.is_empty(), "no replacement -> no release effect");
    assert_eq!(c.attachment_token(), Some("att-1"), "a null receipt token keeps the old one");
    let effects = c.detach();
    assert!(matches!(effects.first(), Some(SessionEffect::Detach { token, .. }) if token == "att-1"));

    // A rejected revision leaves the previous handle and revision bound.
    let effects = c.set_context(ns("ns-fail"));
    let effects = c.on_token(token_id(&effects), Ok(token("tok-fail")));
    let ctx_call = match effects.first() { Some(SessionEffect::SetContext { call_id, .. }) => *call_id, _ => panic!("SetContext") };
    assert!(c.on_set_context(ctx_call, Err(other("revision conflict"))).is_empty());
    assert_eq!(c.snapshot().unwrap().handle, handle("s-1", "e-1"));
    assert_eq!(c.snapshot().unwrap().context_revision, "2");
}

// --- 4. acceptSession guards -------------------------------------------------

#[test]
fn accept_session_rejects_stale_revisions_lost_revival_and_handle_mismatch() {
    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "5", SessionState::Ready, ContextConfidence::Confirmed, "ns-5"));

    assert!(!c.accept_session(session("s-1", "e-1", "4", SessionState::Ready, ContextConfidence::Confirmed, "stale")));
    assert_eq!(c.snapshot().unwrap().context_revision, "5");
    assert_eq!(c.snapshot().unwrap().observed_context.namespace.database.as_deref(), Some("ns-5"));

    assert!(c.accept_session(session("s-1", "e-1", "5", SessionState::Ready, ContextConfidence::Confirmed, "equal")));
    assert_eq!(c.snapshot().unwrap().observed_context.namespace.database.as_deref(), Some("equal"));

    assert!(!c.accept_session(session("other-session", "e-1", "9", SessionState::Ready, ContextConfidence::Confirmed, "mismatch")));
    assert_eq!(c.snapshot().unwrap().handle, handle("s-1", "e-1"));

    assert!(c.mark_session_lost(Some(&handle("s-1", "e-1"))));
    assert!(!c.accept_session(session("s-1", "e-1", "6", SessionState::Ready, ContextConfidence::Confirmed, "revive")));
    assert_eq!(c.snapshot().unwrap().state, SessionState::Lost, "a stale observation cannot revive a lost handle");
    assert!(c.accept_session(session("s-1", "e-1", "6", SessionState::Closed, ContextConfidence::Confirmed, "closed")));
    assert_eq!(c.snapshot().unwrap().state, SessionState::Closed);
}

// --- 5. markSessionLost / error codes / page unload / clone ------------------

#[test]
fn mark_session_lost_keeps_the_handle_and_execution_receipt() {
    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));
    execute_ok(&mut c, "exec-1");
    let before = c.snapshot().unwrap().clone();

    assert!(c.mark_session_lost(Some(&handle("s-1", "e-1"))));
    let after = c.snapshot().unwrap();
    assert_eq!(after.handle, before.handle, "the handle must survive a lost observation");
    assert_eq!(after.state, SessionState::Lost);
    assert_eq!(after.observed_context.transaction_state, TransactionState::Unknown);
    assert_eq!(c.active_execution().map(|e| e.execution_id.as_str()), Some("exec-1"), "the receipt survives too");

    assert!(!c.mark_session_lost(Some(&handle("s-wrong", "e-1"))), "a mismatched handle is a no-op");
    assert_eq!(c.snapshot().unwrap().state, SessionState::Lost);
}

#[test]
fn only_the_three_session_codes_are_accepted_as_session_errors() {
    for code in [ApiErrorCode::SessionLost, ApiErrorCode::SessionNotFound, ApiErrorCode::RuntimeEpochMismatch] {
        let mut c = controller();
        open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));
        let effects = c.get_execution("exec-x".to_owned());
        let id = match effects.first() { Some(SessionEffect::GetExecution { call_id, .. }) => *call_id, _ => panic!("GetExecution") };
        c.on_get_execution(id, Err(session_error(code)));
        assert_eq!(c.snapshot().unwrap().state, SessionState::Lost);
    }

    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));
    let effects = c.get_execution("exec-x".to_owned());
    let id = match effects.first() { Some(SessionEffect::GetExecution { call_id, .. }) => *call_id, _ => panic!("GetExecution") };
    c.on_get_execution(id, Err(other("transport down")));
    assert_eq!(c.snapshot().unwrap().state, SessionState::Ready, "Other codes are not session errors");
}

#[test]
fn a_resource_lost_execution_view_marks_the_captured_handle_lost() {
    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));
    let effects = c.get_execution("exec-x".to_owned());
    let id = match effects.first() { Some(SessionEffect::GetExecution { call_id, .. }) => *call_id, _ => panic!("GetExecution") };
    c.on_get_execution(id, Ok(ExecutionView { execution_id: "exec-x".into(), error_code: Some(ExecutionErrorCode::ResourceLost) }));
    assert_eq!(c.snapshot().unwrap().state, SessionState::Lost);
    assert_eq!(c.snapshot().unwrap().handle, handle("s-1", "e-1"));
}

#[test]
fn the_page_unload_closer_fires_once_with_rollback_and_close() {
    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));
    let effects = c.fire_page_unload();
    let id = match effects.as_slice() {
        [SessionEffect::Close { call_id, handle: closed, mode, purpose }] => {
            assert_eq!(*closed, handle("s-1", "e-1"));
            assert_eq!(*mode, CloseMode::RollbackAndClose, "requireNoTransaction would leak the session");
            assert_eq!(*purpose, ClosePurpose::PageUnload);
            *call_id
        }
        other => panic!("expected one rollbackAndClose, got {other:?}"),
    };
    assert!(c.fire_page_unload().is_empty(), "registered once per controller");
    assert!(c.on_close(id, Err(other("gone"))).is_empty(), "teardown failure is swallowed");
    assert_eq!(c.snapshot().unwrap().state, SessionState::Ready, "the closer does not rebind state");
}

#[test]
fn an_explicit_close_uses_require_no_transaction_and_disarms_the_closer() {
    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));
    let effects = c.close(CloseMode::RequireNoTransaction);
    let id = match effects.first() {
        Some(SessionEffect::Close { call_id, mode, purpose, .. }) => {
            assert_eq!(*mode, CloseMode::RequireNoTransaction);
            assert_eq!(*purpose, ClosePurpose::Operation);
            *call_id
        }
        other => panic!("expected Close, got {other:?}"),
    };
    let effects = c.on_close(id, Ok(CloseReceipt { db_session_id: "s-1".into(), state: CloseState::Closed }));
    assert!(effects.contains(&SessionEffect::UnregisterPageCloser { controller_id: 7 }));
    assert_eq!(c.snapshot().unwrap().state, SessionState::Closed);
    assert_eq!(c.attachment_token(), None);
    assert!(c.fire_page_unload().is_empty(), "the closer was disarmed");
}

#[test]
fn clone_rejects_a_second_editor_with_the_same_pane_id() {
    let c = controller();
    assert!(matches!(c.clone(8, owner()), Err("Cloned editors require a distinct owner ID")));
    let distinct = OwnerRef::Editor {
        client_instance_id: "client-1".to_owned(),
        editor_session_id: "editor-2".to_owned(),
    };
    let clone = c.clone(8, distinct).expect("a distinct pane id is allowed");
    assert!(clone.snapshot().is_none());
    assert_eq!(clone.latest_target(), c.latest_target());
    let job = OwnerRef::Job { job_id: "j".to_owned(), stage_id: "s".to_owned() };
    assert!(c.clone(9, job).is_ok(), "non-editor owners have no editorSessionId to collide");
}

#[test]
fn open_new_session_refuses_a_live_session_then_reopens_a_lost_one() {
    let mut c = controller();
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));
    let effects = c.open_new_session();
    assert!(effects.is_empty(), "a live session must be closed first");
    assert!(matches!(&c.last_result, Some(Err(_))));
    assert_eq!(c.snapshot().unwrap().handle, handle("s-1", "e-1"));

    assert!(c.mark_session_lost(Some(&handle("s-1", "e-1"))));
    let effects = c.open_new_session();
    assert!(matches!(effects.first(), Some(SessionEffect::IssueToken { operation, .. }) if operation == "openSession"));
    assert_eq!(c.snapshot(), None, "the lost handle is cleared before reopening");
}

#[test]
fn cancel_needs_an_execution_and_carries_the_owner_token_payload() {
    let mut c = controller();
    assert!(c.cancel().is_empty(), "no execution -> no cancel");
    open_with(&mut c, session("s-1", "e-1", "1", SessionState::Ready, ContextConfidence::Confirmed, "ns"));
    execute_ok(&mut c, "exec-1");
    let effects = c.cancel();
    match &effects[0] {
        SessionEffect::IssueToken { operation, handle: token_handle, context, .. } => {
            assert_eq!(operation, "cancelExecution");
            assert_eq!(*token_handle, Some(handle("s-1", "e-1")));
            assert_eq!(context.connection_id, "conn-1");
            assert_eq!(context.owner, owner());
        }
        other => panic!("expected IssueToken, got {other:?}"),
    }
    let id = token_id(&effects);
    let effects = c.on_token(id, Ok(token("tok-cancel")));
    match &effects[0] {
        SessionEffect::Cancel { execution_id, token, .. } => {
            assert_eq!(execution_id, "exec-1");
            assert_eq!(token.idempotency_key, "tok-cancel");
        }
        other => panic!("expected Cancel, got {other:?}"),
    }
}

#[test]
fn create_controller_requires_both_parts_of_the_editor_identity() {
    let creation = create_editor_session_controller(target(), "editor-1".to_owned());
    assert_eq!(creation.effects(), &[SessionEffect::Identity { call_id: 1 }]);

    let creation = create_editor_session_controller(target(), "editor-1".to_owned());
    assert!(matches!(creation.complete(1, Err(other("x"))), Err("Missing editor owner identity")));

    let creation = create_editor_session_controller(target(), "".to_owned());
    assert!(matches!(
        creation.complete(1, Ok(PlatformIdentity { client_instance_id: "client-1".into() })),
        Err("Missing editor owner identity")
    ));

    let creation = create_editor_session_controller(target(), "editor-1".to_owned());
    let c = creation
        .complete(1, Ok(PlatformIdentity { client_instance_id: "client-1".into() }))
        .expect("both identities present");
    assert!(c.snapshot().is_none());
    assert_eq!(c.latest_target(), &target());
}
