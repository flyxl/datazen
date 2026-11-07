//! Integration tests for schema object Driver Commands.

use datazen_driver_api::{
    execute_schema_object_command, schema_object_command_definitions, ConnectionConfig,
    DatabaseDriver, ObjectKind, SslMode,
};
use datazen_driver_sqlite::SqliteDriver;
use serde_json::json;

fn test_config(path: &str) -> ConnectionConfig {
    ConnectionConfig {
        id: "sqlite-schema-objects".into(),
        name: "test".into(),
        database_type: "sqlite".into(),
        host: None,
        port: None,
        database: Some(path.into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::Prefer,
        connection_timeout: 30,
        max_pool_size: 10,
        ssh_tunnel: None,
        tunnel_kind: None,
        tunnel_id: None,
        http_proxy_tunnel: None,
        websocket_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
        pinned: false,
    }
}

#[test]
fn command_definitions_include_schema_object_commands() {
    let driver = SqliteDriver::new();
    let defs = driver.command_definitions();
    let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
    assert!(ids.contains(&"list_objects"));
    assert!(ids.contains(&"get_object_ddl"));
    assert!(ids.contains(&"get_object_dependencies"));
    assert!(ids.contains(&"list_privileges"));
    assert_eq!(schema_object_command_definitions().len(), 4);
}

#[tokio::test]
async fn list_objects_and_get_ddl_for_sqlite_trigger() {
    let dir = std::env::temp_dir().join(format!(
        "datazen-sqlite-schema-obj-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("schema_objects.db");
    let path_str = path.to_string_lossy().to_string();
    std::fs::File::create(&path).unwrap();

    let driver = SqliteDriver::new();
    let handle = driver.connect(&test_config(&path_str)).await.unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE t (id INTEGER PRIMARY KEY);
             CREATE TRIGGER trg AFTER INSERT ON t BEGIN SELECT 1; END;",
        )
        .await
        .unwrap();

    let list = execute_schema_object_command(
        &driver,
        "sqlite",
        &handle,
        "list_objects",
        json!({ "kind": ObjectKind::Trigger.as_str() }),
    )
    .await
    .unwrap();
    let objects = list.data["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 1);
    assert_eq!(objects[0]["name"], "trg");

    let ddl = execute_schema_object_command(
        &driver,
        "sqlite",
        &handle,
        "get_object_ddl",
        json!({ "kind": "trigger", "name": "trg" }),
    )
    .await
    .unwrap();
    assert!(ddl.data["ddl"]
        .as_str()
        .unwrap()
        .to_ascii_uppercase()
        .contains("CREATE TRIGGER"));

    let empty = execute_schema_object_command(
        &driver,
        "sqlite",
        &handle,
        "list_objects",
        json!({ "kind": "function" }),
    )
    .await
    .unwrap();
    assert!(empty.data["objects"].as_array().unwrap().is_empty());

    let unsupported_sequence = execute_schema_object_command(
        &driver,
        "sqlite",
        &handle,
        "list_objects",
        json!({ "kind": "sequence" }),
    )
    .await
    .unwrap();
    assert!(unsupported_sequence.data["objects"]
        .as_array()
        .unwrap()
        .is_empty());

    let missing = execute_schema_object_command(
        &driver,
        "sqlite",
        &handle,
        "get_object_ddl",
        json!({ "kind": "trigger", "name": "missing_trigger" }),
    )
    .await
    .unwrap_err();
    assert!(missing.to_string().contains("not found"));

    let privs =
        execute_schema_object_command(&driver, "sqlite", &handle, "list_privileges", json!({}))
            .await
            .unwrap();
    assert!(privs.data["grants"].as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_tester_trigger_identity_journey_preserves_targets_and_escapes_names() {
    let dir = std::env::temp_dir().join(format!(
        "datazen-sqlite-trigger-identity-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("trigger_identity.db");
    let path_str = path.to_string_lossy().to_string();
    std::fs::File::create(&path).unwrap();

    let driver = SqliteDriver::new();
    let handle = driver.connect(&test_config(&path_str)).await.unwrap();
    driver
        .execute(
            &handle,
            r#"
            CREATE TABLE orders (id INTEGER PRIMARY KEY);
            CREATE TABLE audit_log (id INTEGER PRIMARY KEY);
            CREATE TRIGGER "orders_audit'" AFTER INSERT ON orders
            BEGIN SELECT 1; END;
            CREATE TRIGGER audit_log_trigger AFTER INSERT ON audit_log
            BEGIN SELECT 1; END;
            "#,
        )
        .await
        .unwrap();

    let list = execute_schema_object_command(
        &driver,
        "sqlite",
        &handle,
        "list_objects",
        json!({ "kind": "trigger" }),
    )
    .await
    .unwrap();
    let objects = list.data["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 2);
    let order_trigger = objects
        .iter()
        .find(|object| object["name"] == "orders_audit'")
        .expect("quoted trigger should be listed");
    assert_eq!(order_trigger["targetName"], "orders");
    assert!(order_trigger["targetSchema"].is_null());
    let audit_trigger = objects
        .iter()
        .find(|object| object["name"] == "audit_log_trigger")
        .expect("second trigger should be listed");
    assert_eq!(audit_trigger["targetName"], "audit_log");

    let ddl = execute_schema_object_command(
        &driver,
        "sqlite",
        &handle,
        "get_object_ddl",
        json!({
            "kind": "trigger",
            "name": "orders_audit'",
            "targetSchema": null,
            "targetName": "orders"
        }),
    )
    .await
    .unwrap();
    let ddl = ddl.data["ddl"].as_str().unwrap();
    assert!(ddl.to_ascii_uppercase().contains("CREATE TRIGGER"));
    assert!(ddl.contains("orders_audit'"));
    assert!(ddl.contains("orders"));

    let unsupported = execute_schema_object_command(
        &driver,
        "sqlite",
        &handle,
        "get_object_ddl",
        json!({ "kind": "function", "name": "not_supported" }),
    )
    .await
    .unwrap_err();
    assert!(unsupported
        .to_string()
        .contains("does not expose object DDL"));

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_tester_view_ddl_preserves_query_body_when_as_is_multiline() {
    let dir = std::env::temp_dir().join(format!(
        "datazen-sqlite-schema-view-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("schema_views.db");
    let path_str = path.to_string_lossy().to_string();
    std::fs::File::create(&path).unwrap();

    let driver = SqliteDriver::new();
    let handle = driver.connect(&test_config(&path_str)).await.unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE source_rows (id INTEGER PRIMARY KEY);
             CREATE VIEW active_rows AS\nSELECT id\nFROM source_rows;
             CREATE VIEW compact_lower as select id from source_rows;
             CREATE VIEW tabbed_rows\nAS\tSELECT id\nFROM source_rows",
        )
        .await
        .unwrap();

    let list = execute_schema_object_command(
        &driver,
        "sqlite",
        &handle,
        "list_objects",
        json!({ "kind": "view" }),
    )
    .await
    .unwrap();
    let objects = list.data["objects"].as_array().unwrap();
    assert_eq!(objects.len(), 3);
    for (name, expected) in [
        ("active_rows", "SELECT id\nFROM source_rows"),
        ("compact_lower", "select id from source_rows"),
        ("tabbed_rows", "SELECT id\nFROM source_rows"),
    ] {
        let ddl = execute_schema_object_command(
            &driver,
            "sqlite",
            &handle,
            "get_object_ddl",
            json!({ "kind": "view", "name": name }),
        )
        .await
        .unwrap();
        assert_eq!(ddl.data["ddl"], expected, "unexpected DDL for {name}");
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_tester_round2_view_ddl_preserves_single_line_and_spacing_variants() {
    let dir = std::env::temp_dir().join(format!(
        "datazen-sqlite-schema-view-round2-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("schema_views_round2.db");
    let path_str = path.to_string_lossy().to_string();
    std::fs::File::create(&path).unwrap();

    let driver = SqliteDriver::new();
    let handle = driver.connect(&test_config(&path_str)).await.unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE source_rows (id INTEGER PRIMARY KEY);
             CREATE VIEW upper_single_line AS SELECT id FROM source_rows;
             CREATE VIEW extra_spacing    AS    SELECT id FROM source_rows;
             CREATE VIEW crlf_view AS\r\nSELECT id\r\nFROM source_rows",
        )
        .await
        .unwrap();

    for (name, expected) in [
        ("upper_single_line", "SELECT id FROM source_rows"),
        ("extra_spacing", "SELECT id FROM source_rows"),
        ("crlf_view", "SELECT id\r\nFROM source_rows"),
    ] {
        let ddl = execute_schema_object_command(
            &driver,
            "sqlite",
            &handle,
            "get_object_ddl",
            json!({ "kind": "view", "name": name }),
        )
        .await
        .unwrap();
        assert_eq!(ddl.data["ddl"], expected, "unexpected DDL for {name}");
        assert!(!ddl.data["ddl"].as_str().unwrap().contains("ATE VIEW"));
    }

    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn test_tester_round3_view_ddl_strips_mixed_separator_whitespace() {
    let dir = std::env::temp_dir().join(format!(
        "datazen-sqlite-schema-view-round3-{}",
        uuid::Uuid::new_v4()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("schema_views_round3.db");
    let path_str = path.to_string_lossy().to_string();
    std::fs::File::create(&path).unwrap();

    let driver = SqliteDriver::new();
    let handle = driver.connect(&test_config(&path_str)).await.unwrap();
    driver
        .execute(
            &handle,
            "CREATE TABLE source_rows (id INTEGER PRIMARY KEY);
             CREATE VIEW mixed_separator AS \r\n\tSELECT id\r\nFROM source_rows",
        )
        .await
        .unwrap();

    let ddl = execute_schema_object_command(
        &driver,
        "sqlite",
        &handle,
        "get_object_ddl",
        json!({ "kind": "view", "name": "mixed_separator" }),
    )
    .await
    .unwrap();
    assert_eq!(ddl.data["ddl"], "SELECT id\r\nFROM source_rows");
    assert!(!matches!(
        ddl.data["ddl"].as_str().unwrap().chars().next(),
        Some(' ' | '\t' | '\r' | '\n')
    ));
    assert!(!ddl.data["ddl"].as_str().unwrap().contains("ATE VIEW"));

    let _ = std::fs::remove_dir_all(&dir);
}
