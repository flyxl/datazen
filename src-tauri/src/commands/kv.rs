use super::{AppState, log_err};
use std::time::Instant;
use tauri::State;

/// Scan Redis keys with their types, TTL, and value preview (paginated via SCAN cursor).
#[tauri::command]
pub async fn kv_scan_keys(
    state: State<'_, AppState>,
    connection_id: String,
    db_index: u32,
    pattern: String,
    cursor: u64,
    count: u32,
) -> Result<serde_json::Value, String> {
    let start = Instant::now();
    tracing::info!(%connection_id, db_index, %pattern, cursor, count, "kv_scan_keys");
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .map_err(|e| log_err("kv_scan_keys", &e))?;
    let db_type = config.database_type;
    let kv = state
        .driver_registry
        .get_kv_driver(&db_type)
        .await
        .ok_or_else(|| "Key-value operations not supported for this connection".to_string())?;
    let (_driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("kv_scan_keys", &e))?;
    let (next_cursor, keys, db_size) = kv
        .scan_keys_with_info(&handle, db_index, &pattern, cursor, count)
        .await
        .map_err(|e| log_err("kv_scan_keys", &e))?;
    tracing::info!(%connection_id, db_index, next_cursor, key_count = keys.len(), db_size, ms = start.elapsed().as_millis() as u64, "kv_scan_keys OK");
    Ok(serde_json::json!({
        "cursor": next_cursor,
        "keys": keys,
        "dbSize": db_size,
    }))
}

/// Return the full JSON value for a Redis key in the given logical database.
#[tauri::command]
pub async fn kv_get_key(
    state: State<'_, AppState>,
    connection_id: String,
    db_index: u32,
    key: String,
) -> Result<serde_json::Value, String> {
    let start = Instant::now();
    tracing::info!(%connection_id, db_index, %key, "kv_get_key");
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .map_err(|e| log_err("kv_get_key", &e))?;
    let db_type = config.database_type;
    let kv = state
        .driver_registry
        .get_kv_driver(&db_type)
        .await
        .ok_or_else(|| "Key-value operations not supported for this connection".to_string())?;
    let (_driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("kv_get_key", &e))?;
    let detail = kv
        .get_key_detail(&handle, db_index, &key)
        .await
        .map_err(|e| log_err("kv_get_key", &e))?;
    tracing::info!(%connection_id, db_index, %key, ms = start.elapsed().as_millis() as u64, "kv_get_key OK");
    serde_json::to_value(detail).map_err(|e| log_err("kv_get_key", &e))
}
