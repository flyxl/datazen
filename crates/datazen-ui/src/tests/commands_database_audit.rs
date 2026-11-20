//! T258 audit: the database command family, verified against the React
//! source re-read line by line (`commands/__tests__/database.metadata.test.ts`
//! and the filter journeys pin the table-read rail; the shaper rules
//! below are re-derived from the source literal).
//!
//! Tester-written: the coder's module is treated as a black box.

use serde_json::json;

use crate::commands::database::{
    database_discovery_target, er_table_schema_request, get_databases_request,
    get_object_ddl_request, get_privileges_request, list_catalog_request, list_objects_request,
    metadata_target, table_infos_from_catalog, validate_catalog, validate_ddl,
    validate_objects, validate_table_schema, DatabaseLegacyRequest, DatabaseMethod,
    DatabaseTarget, DiscoveryDecision, DiscoveryJoiner, TargetCommandRequest,
};

fn target(connection: &str, database: Option<&str>, schema: Option<&str>) -> DatabaseTarget {
    metadata_target(connection, database, schema, None)
}

#[test]
fn the_target_shapes_follow_targetcommand() {
    // metadataTarget: the namespace carries the explicit halves, no
    // object for a catalog read.
    let t = target("conn-1", Some("shop"), Some("public"));
    assert_eq!(t.connection_id, "conn-1");
    assert_eq!(t.namespace.database.as_deref(), Some("shop"));
    assert_eq!(t.namespace.schema.as_deref(), Some("public"));
    assert!(t.object.is_none());
    // A table-scoped read carries the relation object.
    let t = metadata_target("conn-1", Some("shop"), Some("public"), Some("orders"));
    let object = t.object.expect("the relation object");
    assert_eq!(object.kind, "relation");
    assert_eq!(object.name, "orders");
    assert!(object.signature.is_none());
    // databaseDiscoveryTarget: the namespace cleared whole (discovery
    // belongs to the connection, before any database is selected).
    let t = database_discovery_target("conn-1");
    assert_eq!(t.connection_id, "conn-1");
    assert!(t.namespace.database.is_none());
    assert!(t.namespace.schema.is_none());
    assert!(t.namespace.catalog.is_none());
    assert!(t.namespace.path.is_empty());
}

#[test]
fn the_target_shapers_wire_shapes() {
    // getDatabases: list_databases, empty input.
    let request = get_databases_request(&database_discovery_target("conn-1"));
    assert_eq!(request.command, "list_databases");
    assert_eq!(request.input, json!({}));
    // listCatalog: database + schema ?? null.
    let request = list_catalog_request(&target("conn-1", Some("shop"), Some("public")), "shop", Some("public"));
    assert_eq!(request.command, "list_catalog");
    assert_eq!(request.input, json!({ "database": "shop", "schema": "public" }));
    let request = list_catalog_request(&target("conn-1", Some("shop"), None), "shop", None);
    assert_eq!(request.input, json!({ "database": "shop", "schema": null }));
    // listObjects: kind + database ?? null + schema ?? null.
    let request = list_objects_request(&target("conn-1", None, None), "view", None, None);
    assert_eq!(request.command, "list_objects");
    assert_eq!(request.input, json!({ "kind": "view", "database": null, "schema": null }));
    // getObjectDdl: the seven keys, camelCase, ?? null on all optionals.
    let request = get_object_ddl_request(
        &metadata_target("conn-1", Some("shop"), Some("public"), Some("orders")),
        "table",
        "orders",
        Some("public"),
        None,
        None,
        Some("orders_copy"),
        Some("shop"),
    );
    assert_eq!(request.command, "get_object_ddl");
    assert_eq!(
        request.input,
        json!({
            "kind": "table", "name": "orders", "schema": "public", "signature": null,
            "targetSchema": null, "targetName": "orders_copy", "database": "shop",
        })
    );
}

#[test]
fn the_validations_are_the_react_guards() {
    // Catalog: relations AND schemas must be arrays.
    assert!(validate_catalog(&json!({"relations": [], "schemas": []})).is_ok());
    assert_eq!(
        validate_catalog(&json!({"relations": []})),
        Err("Invalid catalog result".to_owned())
    );
    assert_eq!(
        validate_catalog(&json!({"relations": "x", "schemas": []})),
        Err("Invalid catalog result".to_owned())
    );
    // Objects: the objects array.
    assert!(validate_objects(&json!({"objects": []})).is_ok());
    assert_eq!(
        validate_objects(&json!({})),
        Err("Invalid object catalog result".to_owned())
    );
    // DDL: a string.
    assert!(validate_ddl(&json!({"ddl": "CREATE TABLE x (a int)"})).is_ok());
    assert_eq!(validate_ddl(&json!({"ddl": 42})), Err("Invalid object DDL result".to_owned()));
    assert_eq!(validate_ddl(&json!({})), Err("Invalid object DDL result".to_owned()));
    // Table schema: schema.columns must be an array.
    assert!(validate_table_schema(&json!({"schema": {"columns": []}})).is_ok());
    assert_eq!(
        validate_table_schema(&json!({"schema": {"columns": "nope"}})),
        Err("Invalid schema result".to_owned())
    );
    assert_eq!(
        validate_table_schema(&json!({})),
        Err("Invalid schema result".to_owned())
    );
}

