use super::error::{CmdExt, CommandError};
use super::AppState;
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
        .ok_or_else(|| CommandError::NotFound("Key-value operations not supported for this connection".into()))?;
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
        .ok_or_else(|| CommandError::NotFound("Key-value operations not supported for this connection".into()))?;
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
