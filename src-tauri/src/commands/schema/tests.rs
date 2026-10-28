use super::*;
use crate::testing::app_state::TestAppState;
use crate::testing::mock_driver::MockDriverOptions;
use datazen_driver_api::Value;
use std::collections::HashMap;

#[tokio::test]
async fn schema_commands_with_connected_mock() {
    let test = TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("schema-cfg").await;

    let dbs = read_databases(&test.state, conn_id.clone()).await.unwrap();
    assert_eq!(dbs, vec!["app"]);

    let tables = read_catalog(&test.state, conn_id.clone(), "app".into(), None)
        .await
        .unwrap();
    assert_eq!(tables.len(), 1);
    assert_eq!(tables[0].relation.name, "users");

    let cols = read_column_names(
        &test.state,
        conn_id.clone(),
        "users".into(),
        "app".into(),
        None,
    )
    .await
    .unwrap();
    assert!(cols.contains(&"id".to_string()));
    assert!(cols.contains(&"name".to_string()));

    let schema = read_definition(
        &test.state,
        conn_id.clone(),
        "users".into(),
        "app".into(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(schema.table_name, "users");
    assert_eq!(schema.columns.len(), 2);

    let er = get_er_data_impl(&test.state, conn_id, "app".into(), None)
        .await
        .unwrap();
    assert_eq!(er.len(), 1);
}

#[tokio::test]
async fn get_er_data_errors_when_no_table_metadata_is_readable() {
    use datazen_driver_api::{TableInfo, TableType};

    // Schema-level driver that never supplies a schema (no explicit filter,
    // table schema None, no default): every ExactSchema metadata read fails
    // while the driver still lists tables.
    let opts = MockDriverOptions {
        has_schema_level: true,
        tables: vec![TableInfo {
            name: "users".into(),
            schema: None,
            table_type: TableType::Table,
            row_count: None,
        }],
        ..Default::default()
    };
    let test = TestAppState::with_options(opts).await;
    let (_, conn_id) = test.save_and_connect("er-all-fail").await;
    // When tables exist but every per-table column read fails, an empty Vec
    // would be rendered as "this database has no tables". Fail loudly instead.
    let err = get_er_data_impl(&test.state, conn_id, "app".into(), None)
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("failed to read column metadata for all"),
        "expected the all-tables-failed error, got: {err}"
    );
}

#[tokio::test]
async fn list_databases_rejects_connection_id_without_live_session() {
    let test = TestAppState::with_tables().await;
    test.save_connection("cfg-fallback").await;
    let err = read_databases(&test.state, "cfg-fallback".into())
        .await
        .unwrap_err();
    assert!(
        err.to_string().contains("DB session"),
        "expected dbSessionId error, got: {err}"
    );
}

#[tokio::test]
async fn schema_commands_error_when_not_connected() {
    let test = TestAppState::new().await;
    assert!(
        read_catalog(&test.state, "missing".into(), "app".into(), None)
            .await
            .is_err()
    );
}
fn col(name: &str) -> datazen_driver_api::ColumnSchema {
    datazen_driver_api::ColumnSchema {
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
/// Regression (BUG-003): `get_er_data` listed the requested database's
/// tables but read every table's schema against whatever catalog the shared
/// session sat on. That produced a successful but column-less schema, which
/// the schema cache stored under the requested database and served for its
/// whole TTL — empty structure view, ER diagram without columns, and a data
/// grid with the right row count but blank cells. Each read now carries its
/// own `(database, schema)` target.
#[tokio::test]
async fn get_er_data_reads_each_tables_own_schema_and_never_caches_empty() {
    use datazen_driver_api::{ColumnSchema, TableInfo, TableType};

    fn column(name: &str) -> ColumnSchema {
        ColumnSchema {
            name: name.into(),
            data_type: "text".into(),
            nullable: true,
            default_value: None,
            comment: None,
            is_primary_key: false,
            is_auto_increment: false,
        }
    }

    let opts = MockDriverOptions {
        databases: vec!["app".into(), "analytics".into()],
        tables_by_database: HashMap::from([(
            "analytics".to_string(),
            vec![TableInfo {
                name: "events".into(),
                schema: None,
                table_type: TableType::Table,
                row_count: None,
            }],
        )]),
        columns_by_database: HashMap::from([(
            "analytics".to_string(),
            HashMap::from([(
                "events".to_string(),
                vec![column("id"), column("name"), column("created_at")],
            )]),
        )]),
        ..Default::default()
    };
    let test = TestAppState::with_options(opts).await;
    // Sample config pins database = "app"; the ER view targets "analytics".
    let (_, conn_id) = test.save_and_connect("er-pin-db").await;

    let schemas = get_er_data_impl(&test.state, conn_id.clone(), "analytics".into(), None)
        .await
        .unwrap();

    assert!(
        test.mock.use_database_calls().is_empty(),
        "get_er_data must read each table's schema explicitly, not by switching the session"
    );
    assert_eq!(schemas.len(), 1);
    assert_eq!(
        schemas[0].columns.len(),
        3,
        "columns must come from the requested database"
    );

    // The later reads of that database must see the same columns instead of
    // a poisoned (empty) cache entry.
    let columns = read_column_names(
        &test.state,
        conn_id.clone(),
        "events".into(),
        "analytics".into(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(columns, vec!["id", "name", "created_at"]);

    let schema = read_definition(
        &test.state,
        conn_id,
        "events".into(),
        "analytics".into(),
        None,
    )
    .await
    .unwrap();
    assert_eq!(schema.columns.len(), 3);
}
async fn execute_metadata(
    state: &AppState,
    session: String,
    command: &str,
    input: serde_json::Value,
) -> Result<serde_json::Value, CommandError> {
    super::super::driver_command::execute_driver_command_impl(
        state,
        super::super::driver_command::ExecuteDriverCommandRequest {
            db_session_id: Some(session),
            driver_type: None,
            command: command.into(),
            input,
            database: None,
            schema: None,
        },
    )
    .await
    .map(|result| result.data)
}

async fn read_databases(state: &AppState, session: String) -> Result<Vec<String>, CommandError> {
    let data = execute_metadata(state, session, "list_databases", serde_json::json!({})).await?;
    Ok(datazen_driver_api::parse_databases_from_command(&data))
}

async fn read_catalog(
    state: &AppState,
    session: String,
    database: String,
    schema: Option<String>,
) -> Result<Vec<datazen_driver_api::schema_metadata::RelationSummary>, CommandError> {
    let data = execute_metadata(
        state,
        session,
        "list_catalog",
        serde_json::json!({ "database": database, "schema": schema }),
    )
    .await?;
    let result: datazen_driver_api::schema_metadata::ListCatalogOutput =
        serde_json::from_value(data)?;
    Ok(result.relations)
}

async fn read_column_names(
    state: &AppState,
    session: String,
    name: String,
    database: String,
    schema: Option<String>,
) -> Result<Vec<String>, CommandError> {
    let data = execute_metadata(state, session, "read_relation_columns", serde_json::json!({ "relations": [{ "database": database, "schema": schema, "name": name }] })).await?;
    let result: datazen_driver_api::schema_metadata::ReadColumnsOutput =
        serde_json::from_value(data)?;
    match result
        .results
        .into_iter()
        .next()
        .expect("requested one relation")
    {
        datazen_driver_api::schema_metadata::ColumnsReadResult::Ok { value, .. } => Ok(value
            .columns
            .into_iter()
            .map(|column| column.name)
            .collect()),
        datazen_driver_api::schema_metadata::ColumnsReadResult::Error { error, .. } => {
            Err(CommandError::Internal(error.message))
        }
    }
}

async fn read_definition(
    state: &AppState,
    session: String,
    name: String,
    database: String,
    schema: Option<String>,
) -> Result<TableSchema, CommandError> {
    let data = execute_metadata(
        state,
        session,
        "read_relation_schema",
        serde_json::json!({ "relation": { "database": database, "schema": schema, "name": name } }),
    )
    .await?;
    let result: datazen_driver_api::schema_metadata::RelationSchema =
        serde_json::from_value(data["value"].clone())?;
    Ok(result.definition)
}