#[test]
fn list_tables_projects_the_catalog_relations() {
    let catalog = json!({
        "schemas": [],
        "relations": [
            { "ref": { "name": "orders", "schema": "public", "database": "shop" }, "kind": "table", "rowCount": 100 },
            { "ref": { "name": "v_totals", "schema": "public", "database": "shop" }, "kind": "view" },
        ],
    });
    let rows = table_infos_from_catalog(&catalog).expect("projects");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["name"], "orders");
    assert_eq!(rows[0]["schema"], "public");
    assert_eq!(rows[0]["tableType"], "table");
    assert_eq!(rows[0]["rowCount"], 100);
    assert_eq!(rows[1]["name"], "v_totals");
    assert_eq!(rows[1]["tableType"], "view");
    assert!(rows[1]["rowCount"].is_null());
    // A malformed catalog rejects, never a half-empty list.
    assert!(table_infos_from_catalog(&json!({})).is_err());
}

#[test]
fn the_er_loop_re_anchors_each_relation() {
    // ER metadata must never borrow the query editor's session resource:
    // the base target is captured once, every relation read re-anchors
    // the namespace to the relation's exact identity.
    let base = target("conn-1", Some("shop"), Some("public"));
    let relation = json!({ "ref": { "name": "orders", "schema": "app", "database": "shop2" } });
    let request = er_table_schema_request(&base, &relation).expect("re-anchors");
    assert_eq!(request.command, "get_table_schema");
    assert_eq!(request.target.namespace.database.as_deref(), Some("shop2"));
    assert_eq!(request.target.namespace.schema.as_deref(), Some("app"));
    assert_eq!(request.target.object.as_ref().map(|o| o.name.as_str()), Some("orders"));
    assert_eq!(
        request.input,
        json!({ "database": "shop2", "schema": "app", "table": "orders" })
    );
    // The base target is not mutated (ownership carried per request).
    assert_eq!(base.namespace.database.as_deref(), Some("shop"));
}

#[test]
fn the_domain_methods_wire_names() {
    use DatabaseMethod as M;
    let cases = [
        (M::GetTableData, "getTableData"),
        (M::PreviewTableReadSql, "previewTableReadSql"),
        (M::PreviewPendingChanges, "previewPendingChanges"),
        (M::CommitPendingChanges, "commitPendingChanges"),
        (M::EstimateTableRows, "estimateTableRows"),
        (M::ResolveForeignKeyTargets, "resolveForeignKeyTargets"),
        (M::ListInboundForeignKeys, "listInboundForeignKeys"),
        (M::SearchTableValues, "searchTableValues"),
        (M::ListNamedViews, "listNamedViews"),
        (M::SaveNamedView, "saveNamedView"),
        (M::DeleteNamedView, "deleteNamedView"),
    ];
    for (method, name) in cases {
        assert_eq!(method.wire_name(), name);
    }
}

#[test]
fn the_privileges_face_stays_legacy() {
    assert_eq!(
        get_privileges_request("sess-1"),
        DatabaseLegacyRequest { command: "get_privileges", value: json!({ "dbSessionId": "sess-1" }) }
    );
}

// ---- the discovery joiner -----------------------------------------------------

#[test]
fn the_discovery_join_key_is_the_react_tuple() {
    assert_eq!(
        DiscoveryJoiner::key(Some("sess-1"), None),
        "[\"session\",\"sess-1\"]"
    );
    assert_eq!(
        DiscoveryJoiner::key(None, Some("conn-1")),
        "[\"connection\",\"conn-1\"]"
    );
}

#[test]
fn concurrent_discoveries_join_and_settle_frees() {
    let mut joiner = DiscoveryJoiner::new();
    // The session boundary (no explicit owner).
    let first = joiner.begin(Some("sess-1"), None);
    assert!(matches!(first, DiscoveryDecision::Issue(ref request) if request.command == "list_databases"));
    // A concurrent caller for the same session joins.
    assert_eq!(joiner.begin(Some("sess-1"), None), DiscoveryDecision::JoinInFlight);
    // A different session issues its own.
    assert!(matches!(joiner.begin(Some("sess-2"), None), DiscoveryDecision::Issue(_)));
    // A connection-scoped discovery is a DIFFERENT key (dedicated
    // endpoint sessions share connection-scoped discovery).
    assert!(matches!(joiner.begin(None, Some("conn-1")), DiscoveryDecision::Issue(_)));
    assert!(joiner.is_in_flight(Some("sess-1"), None));
    // Settling frees the slot for the next caller.
    joiner.finish(Some("sess-1"), None);
    assert!(!joiner.is_in_flight(Some("sess-1"), None));
    assert!(matches!(joiner.begin(Some("sess-1"), None), DiscoveryDecision::Issue(_)));
    // Connection-scoped: same connection from two sessions shares.
    let mut joiner = DiscoveryJoiner::new();
    assert!(matches!(joiner.begin(Some("sess-a"), Some("conn-1")), DiscoveryDecision::Issue(_)));
    assert_eq!(
        joiner.begin(Some("sess-b"), Some("conn-1")),
        DiscoveryDecision::JoinInFlight,
        "the connection key dedups across sessions"
    );
}
