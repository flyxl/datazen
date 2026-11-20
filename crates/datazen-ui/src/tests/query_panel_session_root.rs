//! T208: `lib/session/queryPanelSessionRoot.ts` port — real branch tests.
//!
//! Pinned values are the React test's `client-42`, `panel-query-1::pane-b`
//! and `cfg-1 / app / public`. Each test gets a fresh root instance (the Rune
//! equivalent of `vi.resetModules()`), so cached identity and singleton results
//! cannot leak between cases.

use std::rc::Rc;

use crate::session::editor_session_controller::{ExecutionTarget, NamespaceTarget, OwnerRef};
use crate::session::query_panel_session_root::{
    bind_session_client, editor_owner_for_pane, get_query_panel_session_registry,
    query_panel_client_instance_id, query_panel_execution_target,
    reset_query_panel_session_root, BackendClientStub, QueryPanelSessionRoot, RootError,
};

#[test]
fn fails_loudly_when_the_host_issues_no_client_identity() {
    let client = BackendClientStub::new("");
    let mut root = QueryPanelSessionRoot::new();
    root.bind_client(client.clone());

    let error = root
        .query_panel_client_instance_id()
        .err()
        .expect("an empty identity is a loud failure");
    assert_eq!(error, RootError::MissingClientIdentity);
    assert_eq!(error.message(), "Missing client identity");
    // React caches the rejected promise: a second call still fails and the
    // host is asked only once.
    assert_eq!(
        root.query_panel_client_instance_id(),
        Err(RootError::MissingClientIdentity)
    );
    assert_eq!(client.identity_calls(), 1);
}

#[test]
fn mints_one_editor_owner_per_pane_key_from_a_single_client_identity() {
    let client = BackendClientStub::new("client-42");
    let mut root = QueryPanelSessionRoot::new();
    root.bind_client(client.clone());

    let owner = root
        .editor_owner_for_pane("panel-query-1::pane-b")
        .expect("owner for the split pane");
    assert_eq!(
        owner,
        OwnerRef::Editor {
            client_instance_id: "client-42".to_owned(),
            editor_session_id: "panel-query-1::pane-b".to_owned(),
        },
        "the composite pane key is the editor session id"
    );

    let sibling = root
        .editor_owner_for_pane("panel-query-1")
        .expect("owner for the default pane");
    assert_eq!(
        sibling,
        OwnerRef::Editor {
            client_instance_id: "client-42".to_owned(),
            editor_session_id: "panel-query-1".to_owned(),
        },
        "a different pane key is a different owner"
    );
    assert_ne!(owner, sibling);
    assert_eq!(client.identity_calls(), 1, "identity fetched once per process");
}

#[test]
fn builds_an_execution_target_with_a_normalized_namespace() {
    assert_eq!(
        query_panel_execution_target("cfg-1", Some("app"), Some("public")),
        ExecutionTarget {
            connection_id: "cfg-1".to_owned(),
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
        query_panel_execution_target("cfg-1", None, None),
        ExecutionTarget {
            connection_id: "cfg-1".to_owned(),
            namespace: NamespaceTarget {
                database: None,
                catalog: None,
                schema: None,
                path: Vec::new(),
            },
            object: None,
        },
        "nullish database/schema become None"
    );
}

#[test]
fn keeps_the_registry_a_process_wide_singleton_on_the_bound_client() {
    reset_query_panel_session_root();
    let client = BackendClientStub::new("client-42");
    bind_session_client(client.clone());

    let first = get_query_panel_session_registry().expect("registry on the bound root");
    let second = get_query_panel_session_registry().expect("same registry again");
    assert!(
        Rc::ptr_eq(&first, &second),
        "every pane shares one registry instance"
    );
    assert_eq!(
        client.identity_calls(),
        0,
        "constructing the registry does not read the client identity"
    );
    assert!(
        first.borrow().shares_client(&client),
        "the registry holds the bound client (via its lazy facade)"
    );
    reset_query_panel_session_root();
}

#[test]
fn unbound_backend_fails_loud_at_first_wire_use() {
    let mut root = QueryPanelSessionRoot::new();
    assert_eq!(
        root.query_panel_session_client().err(),
        Some(RootError::BackendUnbound)
    );
    assert_eq!(
        root.get_query_panel_session_registry().err(),
        Some(RootError::BackendUnbound),
        "bookkeeping cannot construct a registry on an unbound app"
    );
    assert_eq!(
        root.query_panel_client_instance_id(),
        Err(RootError::BackendUnbound)
    );
}

#[test]
fn reset_drops_the_cached_identity_and_registry() {
    let client = BackendClientStub::new("client-42");
    let mut root = QueryPanelSessionRoot::new();
    root.bind_client(client.clone());
    assert_eq!(root.query_panel_client_instance_id(), Ok("client-42".to_owned()));
    let before = root.get_query_panel_session_registry().expect("first registry");
    root.reset();

    assert_eq!(
        root.query_panel_client_instance_id(),
        Err(RootError::BackendUnbound),
        "the cached identity is gone after teardown"
    );
    // Rebinding yields a fresh registry object, not the old singleton.
    root.bind_client(client);
    let after = root.get_query_panel_session_registry().expect("new registry");
    assert!(!Rc::ptr_eq(&before, &after), "reset minted a new root registry");
}
