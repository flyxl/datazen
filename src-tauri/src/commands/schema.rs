use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{TableDataResult, TableInfo, TableSchema};
use crate::services::{FilterCondition, OrderBy, QueryExecutor, SortCondition};
use std::time::Instant;
use tauri::State;

#[tauri::command]
pub async fn get_databases(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, CommandError> {
    let start = Instant::now();
    tracing::info!(%connection_id, "get_databases");

    let (_runtime_id, driver, handle) = state
        .connection_manager
        .resolve_session(&connection_id)
        .await
        .cmd_err("get_databases")?;

    let dbs = driver
        .get_databases(&handle)
        .await
        .cmd_err("get_databases")?;
    tracing::info!(%connection_id, count = dbs.len(), ms = start.elapsed().as_millis() as u64, "get_databases OK");
    Ok(dbs)
}

#[tauri::command]
pub async fn use_database(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<(), CommandError> {
    let start = Instant::now();
    tracing::info!(%connection_id, %database, "use_database");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("use_database")?;

    driver
        .use_database(&handle, &database)
        .await
        .cmd_err("use_database")?;
    state
        .connection_manager
        .set_active_database(&connection_id, &database)
        .await
        .cmd_err("use_database")?;
    tracing::info!(
        %connection_id,
        %database,
        ms = start.elapsed().as_millis() as u64,
        "use_database OK"
    );
    Ok(())
}

#[tauri::command]
pub async fn get_tables(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<Vec<TableInfo>, CommandError> {
    let start = Instant::now();
    tracing::info!(%connection_id, %database, "get_tables");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("get_tables")?;

    let tables = driver
        .get_tables(&handle, &database)
        .await
        .cmd_err("get_tables")?;
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
) -> Result<Vec<String>, CommandError> {
    let start = Instant::now();
    tracing::info!(%connection_id, %table, "get_columns");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("get_columns")?;

    let (cols, _pks) = driver
        .get_columns(&handle, &table)
        .await
        .cmd_err("get_columns")?;

    tracing::info!(%connection_id, %table, count = cols.len(), ms = start.elapsed().as_millis() as u64, "get_columns OK");
    Ok(cols.into_iter().map(|c| c.name).collect())
}

#[tauri::command]
pub async fn get_table_schema(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
) -> Result<TableSchema, CommandError> {
    let start = Instant::now();
    tracing::info!(%connection_id, %table, "get_table_schema");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("get_table_schema")?;

    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .cmd_err("get_table_schema")?;
    let database = config.database.as_deref().unwrap_or("default");

    let schema = state
        .schema_cache
        .get_table_schema(&connection_id, database, &table, &driver, &handle)
        .await
        .cmd_err("get_table_schema")?;
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
) -> Result<TableDataResult, CommandError> {
    let start = Instant::now();
    tracing::info!(%connection_id, %table, page, page_size, "get_table_data");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("get_table_data")?;

    let config = state
        .connection_manager
        .get_connection_config(&connection_id)
        .await
        .cmd_err("get_table_data")?;
    let database = config.database.as_deref().unwrap_or("default");

    let order = sorts
        .and_then(|list| list.into_iter().next())
        .map(|s| OrderBy {
            column: s.column,
            descending: s.descending,
        });

    let effective_skip_count = skip_count.unwrap_or(false) || driver.skip_count_query();

    let executor = QueryExecutor::new(state.schema_cache.clone());
    let result = executor
        .get_table_data(
            &driver,
            &handle,
            &connection_id,
            database,
            &table,
            page,
            page_size,
            filters,
            order,
            effective_skip_count,
        )
        .await
        .cmd_err("get_table_data")?;
    tracing::info!(%connection_id, %table, rows = result.rows.len(), ms = start.elapsed().as_millis() as u64, "get_table_data OK");
    Ok(result)
}

#[tauri::command]
pub async fn get_er_data(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<Vec<TableSchema>, CommandError> {
    let start = Instant::now();
    tracing::info!(%connection_id, %database, "get_er_data");
    let (driver, handle) = state
        .connection_manager
        .get_connection(&connection_id)
        .await
        .cmd_err("get_er_data")?;

    let tables = driver
        .get_tables(&handle, &database)
        .await
        .cmd_err("get_er_data")?;

    let mut schemas = Vec::with_capacity(tables.len());
    for table in &tables {
        match state
            .schema_cache
            .get_table_schema(&connection_id, &database, &table.name, &driver, &handle)
            .await
        {
            Ok(schema) => schemas.push(schema),
            Err(e) => {
                tracing::warn!(table = %table.name, error = %e, "get_er_data: skipping table");
            }
        }
    }

    tracing::info!(
        %connection_id, %database,
        tables = schemas.len(),
        ms = start.elapsed().as_millis() as u64,
        "get_er_data OK"
    );
    Ok(schemas)
}
