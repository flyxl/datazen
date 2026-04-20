//! Tauri IPC command surface.

use crate::cache::SchemaCache;
use crate::db::registry::DriverRegistry;
use crate::db::{
    ConnectionConfig, ExplainResult, MultiQueryResult, QueryResult, ServerInfo, TableDataResult,
    TableInfo, TableSchema,
};
use crate::services::{ConnectionManager, FilterCondition, OrderBy, QueryExecutor, SortCondition};
use crate::store::{AppSettings, QueryHistoryEntry, Store};
use std::sync::Arc;
use tauri::State;
use uuid::Uuid;
use std::path::PathBuf;

/// Shared application state injected into every command handler.
pub struct AppState {
    #[allow(dead_code)]
    pub driver_registry: Arc<DriverRegistry>,
    pub connection_manager: Arc<ConnectionManager>,
    pub store: Arc<Store>,
    pub schema_cache: Arc<SchemaCache>,
}

fn log_err(cmd: &str, e: &dyn std::fmt::Display) -> String {
    let msg = e.to_string();
    tracing::error!(cmd, error = %msg);
    msg
}

#[tauri::command]
pub async fn get_connections(state: State<'_, AppState>) -> Result<Vec<ConnectionConfig>, String> {
    let list = state.store.get_connections().await;
    tracing::debug!(count = list.len(), "get_connections");
    Ok(list)
}

#[tauri::command]
pub async fn save_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<(), String> {
    tracing::info!(id = %config.id, name = %config.name, "save_connection");
    state
        .store
        .save_connection(config)
        .await
        .map_err(|e| log_err("save_connection", &e))
}

#[tauri::command]
pub async fn delete_connection(state: State<'_, AppState>, id: String) -> Result<(), String> {
    tracing::info!(%id, "delete_connection");
    state
        .store
        .delete_connection(&id)
        .await
        .map_err(|e| log_err("delete_connection", &e))
}

#[tauri::command]
pub async fn get_groups(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    Ok(state.store.get_groups().await)
}

#[tauri::command]
pub async fn save_groups(state: State<'_, AppState>, groups: Vec<String>) -> Result<(), String> {
    tracing::info!(count = groups.len(), "save_groups");
    state
        .store
        .save_groups(groups)
        .await
        .map_err(|e| log_err("save_groups", &e))
}

#[tauri::command]
pub async fn test_connection(
    state: State<'_, AppState>,
    config: ConnectionConfig,
) -> Result<ServerInfo, String> {
    tracing::info!(
        name = %config.name,
        host = ?config.host,
        port = ?config.port,
        db_type = ?config.database_type,
        "test_connection"
    );
    let result = state
        .connection_manager
        .test_connection(&config)
        .await
        .map_err(|e| log_err("test_connection", &e))?;
    tracing::info!(version = %result.server_version, "test_connection OK");
    Ok(result)
}

#[tauri::command]
pub async fn connect(state: State<'_, AppState>, config_id: String) -> Result<String, String> {
    tracing::info!(%config_id, "connect");
    let conn_id = state
        .connection_manager
        .connect(&config_id)
        .await
        .map_err(|e| log_err("connect", &e))?;

    if let Some(mut cfg) = state.store.get_connection(&config_id).await {
        cfg.last_connected_at = Some(chrono::Utc::now().to_rfc3339());
        let _ = state.store.save_connection(cfg).await;
    }

    tracing::info!(%config_id, %conn_id, "connect OK");
    Ok(conn_id)
}

#[tauri::command]
pub async fn disconnect(state: State<'_, AppState>, connection_id: String) -> Result<(), String> {
    tracing::info!(%connection_id, "disconnect");
    state
        .connection_manager
        .disconnect(&connection_id)
        .await
        .map_err(|e| log_err("disconnect", &e))?;
    state.schema_cache.clear_connection(&connection_id).await;
    tracing::info!(%connection_id, "disconnect OK");
    Ok(())
}

