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
    assert!(ids.contains(&"list_privileges"));
    assert_eq!(schema_object_command_definitions().len(), 3);
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

    let privs =
        execute_schema_object_command(&driver, "sqlite", &handle, "list_privileges", json!({}))
            .await
            .unwrap();
    assert!(privs.data["grants"].as_array().unwrap().is_empty());

    let _ = std::fs::remove_dir_all(&dir);
}
