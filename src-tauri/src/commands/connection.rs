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

pub(crate) async fn delete_connection_impl(
    state: &AppState,
    id: String,
) -> Result<(), CommandError> {
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

pub(crate) async fn connect_impl(
    state: &AppState,
    connection_id: String,
) -> Result<String, CommandError> {
    tracing::info!(%connection_id, "connect");
    let db_session_id = state
        .connection_manager
        .get_or_connect_session(&connection_id)
        .await
        .cmd_err("connect")?;

    if let Some(mut cfg) = state.store.get_connection(&connection_id).await {
        cfg.last_connected_at = Some(chrono::Utc::now().to_rfc3339());
        let _ = state.store.save_connection(cfg).await;
    }

    tracing::info!(db_session_id = %db_session_id, "connect OK");
    Ok(db_session_id)
}

pub(crate) async fn connect_dedicated_impl(
    state: &AppState,
    connection_id: String,
    database: Option<String>,
) -> Result<String, CommandError> {
    tracing::info!(%connection_id, database = ?database, "connect_dedicated");
    let db = database.as_deref();
    let db_session_id = state
        .connection_manager
        .connect_dedicated(&connection_id, db)
        .await
        .cmd_err("connect_dedicated")?;

    if let Some(mut cfg) = state.store.get_connection(&connection_id).await {
        cfg.last_connected_at = Some(chrono::Utc::now().to_rfc3339());
        let _ = state.store.save_connection(cfg).await;
    }

    tracing::info!(db_session_id = %db_session_id, "connect_dedicated OK");
    Ok(db_session_id)
}

pub(crate) async fn ping_connection_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<bool, CommandError> {
    let alive = state.connection_manager.ping(&db_session_id).await;
    Ok(alive)
}

pub(crate) async fn release_connection_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<bool, CommandError> {
    tracing::info!(%db_session_id, "release_connection");
    let disconnected = state
        .connection_manager
        .release(&db_session_id)
        .await
        .cmd_err("release_connection")?;
    if disconnected {
        state.schema_cache.clear_connection(&db_session_id).await;
        tracing::info!(%db_session_id, "release_connection: session torn down (ref=0)");
    } else {
        tracing::info!(%db_session_id, "release_connection: ref decremented, session kept alive");
    }
    Ok(disconnected)
}

pub(crate) async fn disconnect_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<(), CommandError> {
    tracing::info!(%db_session_id, "disconnect (force)");
    if let Some(tx) = state
        .session_transactions
        .lock()
        .await
        .remove(&db_session_id)
    {
        if let Ok((driver, _)) = state.connection_manager.get_session(&db_session_id).await {
            if let Err(e) = driver.rollback(tx).await {
                tracing::warn!(%db_session_id, error = %e, "rollback session tx on disconnect");
            }
        }
    }
    state
        .connection_manager
        .disconnect(&db_session_id)
        .await
        .cmd_err("disconnect")?;
    state.schema_cache.clear_connection(&db_session_id).await;
    tracing::info!(%db_session_id, "disconnect OK");
    Ok(())
}

pub(crate) async fn get_connection_info_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<serde_json::Value, CommandError> {
    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .cmd_err("get_connection_info")?;

    let db_type = &config.database_type;

    let driver = state.driver_registry.get(db_type).await;
    let driver_category = match driver.as_ref().map(|d| d.driver_category()) {
        Some(DriverCategory::KeyValue) => "keyvalue",
        Some(DriverCategory::Document) => "document",
        _ => "sql",
    };
    let capabilities = state.driver_registry.get_capabilities(db_type).await;

    Ok(serde_json::json!({
        "databaseType": db_type,
        "driverCategory": driver_category,
        "name": config.name,
        "host": config.host,
        "port": config.port,
        "database": config.database,
        "schema": config.schema,
        "serverVersion": config.server_version,
        "capabilities": capabilities,
    }))
}

pub(crate) async fn get_available_drivers_impl(
    state: &AppState,
) -> Result<Vec<String>, CommandError> {
    Ok(state.driver_registry.available_types())
}

#[tauri::command]
pub async fn get_connections(
    state: State<'_, AppState>,
) -> Result<Vec<ConnectionConfig>, CommandError> {
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
pub async fn connect(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<String, CommandError> {
    // connection_id = 持久化配置连接 id；返回值为运行时 db_session_id。
    connect_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn connect_dedicated(
    state: State<'_, AppState>,
    connection_id: String,
    database: Option<String>,
) -> Result<String, CommandError> {
    connect_dedicated_impl(&state, connection_id, database).await
}

#[tauri::command]
pub async fn ping_connection(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<bool, CommandError> {
    ping_connection_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn release_connection(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<bool, CommandError> {
    release_connection_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn disconnect(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<(), CommandError> {
    disconnect_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn get_connection_info(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<serde_json::Value, CommandError> {
    get_connection_info_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn reorder_connections(
    state: State<'_, AppState>,
    ordered_ids: Vec<String>,
) -> Result<(), CommandError> {
    tracing::info!(count = ordered_ids.len(), "reorder_connections");
    state
        .store
        .reorder_connections(ordered_ids)
        .await
        .cmd_err("reorder_connections")
}

#[tauri::command]
pub async fn get_available_drivers(
    state: State<'_, AppState>,
) -> Result<Vec<String>, CommandError> {
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
        let info = get_connection_info_impl(&test.state, conn_id)
            .await
            .unwrap();
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

#[cfg(test)]
mod coverage_tests {
    //! BUG-002 anchors: exercises the remaining `connection.rs` branches —
    //! release ref-counting, disconnect's pending-transaction rollback,
    //! driver-category reporting, and the reorder wrapper.

    use super::*;
    use crate::db::TransactionHandle;
    use crate::testing::app_state::{sample_postgres_config, TestAppState};
    use crate::testing::mock_driver::{MockDriver, MockDriverOptions};

    #[tokio::test]
    async fn connect_dedicated_opens_separate_session_from_reuse() {
        let test = TestAppState::with_tables().await;
        test.save_connection("d1").await;
        let shared = connect_impl(&test.state, "d1".into()).await.unwrap();
        let dedicated = connect_dedicated_impl(&test.state, "d1".into(), Some("postgres".into()))
            .await
            .unwrap();
        assert_ne!(shared, dedicated);
        assert!(ping_connection_impl(&test.state, shared.clone())
            .await
            .unwrap());
        assert!(ping_connection_impl(&test.state, dedicated.clone())
            .await
            .unwrap());
        let torn = release_connection_impl(&test.state, dedicated)
            .await
            .unwrap();
        assert!(torn);
        assert!(ping_connection_impl(&test.state, shared).await.unwrap());
    }

    #[tokio::test]
    async fn release_connection_decrements_then_tears_down() {
        let test = TestAppState::with_tables().await;
        test.save_connection("r1").await;
        // Two borrowers hold the same session (refs = 2).
        let s1 = connect_impl(&test.state, "r1".into()).await.unwrap();
        let s2 = connect_impl(&test.state, "r1".into()).await.unwrap();
        assert_eq!(s1, s2, "reuse must hand back the same db session id");

        // First release only decrements: session stays alive.
        let torn = release_connection_impl(&test.state, s1.clone())
            .await
            .unwrap();
        assert!(!torn);
        assert!(ping_connection_impl(&test.state, s1.clone()).await.unwrap());

        // Second release hits zero and tears the session down.
        let torn = release_connection_impl(&test.state, s2.clone())
            .await
            .unwrap();
        assert!(torn);
        assert!(
            !ping_connection_impl(&test.state, s1).await.unwrap(),
            "session must be gone after the final release"
        );

        // Releasing an unknown session is a benign no-op (reports torn down).
        let torn = release_connection_impl(&test.state, "missing".into())
            .await
            .unwrap();
        assert!(torn);
    }

    #[tokio::test]
    async fn disconnect_rolls_back_open_session_transaction_and_clears_map() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("tx-1").await;

        // Begin a real transaction through the mock driver so rollback succeeds.
        let (driver, handle) = test
            .state
            .connection_manager
            .get_session(&conn_id)
            .await
            .unwrap();
        let tx = driver.begin_transaction(&handle).await.unwrap();
        test.state
            .session_transactions
            .lock()
            .await
            .insert(conn_id.clone(), tx);

        disconnect_impl(&test.state, conn_id.clone()).await.unwrap();
        assert!(
            !test
                .state
                .session_transactions
                .lock()
                .await
                .contains_key(&conn_id),
            "disconnect must consume the pending session transaction entry"
        );
        assert!(
            !ping_connection_impl(&test.state, conn_id.clone())
                .await
                .unwrap(),
            "disconnect must tear down the session"
        );

        // Rollback-failure branch: an entry whose handle is unknown to the
        // driver logs a warning but disconnect still succeeds.
        test.save_and_connect("tx-2").await;
        test.state.session_transactions.lock().await.insert(
            format!("{}:bogus", "tx-2"),
            TransactionHandle {
                id: "never-begun".into(),
                connection_id: "never-begun-handle".into(),
            },
        );
        disconnect_impl(&test.state, format!("{}:bogus", "tx-2"))
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn get_connection_info_reports_driver_category() {
        let harness = TestAppState::new().await;
        for (driver_type, expected_category, category) in [
            ("kv-mock", "keyvalue", crate::db::DriverCategory::KeyValue),
            ("doc-mock", "document", crate::db::DriverCategory::Document),
            ("postgres", "sql", crate::db::DriverCategory::Sql),
        ] {
            let opts = MockDriverOptions {
                category,
                ..Default::default()
            };
            let mock = MockDriver::new(driver_type, opts);
            harness
                .state
                .driver_registry
                .register_test_driver(driver_type, mock)
                .await;
            let mut cfg = sample_postgres_config(driver_type);
            cfg.database_type = driver_type.into();
            save_connection_impl(&harness.state, cfg).await.unwrap();

            let conn_id = connect_impl(&harness.state, driver_type.into())
                .await
                .unwrap();
            let info = get_connection_info_impl(&harness.state, conn_id)
                .await
                .unwrap();
            assert_eq!(info["databaseType"], driver_type);
            assert_eq!(
                info["driverCategory"], expected_category,
                "driver {driver_type} must report category {expected_category}"
            );
        }
    }

    #[tokio::test]
    async fn reorder_connections_persists_new_order() {
        // `reorder_connections` is a thin IPC wrapper over the store call;
        // tauri::State cannot be built outside the tauri runtime, so exercise
        // the store path the wrapper delegates to.
        let test = TestAppState::new().await;
        for id in ["a", "b", "c"] {
            save_connection_impl(&test.state, sample_postgres_config(id))
                .await
                .unwrap();
        }
        test.state
            .store
            .reorder_connections(vec!["c".into(), "a".into(), "b".into()])
            .await
            .unwrap();
        let order: Vec<String> = get_connections_impl(&test.state)
            .await
            .unwrap()
            .into_iter()
            .map(|c| c.id)
            .collect();
        assert_eq!(order, vec!["c", "a", "b"]);
    }
}
