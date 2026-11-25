//! T292: `lib/session/tableSession.ts` port — real branch tests.
//!
//! Pure helpers; every branch is pinned with exact outputs. The registry-backed
//! handle test uses the real T183 registry and a controller that has accepted a
//! session, so no stand-in or fake async is involved.

use datazen_ui::session::editor_session_controller::{
    AttachmentState, ContextConfidence, ExecutionTarget, NamespaceTarget, OpenSessionReceipt,
    OwnerRef, SessionContext, SessionEffect, SessionHandle, SessionState, SessionView,
    SubmissionToken, TransactionState,
};
use datazen_ui::session::query_panel_session::QueryPanelSessionRegistry;
use datazen_ui::session::table_session::{
    table_manual_handle, table_session_controller_request, table_session_key, TableChangeContext,
    TableSessionError,
};

fn context(connection: Option<&str>, database: Option<&str>, schema: Option<&str>) -> TableChangeContext {
    TableChangeContext {
        connection_id: connection.map(str::to_owned),
        db_session_id: "db-session".to_owned(),
        driver_type: Some("postgresql".to_owned()),
        database: database.map(str::to_owned),
        schema: schema.map(str::to_owned),
        table: "users".to_owned(),
    }
}

fn handle() -> SessionHandle {
    SessionHandle {
        db_session_id: "s".to_owned(),
        runtime_epoch: "epoch".to_owned(),
    }
}

fn view(state: SessionState, transaction: TransactionState) -> SessionView {
    SessionView {
        handle: handle(),
        connection_id: "conn-1".to_owned(),
        config_revision: "1".to_owned(),
        owner: OwnerRef::Editor {
            client_instance_id: "client-42".to_owned(),
            editor_session_id: "panel-1:table".to_owned(),
        },
        initial_target: ExecutionTarget {
            connection_id: "conn-1".to_owned(),
            namespace: NamespaceTarget::default(),
            object: None,
        },
        observed_context: SessionContext {
            namespace: NamespaceTarget::default(),
            search_path: None,
            effective_identity: None,
            transaction_state: transaction,
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

fn registry_with(key: &str, state: SessionState, transaction: TransactionState) -> QueryPanelSessionRegistry {
    let mut registry = QueryPanelSessionRegistry::new();
    let controller = registry.get(
        key,
        &ExecutionTarget {
            connection_id: "conn-1".to_owned(),
            namespace: NamespaceTarget::default(),
            object: None,
        },
        &OwnerRef::Editor {
            client_instance_id: "client-42".to_owned(),
            editor_session_id: key.to_owned(),
        },
    );
    // Establish the session through the real open handshake, as the host does.
    let effects = controller.ensure_session().expect("a fresh controller opens");
    let token_id = match effects.first() {
        Some(SessionEffect::IssueToken { call_id, .. }) => *call_id,
        other => panic!("expected IssueToken, got {other:?}"),
    };
    let effects = controller.on_token(
        token_id,
        Ok(SubmissionToken {
            idempotency_key: "tok".to_owned(),
            expires_at: 0,
        }),
    );
    let open_id = match effects.first() {
        Some(SessionEffect::Open { call_id, .. }) => *call_id,
        other => panic!("expected Open, got {other:?}"),
    };
    let _ = controller.on_open(
        open_id,
        Ok(OpenSessionReceipt {
            session: view(state, transaction),
            attachment_token: "att".to_owned(),
        }),
    );
    registry
}

#[test]
fn table_session_key_appends_a_suffix_without_flattening_panes() {
    assert_eq!(table_session_key("panel-1"), "panel-1:table");
    assert_eq!(
        table_session_key("panel-1::pane-b"),
        "panel-1::pane-b:table"
    );
    assert_ne!(
        table_session_key("panel-1"),
        table_session_key("panel-1::pane-b"),
        "a split pane owns its own table session"
    );
}

#[test]
fn controller_request_fails_loud_without_a_connection() {
    for connection in [None, Some("")] {
        let error = table_session_controller_request("panel-1", &context(connection, None, None), "client-42")
            .err()
            .expect("a table change without a connection is rejected");
        assert_eq!(error, TableSessionError::ConnectionRequired);
        assert_eq!(error.message(), "Table connection required");
    }
}

#[test]
fn controller_request_derives_the_exact_registry_lookup_inputs() {
    let request = table_session_controller_request(
        "panel-1",
        &context(Some("conn-1"), Some("app"), Some("public")),
        "client-42",
    )
    .expect("a connected context");
    assert_eq!(request.key, "panel-1:table");
    assert_eq!(
        request.target,
        ExecutionTarget {
            connection_id: "conn-1".to_owned(),
            namespace: NamespaceTarget {
                database: Some("app".to_owned()),
                catalog: None,
                schema: Some("public".to_owned()),
                path: Vec::new(),
            },
            object: None,
        }
    );
    assert_eq!(
        request.owner,
        OwnerRef::Editor {
            client_instance_id: "client-42".to_owned(),
            editor_session_id: "panel-1:table".to_owned(),
        },
        "the table pane key is the editor owner id"
    );
}

#[test]
fn manual_handle_is_none_without_a_registered_session_or_uses_the_prefixed_key() {
    let empty = QueryPanelSessionRegistry::new();
    assert_eq!(table_manual_handle(&empty, "panel-1"), Ok(None));

    // A controller stored under the bare panel id is invisible to the table
    // helper; only the `:table` key counts.
    let bare = registry_with("panel-1", SessionState::Ready, TransactionState::Active);
    assert_eq!(table_manual_handle(&bare, "panel-1"), Ok(None));
    let prefixed = registry_with("panel-1:table", SessionState::Ready, TransactionState::Active);
    assert_eq!(table_manual_handle(&prefixed, "panel-1"), Ok(Some(handle())));
}

#[test]
fn manual_handle_requires_an_active_or_aborted_transaction() {
    let expected = Ok(Some(handle()));
    assert_eq!(
        table_manual_handle(
            &registry_with("panel-1:table", SessionState::Ready, TransactionState::Active),
            "panel-1"
        ),
        expected
    );
    assert_eq!(
        table_manual_handle(
            &registry_with("panel-1:table", SessionState::Ready, TransactionState::Aborted),
            "panel-1"
        ),
        expected
    );
    for transaction in [
        TransactionState::None,
        TransactionState::Unknown,
        TransactionState::Unsupported,
    ] {
        assert_eq!(
            table_manual_handle(
                &registry_with("panel-1:table", SessionState::Ready, transaction),
                "panel-1"
            ),
            Ok(None),
            "{transaction:?} does not need a manual handle"
        );
    }
}

#[test]
fn manual_handle_fails_on_a_closing_closed_or_lost_session() {
    for state in [SessionState::Closing, SessionState::Closed, SessionState::Lost] {
        let error = table_manual_handle(
            &registry_with("panel-1:table", state, TransactionState::Active),
            "panel-1",
        )
        .err()
        .expect("a stale session must fail loudly");
        assert_eq!(error, TableSessionError::SessionLost);
        assert_eq!(
            error.message(),
            "Table session lost; explicitly open a new session."
        );
    }
}
