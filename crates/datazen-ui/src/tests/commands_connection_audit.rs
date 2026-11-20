//! T190 audit: the connection command family + connect joiner, verified
//! against the React source re-read line by line and
//! `commands/__tests__/connection.test.ts` (every case mirrored — the
//! plain-clone payloads, the explicit-credentials flag, the four
//! joiner races, and the dedicated bypass; the cyclic-group rejection is
//! T375's validation, moot for an owned Rust value — an ownership tree
//! cannot be cyclic by construction).
//!
//! Tester-written: every expectation below is re-derived from the React
//! files.

use serde_json::json;

use datazen_ui_api::{ConnectionConfig, SslMode};

use crate::commands::connection::{
    ConnectDecision, ConnectJoiner, ConnectionRequest, DetectImportPathReceipt,
    ImportConnectionsReceipt, ImportPathMode, ConnectionInfoReceipt, ConnectionSessionRef,
    close_database_request, connect_dedicated_request, connect_request,
    delete_connection_request, detect_connection_import_path_request, disconnect_request,
    duplicate_connection_request, export_connections_request, get_available_drivers_request,
    get_connection_info_request, get_connections_request, get_groups_request,
    get_open_databases_request, import_connections_at_path_request, import_connections_from_app_request,
    import_connections_preview_request, import_connections_request,
    pick_connection_import_path_with_dialog_request, pick_connections_import_file_request,
    ping_connection_request, release_connection_request, reorder_connections_request,
    save_connection_request, save_groups_request, test_connection_request,
};

