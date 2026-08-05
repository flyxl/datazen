use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{ConnectionConfig, DatabaseType, ServerInfo};
use tauri::State;

#[tauri::command]
pub async fn get_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionConfig>, CommandError> {
    let list = state.store.get_connections().await;
    tracing::debug!(count = list.len(), "get_connections");
    Ok(list)
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<(), CommandError> {
    tracing::info!(id = %config.id, name = %config.name, "save_connection");
    state
        .store
        .save_connection(config)
        .await
        .cmd_err("save_connection")
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    tracing::info!(%id, "delete_connection");
    state
        .store
        .delete_connection(&id)
        .await
        .cmd_err("delete_connection")
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<ServerInfo, CommandError> {
    tracing::info!(
        name = %config.name,
        host = ?config.host,
        port = ?config.port,
        db_type = ?config.database_type,
        ssh = ?config.ssh_tunnel.as_ref().map(|s| format!("{}@{}:{}", s.username, s.host, s.port)),
        "test_connection"
    );
    let result = state
        .connection_manager
        .test_connection(&config)
        .await
        .cmd_err("test_connection")?;
    tracing::info!(version = %result.server_version, "test_connection OK");
    Ok(result)
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>, config_id: String) -> Result<String, CommandError> {
    tracing::info!(%config_id, "connect");
    let conn_id = state
        .connection_manager
        .get_or_connect(&config_id)
        .await
        .cmd_err("connect")?;

    if let Some(mut cfg) = state.store.get_connection(&config_id).await {
        cfg.last_connected_at = Some(chrono::Utc::now().to_rfc3339());
        let _ = state.store.save_connection(cfg).await;
    }

    tracing::info!(%config_id, %conn_id, "connect OK");
    Ok(conn_id)
}

#[tauri::command]
pub async fn ping_connection(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<bool, CommandError> {
    let alive = state.connection_manager.ping(&connection_id).await;
    Ok(alive)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, connection_id: String) -> Result<(), CommandError> {
    tracing::info!(%connection_id, "disconnect");
    state
        .connection_manager
        .disconnect(&connection_id)
        .await
        .cmd_err("disconnect")?;
    state.schema_cache.clear_connection(&connection_id).await;
    tracing::info!(%connection_id, "disconnect OK");
    Ok(())
}

#[tauri::command]
pub async fn get_connection_info(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<serde_json::Value, CommandError> {
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .cmd_err("get_connection_info")?;

    let db_type = match config.database_type {
        DatabaseType::PostgreSQL => "postgresql",
        DatabaseType::MySQL => "mysql",
        DatabaseType::MariaDB => "mariadb",
        DatabaseType::SQLite => "sqlite",
        DatabaseType::Redis => "redis",
        DatabaseType::Kiwi => "kiwi",
        DatabaseType::Presto => "presto",
        DatabaseType::Trino => "trino",
    };

    let driver_category = match config.database_type {
        DatabaseType::Redis => "keyvalue",
        _ => "sql",
    };

    Ok(serde_json::json!({
        "databaseType": db_type,
        "driverCategory": driver_category,
        "name": config.name,
        "host": config.host,
        "port": config.port,
        "database": config.database,
        "schema": config.schema,
        "serverVersion": config.server_version,
    }))
}

#[tauri::command]
pub async fn get_available_drivers(state: State<'_, AppState>) -> Result<Vec<String>, CommandError> {
    let types = state.driver_registry.supported_types().await;
    let names: Vec<String> = types
        .into_iter()
        .map(|t| serde_json::to_value(&t).unwrap_or_default())
        .filter_map(|v| v.as_str().map(|s| s.to_string()))
        .collect();
    Ok(names)
}
