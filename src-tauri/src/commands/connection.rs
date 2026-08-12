use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{ConnectionConfig, DriverCategory, ServerInfo};
use tauri::State;

pub(crate) async fn get_connections_impl(
    state: &AppState,
) -> Result<Vec<ConnectionConfig>, CommandError> {
    let list = state.store.get_connections().await;
    tracing::debug!(count = list.len(), "get_connections");
    Ok(list)
}

pub(crate) async fn save_connection_impl(
    state: &AppState,
    config: ConnectionConfig,
) -> Result<(), CommandError> {
    tracing::info!(id = %config.id, name = %config.name, "save_connection");
    state
        .store
        .save_connection(config)
        .await
        .cmd_err("save_connection")
}

pub(crate) async fn delete_connection_impl(state: &AppState, id: String) -> Result<(), CommandError> {
    tracing::info!(%id, "delete_connection");
    state
        .store
        .delete_connection(&id)
        .await
        .cmd_err("delete_connection")
}

pub(crate) async fn test_connection_impl(
    state: &AppState,
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

pub(crate) async fn connect_impl(state: &AppState, config_id: String) -> Result<String, CommandError> {
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

pub(crate) async fn ping_connection_impl(
    state: &AppState,
    connection_id: String,
) -> Result<bool, CommandError> {
    let alive = state.connection_manager.ping(&connection_id).await;
    Ok(alive)
}

pub(crate) async fn disconnect_impl(
    state: &AppState,
    connection_id: String,
) -> Result<(), CommandError> {
    tracing::info!(%connection_id, "disconnect");
    if let Some(tx) = state.session_transactions.lock().await.remove(&connection_id) {
        if let Ok((driver, _)) = state.connection_manager.get_connection(&connection_id).await {
            if let Err(e) = driver.rollback(tx).await {
                tracing::warn!(%connection_id, error = %e, "rollback session tx on disconnect");
            }
        }
    }
    state
        .connection_manager
        .disconnect(&connection_id)
        .await
        .cmd_err("disconnect")?;
    state.schema_cache.clear_connection(&connection_id).await;
    tracing::info!(%connection_id, "disconnect OK");
    Ok(())
}

pub(crate) async fn get_connection_info_impl(
    state: &AppState,
    connection_id: String,
) -> Result<serde_json::Value, CommandError> {
    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .cmd_err("get_connection_info")?;

    let db_type = &config.database_type;

    let driver = state.driver_registry.get(db_type).await;
    let driver_category = match driver.as_ref().map(|d| d.driver_category()) {
        Some(DriverCategory::KeyValue) => "keyvalue",
        Some(DriverCategory::Document) => "document",
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

pub(crate) async fn get_available_drivers_impl(
    state: &AppState,
) -> Result<Vec<String>, CommandError> {
    Ok(state.driver_registry.available_types())
}

#[tauri::command]
pub async fn get_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionConfig>, CommandError> {
    get_connections_impl(&state).await
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<(), CommandError> {
    save_connection_impl(&state, config).await
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), CommandError> {
    delete_connection_impl(&state, id).await
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<ServerInfo, CommandError> {
    test_connection_impl(&state, config).await
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>, config_id: String) -> Result<String, CommandError> {
    connect_impl(&state, config_id).await
}

#[tauri::command]
pub async fn ping_connection(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<bool, CommandError> {
    ping_connection_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, connection_id: String) -> Result<(), CommandError> {
    disconnect_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn get_connection_info(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<serde_json::Value, CommandError> {
    get_connection_info_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn get_available_drivers(state: State<'_, AppState>) -> Result<Vec<String>, CommandError> {
    get_available_drivers_impl(&state).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::app_state::{sample_postgres_config, TestAppState};

    #[tokio::test]
    async fn get_connections_empty_then_saved() {
        let test = TestAppState::new().await;
        assert!(get_connections_impl(&test.state).await.unwrap().is_empty());
        test.save_connection("c1").await;
        let list = get_connections_impl(&test.state).await.unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "c1");
    }

    #[tokio::test]
    async fn save_and_delete_connection() {
        let test = TestAppState::new().await;
        let cfg = sample_postgres_config("del-me");
        save_connection_impl(&test.state, cfg).await.unwrap();
        delete_connection_impl(&test.state, "del-me".into())
            .await
            .unwrap();
        assert!(get_connections_impl(&test.state).await.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_connection_success_and_unknown_driver() {
        let test = TestAppState::with_tables().await;
        let info = test_connection_impl(&test.state, sample_postgres_config("t1"))
            .await
            .unwrap();
        assert!(info.server_version.contains("PostgreSQL"));

        let mut bad = sample_postgres_config("bad");
        bad.database_type = "nonexistent_driver".into();
        assert!(test_connection_impl(&test.state, bad).await.is_err());
    }

    #[tokio::test]
    async fn connect_updates_last_connected_and_ping_disconnect() {
        let test = TestAppState::with_tables().await;
        test.save_connection("cfg-1").await;
        let conn_id = connect_impl(&test.state, "cfg-1".into()).await.unwrap();
        let stored = test.store.get_connection("cfg-1").await.unwrap();
        assert!(stored.last_connected_at.is_some());

        assert!(ping_connection_impl(&test.state, conn_id.clone())
            .await
            .unwrap());
        disconnect_impl(&test.state, conn_id).await.unwrap();
        assert!(!ping_connection_impl(&test.state, "missing".into())
            .await
            .unwrap());
    }

    #[tokio::test]
    async fn get_connection_info_and_available_drivers() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("cfg-2").await;
        let info = get_connection_info_impl(&test.state, conn_id).await.unwrap();
        assert_eq!(info["databaseType"], "postgres");
        assert_eq!(info["driverCategory"], "sql");

        let drivers = get_available_drivers_impl(&test.state).await.unwrap();
        // Test drivers are registered via register_test_driver, not inventory catalog.
        let _ = drivers;
        assert!(test
            .state
            .driver_registry
            .get(&"postgres".to_string())
            .await
            .is_some());
    }

    #[tokio::test]
    async fn connect_unknown_config_errors() {
        let test = TestAppState::new().await;
        assert!(connect_impl(&test.state, "missing".into()).await.is_err());
    }

    #[tokio::test]
    async fn get_connection_info_unknown_connection_errors() {
        let test = TestAppState::new().await;
        assert!(get_connection_info_impl(&test.state, "nope".into())
            .await
            .is_err());
    }

    #[test]
    fn sample_config_has_expected_database_type() {
        let cfg = sample_postgres_config("x");
        assert_eq!(cfg.database_type, "postgres");
    }
}
