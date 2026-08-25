use super::error::{CmdExt, CommandError};
use super::AppState;
use crate::db::{TableDataResult, TableInfo, TableSchema};
use crate::services::{FilterCondition, OrderBy, QueryExecutor, SortCondition};
use std::time::Instant;
use tauri::State;

pub(crate) async fn get_databases_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<Vec<String>, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, "get_databases");

    let (_runtime_id, driver, handle) = state
        .connection_manager
        .resolve_session(&db_session_id)
        .await
        .cmd_err("get_databases")?;

    let dbs = driver
        .get_databases(&handle)
        .await
        .cmd_err("get_databases")?;
    tracing::info!(%db_session_id, count = dbs.len(), ms = start.elapsed().as_millis() as u64, "get_databases OK");
    Ok(dbs)
}

pub(crate) async fn use_database_impl(
    state: &AppState,
    db_session_id: String,
    database: String,
) -> Result<(), CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %database, "use_database");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("use_database")?;

    driver
        .use_database(&handle, &database)
        .await
        .cmd_err("use_database")?;
    state
        .connection_manager
        .set_active_database(&db_session_id, &database)
        .await
        .cmd_err("use_database")?;
    tracing::info!(
        %db_session_id,
        %database,
        ms = start.elapsed().as_millis() as u64,
        "use_database OK"
    );
    Ok(())
}

pub(crate) async fn get_tables_impl(
    state: &AppState,
    db_session_id: String,
    database: String,
) -> Result<Vec<TableInfo>, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %database, "get_tables");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("get_tables")?;

    let tables = driver
        .get_tables(&handle, &database)
        .await
        .cmd_err("get_tables")?;
    tracing::info!(%db_session_id, %database, count = tables.len(), ms = start.elapsed().as_millis() as u64, "get_tables OK");
    Ok(tables)
}

pub(crate) async fn get_columns_impl(
    state: &AppState,
    db_session_id: String,
    table: String,
) -> Result<Vec<String>, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %table, "get_columns");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("get_columns")?;

    let (cols, _pks) = driver
        .get_columns(&handle, &table)
        .await
        .cmd_err("get_columns")?;

    tracing::info!(%db_session_id, %table, count = cols.len(), ms = start.elapsed().as_millis() as u64, "get_columns OK");
    Ok(cols.into_iter().map(|c| c.name).collect())
}

pub(crate) async fn get_table_schema_impl(
    state: &AppState,
    db_session_id: String,
    table: String,
) -> Result<TableSchema, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %table, "get_table_schema");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("get_table_schema")?;

    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
        .await
        .cmd_err("get_table_schema")?;
    let database = config.database.as_deref().unwrap_or("default");

    let schema = state
        .schema_cache
        .get_table_schema(&db_session_id, database, &table, &driver, &handle)
        .await
        .cmd_err("get_table_schema")?;
    tracing::info!(%db_session_id, %table, cols = schema.columns.len(), indexes = schema.indexes.len(), fks = schema.foreign_keys.len(), ms = start.elapsed().as_millis() as u64, "get_table_schema OK");
    Ok(schema)
}

pub(crate) async fn get_table_data_impl(
    state: &AppState,
    db_session_id: String,
    table: String,
    page: u32,
    page_size: u32,
    filters: Option<Vec<FilterCondition>>,
    sorts: Option<Vec<SortCondition>>,
    skip_count: Option<bool>,
    filter_logic: Option<String>,
) -> Result<TableDataResult, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %table, page, page_size, "get_table_data");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
        .await
        .cmd_err("get_table_data")?;

    let config = state
        .connection_manager
        .get_session_config(&db_session_id)
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
            &db_session_id,
            database,
            &table,
            page,
            page_size,
            filters,
            order,
            effective_skip_count,
            filter_logic.as_deref(),
        )
        .await
        .cmd_err("get_table_data")?;
    tracing::info!(%db_session_id, %table, rows = result.rows.len(), ms = start.elapsed().as_millis() as u64, "get_table_data OK");
    Ok(result)
}

pub(crate) async fn get_er_data_impl(
    state: &AppState,
    db_session_id: String,
    database: String,
) -> Result<Vec<TableSchema>, CommandError> {
    let start = Instant::now();
    tracing::info!(%db_session_id, %database, "get_er_data");
    let (driver, handle) = state
        .connection_manager
        .get_session(&db_session_id)
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
            .get_table_schema(&db_session_id, &database, &table.name, &driver, &handle)
            .await
        {
            Ok(schema) => schemas.push(schema),
            Err(e) => {
                tracing::warn!(table = %table.name, error = %e, "get_er_data: skipping table");
            }
        }
    }

    tracing::info!(
        %db_session_id, %database,
        tables = schemas.len(),
        ms = start.elapsed().as_millis() as u64,
        "get_er_data OK"
    );
    Ok(schemas)
}

