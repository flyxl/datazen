//! ClickHouse HTTP query smoke via wiremock (no live ClickHouse server).

use datazen_driver_api::{ConnectionConfig, DatabaseDriver, SslMode, Value};
use datazen_driver_clickhouse::ClickHouseDriver;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn http_config(server: &MockServer) -> ConnectionConfig {
    let addr = server.address();
    ConnectionConfig {
        id: "ch-it".into(),
        name: "clickhouse wiremock".into(),
        database_type: "clickhouse".into(),
        host: Some(addr.ip().to_string()),
        port: Some(addr.port()),
        database: None,
        schema: None,
        username: None,
        password: None,
        ssl_mode: SslMode::Disable,
        connection_timeout: 5,
        max_pool_size: 4,
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

#[tokio::test]
async fn query_returns_rows_from_http_json() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "meta": [{"name": "n", "type": "UInt8"}],
            "data": [{"n": 42}],
            "rows": 1
        })))
        .mount(&server)
        .await;

    let driver = ClickHouseDriver::new();
    let handle = driver.connect(&http_config(&server)).await.unwrap();
    let result = driver.query(&handle, "SELECT 42 AS n").await.unwrap();
    assert_eq!(result.rows.len(), 1);
    assert!(matches!(result.rows[0][0], Some(Value::Integer(42))));
    driver.disconnect(handle).await.unwrap();
}
