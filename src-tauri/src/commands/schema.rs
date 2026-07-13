use super::{AppState, log_err};
use crate::db::{TableDataResult, TableInfo, TableSchema};
use crate::services::{FilterCondition, OrderBy, QueryExecutor, SortCondition};
use std::time::Instant;
use tauri::State;

#[tauri::command]
pub async fn get_databases(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, String> {
    let start = Instant::now();
    tracing::info!(%connection_id, "get_databases");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_databases", &e))?;

    let dbs = driver
        .get_databases(&handle)
        .await
        .map_err(|e| log_err("get_databases", &e))?;
    tracing::info!(%connection_id, count = dbs.len(), ms = start.elapsed().as_millis() as u64, "get_databases OK");
    Ok(dbs)
}

#[tauri::command]
pub async fn get_tables(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<Vec<TableInfo>, String> {
    let start = Instant::now();
    tracing::info!(%connection_id, %database, "get_tables");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_tables", &e))?;

    let tables = driver
        .get_tables(&handle, &database)
        .await
        .map_err(|e| log_err("get_tables", &e))?;
    tracing::info!(%connection_id, %database, count = tables.len(), ms = start.elapsed().as_millis() as u64, "get_tables OK");
    Ok(tables)
}
/// Lightweight column-only query — no FK / index lookups.
/// Used by the SQL editor for autocompletion.
#[tauri::command]
pub async fn get_columns(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
) -> Result<Vec<String>, String> {
    let start = Instant::now();
    tracing::info!(%connection_id, %table, "get_columns");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_columns", &e))?;

    let (cols, _pks) = driver
        .get_columns(&handle, &table)
        .await
        .map_err(|e| log_err("get_columns", &e))?;

    tracing::info!(%connection_id, %table, count = cols.len(), ms = start.elapsed().as_millis() as u64, "get_columns OK");
    Ok(cols.into_iter().map(|c| c.name).collect())
}

#[tauri::command]
pub async fn get_table_schema(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
) -> Result<TableSchema, String> {
    let start = Instant::now();
    tracing::info!(%connection_id, %table, "get_table_schema");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .map_err(|e| log_err("get_table_schema", &e))?;

    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .map_err(|e| log_err("get_table_schema", &e))?;
    let database = config.database.as_deref().unwrap_or("default");

    let schema = state
        .schema_cache
        .get_table_schema(&connection_id, database, &table, &driver, &handle)
        .await
        .map_err(|e| log_err("get_table_schema", &e))?;
    tracing::info!(%connection_id, %table, cols = schema.columns.len(), indexes = schema.indexes.len(), fks = schema.foreign_keys.len(), ms = start.elapsed().as_millis() as u64, "get_table_schema OK");
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
    skip_count: Option<bool>,
) -> Result<TableDataResult, String> {
    let start = Instant::now();
    tracing::info!(%connection_id, %table, page, page_size, "get_table_data");
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

    // For Kiwi connections, always skip COUNT(*) — max 1000 rows anyway
    let effective_skip_count = skip_count.unwrap_or(false) || driver.skip_count_query();

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
            effective_skip_count,
        )
        .await
        .map_err(|e| log_err("get_table_data", &e))?;
    tracing::info!(%connection_id, %table, rows = result.rows.len(), ms = start.elapsed().as_millis() as u64, "get_table_data OK");
    Ok(result)
}
