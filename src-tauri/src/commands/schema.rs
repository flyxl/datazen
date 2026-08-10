use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{TableDataResult, TableInfo, TableSchema};
use crate::services::{FilterCondition, OrderBy, QueryExecutor, SortCondition};
use std::time::Instant;
use tauri::State;

pub(crate) async fn get_databases_impl(
    state: &AppState,
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

pub(crate) async fn use_database_impl(
    state: &AppState,
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

pub(crate) async fn get_tables_impl(
    state: &AppState,
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

pub(crate) async fn get_columns_impl(
    state: &AppState,
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

pub(crate) async fn get_table_schema_impl(
    state: &AppState,
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

pub(crate) async fn get_table_data_impl(
    state: &AppState,
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

pub(crate) async fn get_er_data_impl(
    state: &AppState,
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

#[tauri::command]
pub async fn get_databases(
    state: State<'_, AppState>,
    connection_id: String,
) -> Result<Vec<String>, CommandError> {
    get_databases_impl(&state, connection_id).await
}

#[tauri::command]
pub async fn use_database(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<(), CommandError> {
    use_database_impl(&state, connection_id, database).await
}

#[tauri::command]
pub async fn get_tables(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<Vec<TableInfo>, CommandError> {
    get_tables_impl(&state, connection_id, database).await
}

#[tauri::command]
pub async fn get_columns(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
) -> Result<Vec<String>, CommandError> {
    get_columns_impl(&state, connection_id, table).await
}

#[tauri::command]
pub async fn get_table_schema(
    state: State<'_, AppState>,
    connection_id: String,
    table: String,
) -> Result<TableSchema, CommandError> {
    get_table_schema_impl(&state, connection_id, table).await
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
    get_table_data_impl(
        &state,
        connection_id,
        table,
        page,
        page_size,
        filters,
        sorts,
        skip_count,
    )
    .await
}

#[tauri::command]
pub async fn get_er_data(
    state: State<'_, AppState>,
    connection_id: String,
    database: String,
) -> Result<Vec<TableSchema>, CommandError> {
    get_er_data_impl(&state, connection_id, database).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Value;
    use crate::services::SortCondition;
    use crate::testing::app_state::TestAppState;
    use crate::testing::mock_driver::MockDriverOptions;

    #[tokio::test]
    async fn schema_commands_with_connected_mock() {
        let test = TestAppState::with_tables().await;
        let (_, conn_id) = test.save_and_connect("schema-cfg").await;

        let dbs = get_databases_impl(&test.state, conn_id.clone())
            .await
            .unwrap();
        assert_eq!(dbs, vec!["app"]);

        use_database_impl(&test.state, conn_id.clone(), "app".into())
            .await
            .unwrap();

        let tables = get_tables_impl(&test.state, conn_id.clone(), "app".into())
            .await
            .unwrap();
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].name, "users");

        let cols = get_columns_impl(&test.state, conn_id.clone(), "users".into())
            .await
            .unwrap();
        assert!(cols.contains(&"id".to_string()));
        assert!(cols.contains(&"name".to_string()));

        let schema = get_table_schema_impl(&test.state, conn_id.clone(), "users".into())
            .await
            .unwrap();
        assert_eq!(schema.table_name, "users");
        assert_eq!(schema.columns.len(), 2);

        let data = get_table_data_impl(
            &test.state,
            conn_id.clone(),
            "users".into(),
            0,
            10,
            None,
            Some(vec![SortCondition {
                column: "id".into(),
                descending: false,
            }]),
            None,
        )
        .await
        .unwrap();
        assert_eq!(data.total_rows, Some(2));

        let er = get_er_data_impl(&test.state, conn_id, "app".into())
            .await
            .unwrap();
        assert_eq!(er.len(), 1);
    }

    #[tokio::test]
    async fn get_databases_via_config_id_fallback() {
        let test = TestAppState::with_tables().await;
        test.save_connection("cfg-fallback").await;
        let dbs = get_databases_impl(&test.state, "cfg-fallback".into())
            .await
            .unwrap();
        assert_eq!(dbs, vec!["app"]);
    }

    #[tokio::test]
    async fn schema_commands_error_when_not_connected() {
        let test = TestAppState::new().await;
        assert!(get_tables_impl(&test.state, "missing".into(), "app".into())
            .await
            .is_err());
    }

    #[tokio::test]
    async fn get_table_data_with_query_rows() {
        use crate::db::{ColumnSchema, TableInfo, TableType};

        let opts = MockDriverOptions {
            databases: vec!["app".into()],
            tables: vec![TableInfo {
                name: "items".into(),
                schema: None,
                table_type: TableType::Table,
                row_count: None,
            }],
            columns: vec![ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            }],
            primary_keys: vec!["id".into()],
            count_total: 1,
            query_rows: vec![vec![Some(Value::Integer(1))]],
            ..Default::default()
        };
        let test = TestAppState::with_options(opts).await;
        let (_, conn_id) = test.save_and_connect("data-cfg").await;
        let data = get_table_data_impl(
            &test.state,
            conn_id,
            "items".into(),
            0,
            50,
            None,
            None,
            Some(true),
        )
        .await
        .unwrap();
        assert_eq!(data.rows.len(), 1);
    }
}
