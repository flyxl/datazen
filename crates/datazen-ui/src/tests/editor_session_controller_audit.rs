//! Independent T083 audit: observable state transitions on branches the port's
//! own suite leaves unpinned (refresh, detach/attach feeds, open-failure queue
//! isolation, close-awaiting-open, cancel resolution and observer publishing).
//!
//! Every assertion reads a runtime effect or a published snapshot — never a
//! helper's own report — so a mutation of the controller cannot hide behind a
//! self-satisfying probe.

use std::cell::Cell;
use std::rc::Rc;

use crate::session::editor_session_controller::*;

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

fn view(id: &str, epoch: &str, rev: &str, state: SessionState, namespace: &str) -> SessionView {
    SessionView {
        handle: handle(id, epoch),
        connection_id: "conn-1".to_owned(),
        config_revision: "1".to_owned(),
        owner: owner(),
        initial_target: target(),
        observed_context: SessionContext {
            namespace: NamespaceTarget { database: Some(namespace.to_owned()), ..Default::default() },
            search_path: None,
            effective_identity: None,
            transaction_state: TransactionState::None,
            autocommit: Some(true),
            confidence: ContextConfidence::Confirmed,
        },
        context_revision: rev.to_owned(),
        state,
        attachment_state: AttachmentState::Attached,
        active_execution_id: None,
        expires_at: None,
    }
}

fn tok(key: &str) -> SubmissionToken {
    SubmissionToken { idempotency_key: key.to_owned(), expires_at: 0 }
}

fn receipt(id: &str) -> ExecutionReceipt {
    ExecutionReceipt {
        execution_id: id.to_owned(),
        stream_id: format!("stream-{id}"),
        state: ExecutionState::Running,
    }
}

fn controller() -> EditorSessionController {
    EditorSessionController::new(7, target(), owner())
}

