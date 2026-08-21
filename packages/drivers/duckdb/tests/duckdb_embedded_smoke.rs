//! Embedded DuckDB smoke (`:memory:` — no remote server).

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, Value};

use datazen_driver_duckdb::DuckDbDriver;

fn memory_config() -> ConnectionConfig {
    ConnectionConfig {
        id: "duckdb-it".into(),
        name: "duckdb integration".into(),
        database_type: "duckdb".into(),
        host: None,
        port: None,
        database: Some(":memory:".into()),
        schema: None,
        username: None,
        password: None,
        ssl_mode: Default::default(),
        connection_timeout: 5,
        max_pool_size: 10,
        ssh_tunnel: None,
        color_tag: None,
        group: None,
        last_connected_at: None,
        server_version: None,
        options: None,
        read_only: false,
    }
}

#[tokio::test]
async fn in_memory_query_and_schema_smoke() {
    let driver = DuckDbDriver::new();
    let handle = driver.connect(&memory_config()).await.unwrap();

    driver
        .execute(
            &handle,
            "CREATE TABLE t (id INTEGER PRIMARY KEY, label VARCHAR)",
        )
        .await
        .unwrap();
    driver
        .execute(&handle, "INSERT INTO t VALUES (1, 'one')")
        .await
        .unwrap();

    let result = driver
        .query(&handle, "SELECT id, label FROM t ORDER BY id")
        .await
        .unwrap();
    assert_eq!(result.rows.len(), 1);
    assert!(matches!(result.rows[0][0], Some(Value::Integer(1))));

    let schema = driver.get_table_schema(&handle, "t").await.unwrap();
    assert_eq!(schema.primary_keys, vec!["id".to_string()]);

    driver.disconnect(handle).await.unwrap();
}