#[tauri::command]
pub async fn get_databases(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, String> {
    tracing::debug!(%connection_id, "get_databases");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_databases", &e))?;

    let dbs = driver
        .get_databases(&handle)
        .await
        .map_err(|e| log_err("get_databases", &e))?;
    tracing::debug!(%connection_id, count = dbs.len(), "get_databases OK");
    Ok(dbs)
}

#[tauri::command]
pub async fn get_tables(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<Vec<TableInfo>, String> {
    tracing::debug!(%connection_id, %database, "get_tables");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_tables", &e))?;

    let tables = driver
        .get_tables(&handle, &database)
        .await
        .map_err(|e| log_err("get_tables", &e))?;
    tracing::debug!(%connection_id, %database, count = tables.len(), "get_tables OK");
    Ok(tables)
}

#[tauri::command]
pub async fn get_table_schema(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
) -> Result<TableSchema, String> {
    tracing::debug!(%connection_id, %table, "get_table_schema");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_table_schema", &e))?;

    let schema = driver
        .get_table_schema(&handle, &table)
        .await
        .map_err(|e| log_err("get_table_schema", &e))?;
    tracing::debug!(%connection_id, %table, cols = schema.columns.len(), "get_table_schema OK");
    Ok(schema)
}

#[tauri::command]
pub async fn get_table_data(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
    page: u32,
    page_size: u32,
    filters: Option<Vec<FilterCondition>>,
    sorts: Option<Vec<SortCondition>>,
) -> Result<TableDataResult, String> {
    tracing::debug!(%connection_id, %table, page, page_size, "get_table_data");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_table_data", &e))?;

    let order = sorts
        .and_then(|list| list.into_iter().next())
        .map(|s| OrderBy {
            column: s.column,
            descending: s.descending,
        });

    let executor = QueryExecutor::new(state.schema_cache.clone());
    let result = executor
        .get_table_data(
            &driver,
            &handle,
            &connection_id,
            "",
            &table,
            page,
            page_size,
            filters,
            order,
        )
        .await
        .map_err(|e| log_err("get_table_data", &e))?;
    tracing::debug!(%connection_id, %table, rows = result.rows.len(), "get_table_data OK");
    Ok(result)
}

#[tauri::command]
pub async fn execute_query(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
) -> Result<MultiQueryResult, String> {
    tracing::info!(%connection_id, sql_len = sql.len(), "execute_query");
    let settings = state.store.get_settings().await;
    let limit = if settings.limit_select_results && settings.query_result_limit > 0 {
        Some(settings.query_result_limit)
    } else {
        None
    };

    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("execute_query", &e))?;

    match driver.query_multi(&handle, &sql, limit).await {
        Ok(result) => {
            tracing::info!(
                %connection_id,
                statements = result.results.len(),
                ms = result.total_time_ms,
                "execute_query OK"
            );
            let total_rows: u64 = result
                .results
                .iter()
                .filter_map(|r| r.rows_affected)
                .sum();
            let entry = QueryHistoryEntry {
                id: Uuid::new_v4().to_string(),
                connection_id: connection_id.clone(),
                database: String::new(),
                sql: sql.clone(),
                executed_at: chrono::Utc::now(),
                execution_time_ms: result.total_time_ms,
                rows_affected: Some(total_rows),
                success: true,
                error_message: None,
            };
            let _ = state.store.add_query_history(entry).await;
            Ok(result)
        }
        Err(err) => {
            tracing::error!(%connection_id, error = %err, "execute_query failed");
            let entry = QueryHistoryEntry {
                id: Uuid::new_v4().to_string(),
                connection_id: connection_id.clone(),
                database: String::new(),
                sql: sql.clone(),
                executed_at: chrono::Utc::now(),
                execution_time_ms: 0,
                rows_affected: None,
                success: false,
                error_message: Some(err.to_string()),
            };
            let _ = state.store.add_query_history(entry).await;
            Err(err.to_string())
        }
    }
}

#[tauri::command]
pub async fn get_explain(
    state: State<'_, AppState>,
    connection_id: String,
    sql: String,
) -> Result<ExplainResult, String> {
    tracing::debug!(%connection_id, "get_explain");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_explain", &e))?;

    driver
        .explain(&handle, &sql)
        .await
        .map_err(|e| log_err("get_explain", &e))
}

#[tauri::command]
pub async fn cancel_query(state: State<'_, AppState>, connection_id: String) -> Result<(), String> {
    tracing::info!(%connection_id, "cancel_query");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("cancel_query", &e))?;

    driver
        .cancel_query(&handle)
        .await
        .map_err(|e| log_err("cancel_query", &e))
}

#[tauri::command]
pub async fn get_query_history(
    state: State<'_, AppState>,
    limit: usize,
) -> Result<Vec<QueryHistoryEntry>, String> {
    Ok(state.store.get_query_history(limit).await)
}

#[tauri::command]
pub async fn clear_query_history(state: State<'_, AppState>) -> Result<(), String> {
    tracing::info!("clear_query_history");
    state
        .store
        .clear_query_history()
        .await
        .map_err(|e| log_err("clear_query_history", &e))
}

#[tauri::command]
pub async fn get_settings(state: State<'_, AppState>) -> Result<AppSettings, String> {
    Ok(state.store.get_settings().await)
}

#[tauri::command]
pub async fn save_settings(state: State<'_, AppState>, settings: AppSettings) -> Result<(), String> {
    tracing::debug!(theme = %settings.theme, "save_settings");
    state
        .store
        .save_settings(settings)
        .await
        .map_err(|e| log_err("save_settings", &e))
}

#[tauri::command]
pub async fn write_file(path: String, contents: String) -> Result<(), String> {
    let p = PathBuf::from(&path);
    tokio::fs::write(&p, contents.as_bytes())
        .await
        .map_err(|e| log_err("write_file", &e))
}

#[tauri::command]
pub async fn read_file(path: String) -> Result<String, String> {
    let p = PathBuf::from(&path);
    tokio::fs::read_to_string(&p)
        .await
        .map_err(|e| log_err("read_file", &e))
}