fn parse_objects_from_command(
    data: &serde_json::Value,
) -> Vec<crate::schema_objects::DatabaseObject> {
    data.get("objects")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

fn parse_grants_from_command(
    data: &serde_json::Value,
) -> Vec<crate::schema_objects::PrivilegeGrant> {
    data.get("grants")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default()
}

async fn run_schema_object_command(
    state: &AppState,
    db_session_id: &str,
    command: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    let result = super::driver_command::execute_driver_command_impl(
        state,
        super::driver_command::ExecuteDriverCommandRequest {
            db_session_id: Some(db_session_id.to_string()),
            driver_type: None,
            command: command.to_string(),
            input,
        },
    )
    .await?;
    Ok(result.data)
}

pub(crate) async fn get_database_objects_impl(
    state: &AppState,
    db_session_id: String,
    kind: String,
) -> Result<Vec<crate::schema_objects::DatabaseObject>, CommandError> {
    if crate::schema_objects::ObjectKind::parse(&kind).is_none() {
        return Err(CommandError::Validation(format!(
            "Unknown object kind: {kind}"
        )));
    }
    let data = run_schema_object_command(
        state,
        &db_session_id,
        "list_objects",
        serde_json::json!({ "kind": kind }),
    )
    .await?;
    Ok(parse_objects_from_command(&data))
}

pub(crate) async fn get_object_ddl_impl(
    state: &AppState,
    db_session_id: String,
    kind: String,
    name: String,
    schema: Option<String>,
) -> Result<String, CommandError> {
    if crate::schema_objects::ObjectKind::parse(&kind).is_none() {
        return Err(CommandError::Validation(format!(
            "Unknown object kind: {kind}"
        )));
    }
    let data = run_schema_object_command(
        state,
        &db_session_id,
        "get_object_ddl",
        serde_json::json!({
            "kind": kind,
            "name": name,
            "schema": schema,
        }),
    )
    .await?;
    Ok(data
        .get("ddl")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string())
}

pub(crate) async fn get_privileges_impl(
    state: &AppState,
    db_session_id: String,
) -> Result<Vec<crate::schema_objects::PrivilegeGrant>, CommandError> {
    let data = run_schema_object_command(
        state,
        &db_session_id,
        "list_privileges",
        serde_json::json!({}),
    )
    .await?;
    Ok(parse_grants_from_command(&data))
}

#[tauri::command]
pub async fn get_database_objects(
    state: State<'_, AppState>,
    db_session_id: String,
    kind: String,
) -> Result<Vec<crate::schema_objects::DatabaseObject>, CommandError> {
    get_database_objects_impl(&state, db_session_id, kind).await
}

#[tauri::command]
pub async fn get_object_ddl(
    state: State<'_, AppState>,
    db_session_id: String,
    kind: String,
    name: String,
    schema: Option<String>,
) -> Result<String, CommandError> {
    get_object_ddl_impl(&state, db_session_id, kind, name, schema).await
}

#[tauri::command]
pub async fn get_privileges(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<Vec<crate::schema_objects::PrivilegeGrant>, CommandError> {
    get_privileges_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn get_databases(
    state: State<'_, AppState>,
    db_session_id: String,
) -> Result<Vec<String>, CommandError> {
    get_databases_impl(&state, db_session_id).await
}

#[tauri::command]
pub async fn use_database(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
) -> Result<(), CommandError> {
    use_database_impl(&state, db_session_id, database).await
}

#[tauri::command]
pub async fn get_tables(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
) -> Result<Vec<TableInfo>, CommandError> {
    get_tables_impl(&state, db_session_id, database).await
}

#[tauri::command]
pub async fn get_columns(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
) -> Result<Vec<String>, CommandError> {
    get_columns_impl(&state, db_session_id, table).await
}

#[tauri::command]
pub async fn get_table_schema(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
) -> Result<TableSchema, CommandError> {
    get_table_schema_impl(&state, db_session_id, table).await
}

#[tauri::command]
pub async fn get_table_data(
    state: State<'_, AppState>,
    db_session_id: String,
    table: String,
    page: u32,
    page_size: u32,
    filters: Option<Vec<FilterCondition>>,
    sorts: Option<Vec<SortCondition>>,
    skip_count: Option<bool>,
    filter_logic: Option<String>,
) -> Result<TableDataResult, CommandError> {
    get_table_data_impl(
        &state,
        db_session_id,
        table,
        page,
        page_size,
        filters,
        sorts,
        skip_count,
        filter_logic,
    )
    .await
}

#[tauri::command]
pub async fn get_er_data(
    state: State<'_, AppState>,
    db_session_id: String,
    database: String,
) -> Result<Vec<TableSchema>, CommandError> {
    get_er_data_impl(&state, db_session_id, database).await
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
    async fn get_databases_via_connection_id_fallback() {
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
            None,
        )
        .await
        .unwrap();
        assert_eq!(data.rows.len(), 1);
    }

    fn col(name: &str) -> crate::db::ColumnSchema {
        crate::db::ColumnSchema {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }

    #[tokio::test]
    async fn unknown_object_kind_is_validation_error() {
        let test = TestAppState::new().await;
        let (_, conn_id) = test.save_and_connect("obj-bad-kind").await;
        let err = get_database_objects_impl(&test.state, conn_id, "view".into())
            .await
            .unwrap_err();
        assert!(err.to_string().contains("Unknown object kind"));
    }

    #[tokio::test]
    async fn lists_database_objects_returns_empty_when_query_has_no_rows_and_no_columns() {
        let opts = MockDriverOptions {
            columns: vec![],
            query_rows: vec![],
            ..Default::default()
        };
        let test = TestAppState::with_options(opts).await;
        let (_, conn_id) = test.save_and_connect("obj-empty").await;
        let rows = get_database_objects_impl(&test.state, conn_id, "function".into())
            .await
            .unwrap();
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn lists_database_objects_from_name_column() {
        let opts = MockDriverOptions {
            columns: vec![col("schema"), col("name")],
            query_rows: vec![
                vec![
                    Some(Value::String("public".into())),
                    Some(Value::String("fn_ok".into())),
                ],
                vec![
                    Some(Value::String("public".into())),
                    Some(Value::String("".into())),
                ],
            ],
            ..Default::default()
        };
        let test = TestAppState::with_options(opts).await;
        let (_, conn_id) = test.save_and_connect("obj-list").await;
        let rows = get_database_objects_impl(&test.state, conn_id, "function".into())
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "fn_ok");
        assert_eq!(rows[0].schema.as_deref(), Some("public"));
        assert_eq!(rows[0].kind, "function");
    }

    #[tokio::test]
    async fn object_ddl_reads_named_or_second_column() {
        let opts = MockDriverOptions {
            columns: vec![col("Function"), col("Create Function")],
            query_rows: vec![vec![
                Some(Value::String("fn_ok".into())),
                Some(Value::String("CREATE FUNCTION fn_ok() ...".into())),
            ]],
            ..Default::default()
        };
        let test = TestAppState::with_options(opts).await;
        let (_, conn_id) = test.save_and_connect("obj-ddl").await;
        let ddl = get_object_ddl_impl(
            &test.state,
            conn_id,
            "function".into(),
            "fn_ok".into(),
            Some("public".into()),
        )
        .await
        .unwrap();
        assert!(ddl.contains("CREATE FUNCTION"));
    }

    #[tokio::test]
    async fn privileges_map_grantee_and_skip_incomplete_rows() {
        let opts = MockDriverOptions {
            columns: vec![col("grantee"), col("schema"), col("name"), col("privilege")],
            query_rows: vec![
                vec![
                    Some(Value::String("alice".into())),
                    Some(Value::String("public".into())),
                    Some(Value::String("users".into())),
                    Some(Value::String("SELECT".into())),
                ],
                vec![None, None, None, None],
            ],
            ..Default::default()
        };
        let test = TestAppState::with_options(opts).await;
        let (_, conn_id) = test.save_and_connect("priv-list").await;
        let grants = get_privileges_impl(&test.state, conn_id).await.unwrap();
        assert_eq!(grants.len(), 1);
        assert_eq!(grants[0].grantee, "alice");
        assert_eq!(grants[0].object_name, "users");
        assert_eq!(grants[0].privilege, "SELECT");
    }

    #[tokio::test]
    async fn get_table_data_joins_filters_with_or() {
        use crate::db::{TableInfo, TableType};
        use crate::services::query_executor::FilterOperator;
        use crate::services::FilterCondition;

        let opts = MockDriverOptions {
            databases: vec!["app".into()],
            tables: vec![TableInfo {
                name: "items".into(),
                schema: None,
                table_type: TableType::Table,
                row_count: None,
            }],
            columns: vec![col("id"), col("status")],
            primary_keys: vec!["id".into()],
            count_total: 1,
            query_rows: vec![vec![
                Some(Value::Integer(1)),
                Some(Value::String("a".into())),
            ]],
            ..Default::default()
        };
        let test = TestAppState::with_options(opts).await;
        let (_, conn_id) = test.save_and_connect("filter-or").await;
        let data = get_table_data_impl(
            &test.state,
            conn_id,
            "items".into(),
            0,
            50,
            Some(vec![
                FilterCondition {
                    column: "id".into(),
                    operator: FilterOperator::Eq,
                    value: Value::Integer(1),
                },
                FilterCondition {
                    column: "status".into(),
                    operator: FilterOperator::Eq,
                    value: Value::String("a".into()),
                },
            ]),
            None,
            Some(true),
            Some("or".into()),
        )
        .await
        .unwrap();
        assert_eq!(data.rows.len(), 1);
    }
}
