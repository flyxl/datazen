//! Independent audit for T292: `tableSession.ts` port.
//!
//! Fill two branches the coder does not drive: a controller request whose
//! database/schema are absent, and a registered controller that has not opened
//! a session yet (so `snapshot()` yields no view).

use datazen_ui::session::editor_session_controller::{ExecutionTarget, NamespaceTarget, OwnerRef};
use datazen_ui::session::query_panel_session::QueryPanelSessionRegistry;
use datazen_ui::session::table_session::{
    table_manual_handle, table_session_controller_request, TableChangeContext,
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

/// React passes `queryPanelExecutionTarget(connectionId, database, schema)`,
/// whose `?? null` normalization leaves absent database/schema as null (Rust
/// `None`). The coder only pins the both-present case.
#[test]
fn controller_request_normalizes_absent_database_and_schema() {
    let request = table_session_controller_request(
        "panel-1",
        &context(Some("conn-1"), None, None),
        "client-42",
    )
    .expect("a connected context");
    assert_eq!(request.key, "panel-1:table");
    assert_eq!(
        request.target,
        ExecutionTarget {
            connection_id: "conn-1".to_owned(),
            namespace: NamespaceTarget {
                database: None,
                catalog: None,
                schema: None,
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
        }
    );
}

/// React: `peek(key)?.snapshot` is undefined for a controller that mounted but
/// never opened a session. The coder tests an empty registry and a wrong key,
/// but not a correctly-keyed controller with no snapshot.
#[test]
fn manual_handle_is_none_for_a_controller_without_a_snapshot() {
    let mut registry = QueryPanelSessionRegistry::new();
    let _ = registry.get(
        "panel-1:table",
        &ExecutionTarget {
            connection_id: "conn-1".to_owned(),
            namespace: NamespaceTarget::default(),
            object: None,
        },
        &OwnerRef::Editor {
            client_instance_id: "client-42".to_owned(),
            editor_session_id: "panel-1:table".to_owned(),
        },
    );
    assert_eq!(
        table_manual_handle(&registry, "panel-1"),
        Ok(None),
        "a mounted-but-unopened controller yields no handle"
    );
}
