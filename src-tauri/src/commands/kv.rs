use super::error::{CmdExt, CommandError};
use super::AppState;
use std::time::Instant;
use tauri::State;

pub(crate) fn kv_scan_keys_response(
    next_cursor: u64,
    keys: &[crate::db::KeyEntry],
    db_size: u64,
) -> serde_json::Value {
    serde_json::json!({
        "cursor": next_cursor,
        "keys": keys,
        "dbSize": db_size,
    })
}

pub(crate) async fn kv_scan_keys_impl(
    state: &AppState,
    connection_id: String,
    db_index: u32,
    pattern: String,
    cursor: u64,
    count: u32,
) -> Result<serde_json::Value, CommandError> {
    let start = Instant::now();
    tracing::info!(%connection_id, db_index, %pattern, cursor, count, "kv_scan_keys");
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .cmd_err("kv_scan_keys")?;
    let db_type = config.database_type;
    let kv = state
        .driver_registry
        .get_kv_driver(&db_type)
        .await
        .ok_or_else(|| {
            CommandError::NotFound("Key-value operations not supported for this connection".into())
        })?;
    let (_driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("kv_scan_keys")?;
    let (next_cursor, keys, db_size) = kv
        .scan_keys_with_info(&handle, db_index, &pattern, cursor, count)
        .await
        .cmd_err("kv_scan_keys")?;
    tracing::info!(%connection_id, db_index, next_cursor, key_count = keys.len(), db_size, ms = start.elapsed().as_millis() as u64, "kv_scan_keys OK");
    Ok(kv_scan_keys_response(next_cursor, &keys, db_size))
}

pub(crate) async fn kv_get_key_impl(
    state: &AppState,
    connection_id: String,
    db_index: u32,
    key: String,
) -> Result<serde_json::Value, CommandError> {
    let start = Instant::now();
    tracing::info!(%connection_id, db_index, %key, "kv_get_key");
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .cmd_err("kv_get_key")?;
    let db_type = config.database_type;
    let kv = state
        .driver_registry
        .get_kv_driver(&db_type)
        .await
        .ok_or_else(|| {
            CommandError::NotFound("Key-value operations not supported for this connection".into())
        })?;
    let (_driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("kv_get_key")?;
    let detail = kv
        .get_key_detail(&handle, db_index, &key)
        .await
        .cmd_err("kv_get_key")?;
    tracing::info!(%connection_id, db_index, %key, ms = start.elapsed().as_millis() as u64, "kv_get_key OK");
    serde_json::to_value(detail).cmd_err("kv_get_key")
}

/// Scan Redis keys with their types, TTL, and value preview (paginated via SCAN cursor).
#[tauri::command]
pub async fn kv_scan_keys(
    state: State<'_, AppState>,
    connection_id: String,
    db_index: u32,
    pattern: String,
    cursor: u64,
    count: u32,
) -> Result<serde_json::Value, CommandError> {
    kv_scan_keys_impl(&state, connection_id, db_index, pattern, cursor, count).await
}

/// Return the full JSON value for a Redis key in the given logical database.
#[tauri::command]
pub async fn kv_get_key(
    state: State<'_, AppState>,
    connection_id: String,
    db_index: u32,
    key: String,
) -> Result<serde_json::Value, CommandError> {
    kv_get_key_impl(&state, connection_id, db_index, key).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{ConnectionConfig, SslMode};
    use crate::testing::app_state::TestAppState;
    use crate::testing::mock_kv_driver::MockKvDriverOptions;

    fn redis_config(id: &str) -> ConnectionConfig {
        ConnectionConfig {
            id: id.into(),
            name: id.into(),
            database_type: "redis".into(),
            host: Some("127.0.0.1".into()),
            port: Some(6379),
            database: None,
            schema: None,
            username: None,
            password: None,
            ssl_mode: SslMode::default(),
            connection_timeout: 30,
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

    #[test]
    fn kv_scan_keys_response_shape() {
        use crate::db::KeyEntry;

        let keys = [KeyEntry {
            key: "foo".into(),
            key_type: "string".into(),
            ttl: 60,
            size: 3,
            preview: "\"bar\"".into(),
        }];
        let json = kv_scan_keys_response(42, &keys, 100);
        assert_eq!(json["cursor"], 42);
        assert_eq!(json["dbSize"], 100);
        assert_eq!(json["keys"][0]["key"], "foo");
    }

    #[tokio::test]
    async fn kv_scan_keys_impl_uses_mock_driver() {
        let test = TestAppState::new().await;
        test.register_redis_kv(MockKvDriverOptions::default()).await;
        let config = redis_config("redis-kv");
        test.store.save_connection(config).await.unwrap();
        let conn_id = test.connect_config("redis-kv").await;

        let result = kv_scan_keys_impl(&test.state, conn_id, 0, "*".into(), 0, 10)
            .await
            .unwrap();
        assert_eq!(result["keys"][0]["key"], "user:1");
        assert_eq!(result["dbSize"], 1);
    }

    #[tokio::test]
    async fn kv_get_key_impl_returns_detail_json() {
        let test = TestAppState::new().await;
        test.register_redis_kv(MockKvDriverOptions::default()).await;
        let config = redis_config("redis-get");
        test.store.save_connection(config).await.unwrap();
        let conn_id = test.connect_config("redis-get").await;

        let result = kv_get_key_impl(&test.state, conn_id, 0, "user:1".into())
            .await
            .unwrap();
        assert_eq!(result["key"], "user:1");
        assert_eq!(result["value"], "alice");
    }

    #[tokio::test]
    async fn kv_scan_keys_missing_driver_returns_not_found() {
        let test = TestAppState::new().await;
        let config = test.save_connection("pg-no-kv").await;
        let conn_id = test.connect_config("pg-no-kv").await;

        let err = kv_scan_keys_impl(&test.state, conn_id, 0, "*".into(), 0, 10)
            .await
            .unwrap_err();
        assert!(err.to_string().contains("not supported"));
        let _ = config;
    }
}