fn issue_id(effects: &[SessionEffect]) -> u64 {
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

fn open_with(c: &mut EditorSessionController, v: SessionView) {
    let id = issue_id(&c.ensure_session().expect("fresh open"));
    let id = open_id(&c.on_token(id, Ok(tok("tok-open"))));
    let _ = c.on_open(id, Ok(OpenSessionReceipt { session: v, attachment_token: "att-1".to_owned() }));
}

fn execute_ok(c: &mut EditorSessionController, id: &str) {
    let effects = c.execute(CommandCall { command: id.to_owned(), input: "{}".to_owned() });
    let effects = c.on_token(issue_id(&effects), Ok(tok(&format!("tok-{id}"))));
    let exe = match effects.first() {
        Some(SessionEffect::Execute { call_id, .. }) => *call_id,
        other => panic!("expected Execute, got {other:?}"),
    };
    let _ = c.on_execute(exe, Ok(receipt(id)));
}

#[test]
fn refresh_feeds_accept_session_and_a_session_error_marks_the_captured_handle_lost() {
    let mut c = controller();
    open_with(&mut c, view("s-1", "e-1", "3", SessionState::Ready, "ns-a"));

    let effects = c.refresh();
    let id = match effects.as_slice() {
        [SessionEffect::GetSession { call_id, handle: h }] => {
            assert_eq!(*h, handle("s-1", "e-1"), "refresh must capture the current handle");
            *call_id
        }
        other => panic!("expected one GetSession, got {other:?}"),
    };
    assert!(c.on_get_session(id, Ok(view("s-1", "e-1", "4", SessionState::Ready, "ns-b"))).is_empty());
    assert_eq!(c.snapshot().unwrap().context_revision, "4");
    assert_eq!(c.latest_target().namespace.database.as_deref(), Some("ns-b"));

    // A stale refresh is refused by acceptSession: neither revision nor namespace move.
    let effects = c.refresh();
    let id = match effects.first() { Some(SessionEffect::GetSession { call_id, .. }) => *call_id, _ => panic!("GetSession") };
    c.on_get_session(id, Ok(view("s-1", "e-1", "2", SessionState::Ready, "stale")));
    assert_eq!(c.snapshot().unwrap().context_revision, "4");
    assert_eq!(c.latest_target().namespace.database.as_deref(), Some("ns-b"));

    // A session error on refresh marks the handle lost (but keeps it bound).
    let effects = c.refresh();
    let id = match effects.first() { Some(SessionEffect::GetSession { call_id, .. }) => *call_id, _ => panic!("GetSession") };
    c.on_get_session(id, Err(SessionError {
        code: ApiErrorCode::SessionNotFound,
        message: "gone".to_owned(),
    }));
    assert_eq!(c.snapshot().unwrap().state, SessionState::Lost);
    assert_eq!(c.snapshot().unwrap().handle, handle("s-1", "e-1"));
}

#[test]
fn detach_and_attach_feed_accept_session_but_never_mark_lost_on_error() {
    let mut c = controller();
    open_with(&mut c, view("s-1", "e-1", "1", SessionState::Ready, "ns-a"));

    let effects = c.detach();
    let id = match effects.as_slice() {
        [SessionEffect::Detach { call_id, handle: h, token }] => {
            assert_eq!(*h, handle("s-1", "e-1"));
            assert_eq!(token, "att-1");
            *call_id
        }
        other => panic!("expected one Detach, got {other:?}"),
    };
    c.on_attachment(id, Ok(view("s-1", "e-1", "2", SessionState::Ready, "ns-detach")));
    assert_eq!(c.snapshot().unwrap().context_revision, "2");
    assert_eq!(c.latest_target().namespace.database.as_deref(), Some("ns-detach"));

    // A null/invalid result on attach is not a session error: the handle stays usable.
    let effects = c.attach();
    let id = match effects.as_slice() {
        [SessionEffect::Attach { call_id, token, .. }] => {
            assert_eq!(token, "att-1", "the old token survives a null receipt");
            *call_id
        }
        other => panic!("expected one Attach, got {other:?}"),
    };
    c.on_attachment(id, Err(SessionError { code: ApiErrorCode::Other("boom".into()), message: "boom".into() }));
    assert_eq!(c.snapshot().unwrap().state, SessionState::Ready);

    // A result for an unknown call is a no-op.
    let before = c.snapshot().unwrap().clone();
    assert!(c.on_attachment(999_999, Ok(view("s-1", "e-1", "9", SessionState::Ready, "unknown"))).is_empty());
    assert_eq!(c.snapshot(), Some(&before));
}

#[test]
fn a_failed_open_isolation_lets_the_next_queued_operation_start_its_own_open() {
    let mut c = controller();
    let first = issue_id(&c.execute(CommandCall { command: "a".into(), input: "{}".into() }));
    assert!(c.execute(CommandCall { command: "b".into(), input: "{}".into() }).is_empty(), "B queues behind A");

    let effects = c.on_token(first, Err(SessionError { code: ApiErrorCode::Other("token down".into()), message: "token down".into() }));
    let second = issue_id(&effects);
    assert_ne!(second, first, "B must allocate a fresh open token, not reuse A's");
    assert!(c.snapshot().is_none(), "a failed open leaves no session");
    assert!(matches!(c.last_result, Some(Err(_))));

    // Drive B's open to completion; B then resumes with its own execute token.
    let effects = c.on_token(second, Ok(tok("tok-b")));
    let open = open_id(&effects);
    let effects = c.on_open(open, Ok(OpenSessionReceipt {
        session: view("s-b", "e-1", "1", SessionState::Ready, "ns-b"),
        attachment_token: "att-b".into(),
    }));
    assert!(
        effects.iter().any(|e| matches!(
            e,
            SessionEffect::IssueToken { operation, .. } if operation == "executeInSession"
        )),
        "B resumes after A's open failed: {effects:?}"
    );
}

#[test]
fn close_awaits_an_in_flight_open_then_closes_the_newly_opened_handle() {
    let mut c = controller();
    let open_token = issue_id(&c.ensure_session().expect("starts open"));
    assert!(c.close(CloseMode::RollbackAndClose).is_empty(), "close waits for the opening");

    let effects = c.on_token(open_token, Ok(tok("tok-open")));
    let open = open_id(&effects);
    let effects = c.on_open(open, Ok(OpenSessionReceipt {
        session: view("s-late", "e-1", "1", SessionState::Ready, "ns-late"),
        attachment_token: "att-late".into(),
    }));
    let close = effects.iter().find_map(|e| match e {
        SessionEffect::Close { handle: h, mode, purpose, .. } => Some((h, *mode, *purpose)),
        _ => None,
    });
    match close {
        Some((h, mode, purpose)) => {
            assert_eq!(h, &handle("s-late", "e-1"), "closes the handle opened while it waited");
            assert_eq!(mode, CloseMode::RollbackAndClose);
            assert_eq!(purpose, ClosePurpose::Operation);
        }
        None => panic!("expected a Close after the awaited open, got {effects:?}"),
    }
}

#[test]
fn cancel_result_is_observable_once_and_a_repeat_is_ignored() {
    let mut c = controller();
    open_with(&mut c, view("s-1", "e-1", "1", SessionState::Ready, "ns"));
    execute_ok(&mut c, "exec-1");

    let effects = c.cancel();
    let effects = c.on_token(issue_id(&effects), Ok(tok("tok-cancel")));
    let id = match effects.first() {
        Some(SessionEffect::Cancel { call_id, .. }) => *call_id,
        other => panic!("expected Cancel, got {other:?}"),
    };
    let receipt = CancelReceipt { execution_id: "exec-1".into() };
    c.on_cancel(id, Ok(receipt.clone()));
    assert_eq!(c.last_result, Some(Ok(OpResult::Cancel(receipt))));
    assert!(c.on_cancel(id, Ok(CancelReceipt { execution_id: "exec-1".into() })).is_empty(), "consumed once");
}

#[test]
fn publish_reaches_subscribers_on_transitions_and_stops_after_unsubscribe() {
    let mut c = controller();
    let hits = Rc::new(Cell::new(0usize));
    let observed = Rc::clone(&hits);
    let id = c.subscribe(Box::new(move || observed.set(observed.get() + 1)));

    open_with(&mut c, view("s-1", "e-1", "1", SessionState::Ready, "ns"));
    assert_eq!(hits.get(), 1, "the open publishes");
    assert!(c.mark_session_lost(Some(&handle("s-1", "e-1"))));
    assert_eq!(hits.get(), 2, "marking lost publishes");
    assert!(c.accept_session(view("s-1", "e-1", "2", SessionState::Closed, "ns")));
    assert_eq!(hits.get(), 3, "accepting a close publishes");

    c.unsubscribe(id);
    assert!(c.accept_session(view("s-1", "e-1", "3", SessionState::Closed, "ns")));
    assert_eq!(hits.get(), 3, "no publish after unsubscribe");
}
