//! Independent audit for T208: `queryPanelSessionRoot.ts` port.
//!
//! The coder's suite drives the standalone `QueryPanelSessionRoot` methods,
//! but the module's real surface is the thread-local ambient functions that
//! mirror React's named exports. Those are untested. These tests also pin the
//! independent `?? null` normalization of database and schema, and the exact
//! fail-loud messages.

use datazen_ui::session::editor_session_controller::{ExecutionTarget, NamespaceTarget, OwnerRef};
use datazen_ui::session::query_panel_session_root::{
    bind_session_client, editor_owner_for_pane, get_query_panel_session_registry,
    query_panel_client_instance_id, query_panel_execution_target,
    reset_query_panel_session_root, BackendClientStub, RootError,
};

/// The ambient `query_panel_client_instance_id` / `editor_owner_for_pane`
/// exports must read the same thread-local root that `bind_session_client`
/// writes, cache the identity once, and fail loud after teardown.
#[test]
fn ambient_exports_read_the_bound_root_and_fail_loud_after_reset() {
    reset_query_panel_session_root();
    let client = BackendClientStub::new("client-42");
    bind_session_client(client.clone());

    assert_eq!(
        query_panel_client_instance_id(),
        Ok("client-42".to_owned()),
        "ambient identity read"
    );
    assert_eq!(
        query_panel_client_instance_id(),
        Ok("client-42".to_owned()),
        "the identity result is cached"
    );
    assert_eq!(client.identity_calls(), 1, "the host is asked only once");

    let pane = editor_owner_for_pane("panel-query-1::pane-b").expect("ambient owner");
    assert_eq!(
        pane,
        OwnerRef::Editor {
            client_instance_id: "client-42".to_owned(),
            editor_session_id: "panel-query-1::pane-b".to_owned(),
        },
        "the composite pane key becomes the editor session id"
    );
    let sibling = editor_owner_for_pane("panel-query-1").expect("ambient sibling owner");
    assert_ne!(pane, sibling, "a duplicated tab gets a distinct owner");
    assert_eq!(client.identity_calls(), 1, "owner mints reuse the cached identity");

    // The ambient registry export is the same singleton the owner reads from.
    let registry = get_query_panel_session_registry().expect("ambient registry");
    assert!(
        registry.borrow().shares_client(&client),
        "the ambient registry is bound to the same client"
    );

    reset_query_panel_session_root();
    assert_eq!(
        query_panel_client_instance_id(),
        Err(RootError::BackendUnbound),
        "teardown drops the cached identity"
    );
    assert_eq!(
        editor_owner_for_pane("panel-query-1"),
        Err(RootError::BackendUnbound),
        "owner minting fails loud after teardown"
    );
    assert_eq!(
        get_query_panel_session_registry().err(),
        Some(RootError::BackendUnbound),
        "teardown drops the ambient registry"
    );
}

/// React normalizes `database` and `schema` independently with `?? null`.
/// Pin all four combinations, not just both-present / both-absent.
#[test]
fn execution_target_normalizes_database_and_schema_independently() {
    let cases: [(&str, Option<&str>, Option<&str>, Option<&str>, Option<&str>); 4] = [
        ("cfg-1", Some("app"), Some("public"), Some("app"), Some("public")),
        ("cfg-1", Some("app"), None, Some("app"), None),
        ("cfg-1", None, Some("public"), None, Some("public")),
        ("cfg-1", None, None, None, None),
    ];
    for (connection, database, schema, want_db, want_schema) in cases {
        let target: ExecutionTarget = query_panel_execution_target(connection, database, schema);
        assert_eq!(target.connection_id, "cfg-1");
        assert_eq!(
            target.namespace,
            NamespaceTarget {
                database: want_db.map(str::to_owned),
                catalog: None,
                schema: want_schema.map(str::to_owned),
                path: Vec::new(),
            },
            "db={database:?} schema={schema:?} normalizes independently"
        );
        assert!(target.object.is_none());
    }
}

/// The two React-host error sentences must be exact (no invented copy).
#[test]
fn root_error_messages_are_exact() {
    assert_eq!(
        RootError::BackendUnbound.message(),
        "Backend client is not bound"
    );
    assert_eq!(
        RootError::MissingClientIdentity.message(),
        "Missing client identity"
    );
}