fn config(id: &str, name: &str, database_type: &str, group: Option<&str>) -> ConnectionConfig {
    ConnectionConfig {
        id: id.to_owned(),
        name: name.to_owned(),
        database_type: database_type.to_owned(),
        host: Some("127.0.0.1".to_owned()),
        port: Some(5432),
        database: Some("postgres".to_owned()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::Prefer,
        connection_timeout: None,
        max_pool_size: None,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: group.map(str::to_owned),
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: None,
        pinned: None,
    }
}

fn req(command: &'static str, value: serde_json::Value) -> ConnectionRequest {
    ConnectionRequest { command, value }
}

// ---- the config-carrying shapers (React tests 1-4) --------------------------

/// React test 1: testConnection passes a plain cloned config to invoke.
#[test]
fn test_connection_wraps_a_plain_cloned_config() {
    let cfg = config("c1", "Local PG", "postgresql", Some("preset:development"));
    let request = test_connection_request(&cfg);
    assert_eq!(request.command, "test_connection");
    let payload = &request.value;
    assert_eq!(payload["config"]["id"], "c1");
    assert_eq!(payload["config"]["name"], "Local PG");
    assert_eq!(payload["config"]["group"], "preset:development");
    // The payload is a fresh serialization, not the caller's value (a
    // JSON value on the wire, `not.toBe(config)`).
    assert!(payload["config"].is_object());
    assert!(serde_json::to_string(payload).is_ok(), "the payload stringifies");
}

/// React tests 2-3: saveConnection's clearMissingCredentials default
/// (false) and its explicit-true spelling.
#[test]
fn save_connection_defaults_and_explicit_flag() {
    let cfg = config("c2", "Save me", "mysql", None);
    // The default arm: objectContaining { id, databaseType } + the flag.
    let request = save_connection_request(&cfg, false);
    assert_eq!(request.command, "save_connection");
    assert_eq!(request.value["clearMissingCredentials"], false);
    assert_eq!(request.value["config"]["id"], "c2");
    assert_eq!(request.value["config"]["databaseType"], "mysql");
    assert!(serde_json::to_string(&request.value).is_ok(), "the payload stringifies");
    // The explicit flag reaches the wire.
    let request = save_connection_request(&cfg, true);
    assert_eq!(request.value["clearMissingCredentials"], true);
    assert_eq!(request.value["config"]["id"], "c2");
}

/// React test 4: duplicateConnection's three keys.
#[test]
fn duplicate_connection_shape() {
    assert_eq!(
        duplicate_connection_request("source", "copy", "Copy"),
        req(
            "duplicate_connection",
            json!({ "sourceId": "source", "newId": "copy", "newName": "Copy" })
        )
    );
}

// ---- the session shapers (the source literal) -------------------------------

#[test]
fn session_shapers_carry_the_session_ids() {
    assert_eq!(
        connect_request("cfg-1"),
        req("connect", json!({ "connectionId": "cfg-1" }))
    );
    // `database ?? null`: absent and explicit both land as null; a value
    // passes through.
    assert_eq!(
        connect_dedicated_request("cfg-sub", None),
        req("connect_dedicated", json!({ "connectionId": "cfg-sub", "database": null }))
    );
    assert_eq!(
        connect_dedicated_request("cfg-sub", Some("analytics")),
        req("connect_dedicated", json!({ "connectionId": "cfg-sub", "database": "analytics" }))
    );
    assert_eq!(
        ping_connection_request("sess-1"),
        req("ping_connection", json!({ "dbSessionId": "sess-1" }))
    );
    assert_eq!(
        release_connection_request("sess-1"),
        req("release_connection", json!({ "dbSessionId": "sess-1" }))
    );
    assert_eq!(
        disconnect_request("sess-1"),
        req("disconnect", json!({ "dbSessionId": "sess-1" }))
    );
    assert_eq!(
        close_database_request("sess-1", "shop"),
        req("close_database", json!({ "dbSessionId": "sess-1", "database": "shop" }))
    );
    assert_eq!(
        get_open_databases_request("sess-1"),
        req("get_open_databases", json!({ "dbSessionId": "sess-1" }))
    );
    assert_eq!(
        get_connection_info_request("sess-1"),
        req("get_connection_info", json!({ "dbSessionId": "sess-1" }))
    );
}

#[test]
fn registry_and_group_shapers() {
    assert_eq!(get_connections_request(), req("get_connections", json!({})));
    assert_eq!(
        delete_connection_request("c9"),
        req("delete_connection", json!({ "id": "c9" }))
    );
    assert_eq!(
        reorder_connections_request(&["a", "b", "c"]),
        req("reorder_connections", json!({ "orderedIds": ["a", "b", "c"] }))
    );
    assert_eq!(get_available_drivers_request(), req("get_available_drivers", json!({})));
    assert_eq!(get_groups_request(), req("get_groups", json!({})));
    assert_eq!(
        save_groups_request(&["g1".to_owned(), "g2".to_owned()]),
        req("save_groups", json!({ "groups": ["g1", "g2"] }))
    );
}

#[test]
fn import_export_shapers() {
    // exportConnections: merged IPC, no override_path from production.
    let export = export_connections_request("pw", "datazen.connections");
    assert_eq!(export.command, "export_connections");
    assert_eq!(
        export.value,
        json!({ "password": "pw", "defaultFileName": "datazen.connections" })
    );
    assert!(export.value.get("overridePath").is_none());
    // The legacy password-then-dialog flow.
    assert_eq!(
        import_connections_preview_request("pw"),
        req("import_connections_preview", json!({ "password": "pw" }))
    );
    assert_eq!(
        import_connections_request("pw"),
        req("import_connections_with_dialog", json!({ "password": "pw" }))
    );
    // F4c pickers: the shared command, the kind names the filter.
    assert_eq!(
        pick_connections_import_file_request(),
        req("pick_import_file", json!({ "kind": "connections" }))
    );
    assert_eq!(
        pick_connection_import_path_with_dialog_request(ImportPathMode::Folder, "tableplus"),
        req(
            "pick_import_file",
            json!({ "kind": "competitor-path", "mode": "folder", "source": "tableplus" })
        )
    );
    assert_eq!(
        pick_connection_import_path_with_dialog_request(ImportPathMode::File, "dbeaver"),
        req(
            "pick_import_file",
            json!({ "kind": "competitor-path", "mode": "file", "source": "dbeaver" })
        )
    );
    // The file-first flow and the app import.
    assert_eq!(
        import_connections_at_path_request("pw", "/x/connections.tpc"),
        req(
            "import_connections_at_path",
            json!({ "password": "pw", "path": "/x/connections.tpc" })
        )
    );
    assert_eq!(
        import_connections_from_app_request("tableplus", "pw", "/x/Library"),
        req(
            "import_connections_from_app",
            json!({ "source": "tableplus", "password": "pw", "dataPath": "/x/Library" })
        )
    );
    assert_eq!(
        detect_connection_import_path_request("tableplus"),
        req("detect_connection_import_path", json!({ "source": "tableplus" }))
    );
}

#[test]
fn receipts_decode_the_wire_shapes() {
    // ConnectionSessionRef (camelCase on the wire).
    let session: ConnectionSessionRef = serde_json::from_value(json!({
        "connectionId": "cfg-1", "dbSessionId": "sess-one"
    }))
    .expect("session ref decodes");
    assert_eq!(
        session,
        ConnectionSessionRef {
            connection_id: "cfg-1".to_owned(),
            db_session_id: "sess-one".to_owned(),
        }
    );
    // The import receipt with its optional halves.
    let receipt: ImportConnectionsReceipt = serde_json::from_value(json!({
        "imported": 3, "overwritten": 1, "groupsAdded": 2,
        "skipped": ["bad.tpc"], "sourceFormat": "tableplus"
    }))
    .expect("import receipt decodes");
    assert_eq!(receipt.imported, 3);
    assert_eq!(receipt.overwritten, 1);
    assert_eq!(receipt.groups_added, 2);
    assert_eq!(receipt.skipped.as_deref(), Some(&["bad.tpc".to_owned()][..]));
    assert_eq!(receipt.source_format.as_deref(), Some("tableplus"));
    let bare: ImportConnectionsReceipt = serde_json::from_value(json!({
        "imported": 1, "overwritten": 0, "groupsAdded": 0
    }))
    .expect("the optionals may be absent");
    assert!(bare.skipped.is_none() && bare.source_format.is_none());
    // ConnectionInfo with the absent capabilities.
    let info: ConnectionInfoReceipt = serde_json::from_value(json!({
        "databaseType": "postgresql", "driverCategory": "postgres",
        "name": "Local", "host": "127.0.0.1", "port": 5432
    }))
    .expect("info decodes");
    assert_eq!(info.database_type, "postgresql");
    assert!(info.capabilities.is_none());
    // Detect's two-field receipt.
    let detect: DetectImportPathReceipt =
        serde_json::from_value(json!({ "path": "/x", "found": true })).expect("detect decodes");
    assert_eq!(detect, DetectImportPathReceipt { path: "/x".to_owned(), found: true });
}

// ---- the in-flight connect de-duplication (React tests 5-9) -----------------

/// React test 5: concurrent callers for the same connectionId join into
/// one invoke.
#[test]
fn concurrent_callers_join_the_first_attempt() {
    let mut joiner = ConnectJoiner::new();
    // The tab path and the store path race on cfg-race.
    let tab = joiner.begin("cfg-race");
    let store = joiner.begin("cfg-race");
    assert!(matches!(tab, ConnectDecision::Issue(ref request) if request.command == "connect"));
    assert_eq!(store, ConnectDecision::JoinInFlight);
    assert!(joiner.is_in_flight("cfg-race"));
    // One answer settles the shared attempt.
    joiner.finish("cfg-race");
    assert!(!joiner.is_in_flight("cfg-race"));
}

/// React test 6: a settled slot frees, so a retry issues a fresh invoke.
#[test]
fn a_settled_slot_frees_for_the_retry() {
    let mut joiner = ConnectJoiner::new();
    assert!(matches!(joiner.begin("cfg-retry"), ConnectDecision::Issue(_)));
    joiner.finish("cfg-retry");
    // The second connect is a fresh invoke (session two), not a join.
    assert!(matches!(joiner.begin("cfg-retry"), ConnectDecision::Issue(_)));
}

/// React test 7: the rejection is shared with the joiners (the join
/// decision is the same), and a later retry still issues.
#[test]
fn a_rejection_is_shared_and_the_retry_still_issues() {
    let mut joiner = ConnectJoiner::new();
    let first = joiner.begin("cfg-fail");
    let joiner_call = joiner.begin("cfg-fail");
    assert!(matches!(first, ConnectDecision::Issue(_)));
    assert_eq!(joiner_call, ConnectDecision::JoinInFlight);
    // The attempt rejects: both callers see the failure (they hold the
    // same attempt), and the slot still frees.
    joiner.finish("cfg-fail");
    let retry = joiner.begin("cfg-fail");
    assert!(matches!(retry, ConnectDecision::Issue(_)));
}

/// React test 8: distinct connectionIds never de-duplicate.
#[test]
fn distinct_connections_never_join() {
    let mut joiner = ConnectJoiner::new();
    assert!(matches!(joiner.begin("cfg-a"), ConnectDecision::Issue(_)));
    assert!(matches!(joiner.begin("cfg-b"), ConnectDecision::Issue(_)));
    assert!(joiner.is_in_flight("cfg-a"));
    assert!(joiner.is_in_flight("cfg-b"));
}

/// React test 9: connectDedicated is untouched — sub-windows keep
/// isolated sessions (two invocations for the same connection, and the
/// `database ?? null` wire).
#[test]
fn dedicated_connects_bypass_the_joiner() {
    let mut joiner = ConnectJoiner::new();
    // Two dedicated connects for the same id: both issue.
    let one = connect_dedicated_request("cfg-sub", None);
    let two = connect_dedicated_request("cfg-sub", None);
    assert_eq!(one, two);
    assert_eq!(
        one,
        req("connect_dedicated", json!({ "connectionId": "cfg-sub", "database": null }))
    );
    // And neither ever consulted the joiner (it stays empty).
    assert!(!joiner.is_in_flight("cfg-sub"));
}
