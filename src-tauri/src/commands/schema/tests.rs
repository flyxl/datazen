use super::*;
use crate::db::Value;
use crate::services::SortCondition;
use crate::testing::app_state::TestAppState;
use crate::testing::mock_driver::MockDriverOptions;
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
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(data.total_rows, Some(2));

    let er = get_er_data_impl(&test.state, conn_id, "app".into(), None)
        .await
        .unwrap();
    assert_eq!(er.len(), 1);
}

#[tokio::test]
async fn get_er_data_errors_when_no_table_metadata_is_readable() {
    use crate::db::{TableInfo, TableType};

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
        None,
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
    let err = get_database_objects_impl(&test.state, conn_id, "invalid_kind".into())
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
async fn lists_database_objects_preserves_signature_and_trigger_target() {
    let opts = MockDriverOptions {
        columns: vec![
            col("schema"),
            col("name"),
            col("signature"),
            col("target_schema"),
            col("target_name"),
        ],
        query_rows: vec![vec![
            Some(Value::String("public".into())),
            Some(Value::String("audit".into())),
            Some(Value::String("integer".into())),
            Some(Value::String("public".into())),
            Some(Value::String("orders".into())),
        ]],
        ..Default::default()
    };
    let test = TestAppState::with_options(opts).await;
    let (_, conn_id) = test.save_and_connect("obj-identity").await;
    let rows = get_database_objects_impl(&test.state, conn_id, "trigger".into())
        .await
        .unwrap();
    assert_eq!(rows[0].signature.as_deref(), Some("integer"));
    assert_eq!(rows[0].target_schema.as_deref(), Some("public"));
    assert_eq!(rows[0].target_name.as_deref(), Some("orders"));
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
async fn object_ddl_missing_object_fails_closed() {
    let opts = MockDriverOptions {
        columns: vec![col("ddl")],
        query_rows: vec![],
        ..Default::default()
    };
    let test = TestAppState::with_options(opts).await;
    let (_, conn_id) = test.save_and_connect("obj-missing-ddl").await;
    let error = get_object_ddl_with_metadata_impl(
        &test.state,
        conn_id,
        "function".into(),
        "missing".into(),
        Some("public".into()),
        Some("integer".into()),
        None,
        None,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("not found"));
}

#[tokio::test]
async fn object_catalog_preserves_permission_query_errors() {
    let opts = MockDriverOptions {
        query_error: Some("permission denied for relation pg_proc".into()),
        ..Default::default()
    };
    let test = TestAppState::with_options(opts).await;
    let (_, conn_id) = test.save_and_connect("obj-permission").await;
    let error = get_database_objects_impl(&test.state, conn_id, "function".into())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("permission denied"));
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
        None,
        None,
    )
    .await
    .unwrap();
    assert_eq!(data.rows.len(), 1);
}

#[tokio::test]
async fn get_table_data_reads_the_target_database_without_switching() {
    let test = TestAppState::with_tables().await;
    let (_, conn_id) = test.save_and_connect("tdata-pin-db").await;
    // Sample config pins database = "app"; opening a table that lives in
    // another database must read *that* database (BUG-002) — via the
    // explicit argument, never by re-pointing the shared session.
    let data = get_table_data_impl(
        &test.state,
        conn_id.clone(),
        "users".into(),
        0,
        50,
        None,
        None,
        Some(true),
        None,
        Some("analytics".into()),
        None,
    )
    .await
    .unwrap();
    assert_eq!(data.rows.len(), 1);
    assert!(
        test.mock.use_database_calls().is_empty(),
        "get_table_data must not switch the session's database"
    );
    // The session keeps its own configured database; the request target is
    // per-call.
    let config = test
        .state
        .connection_manager
        .get_session_config(&conn_id)
        .await
        .unwrap();
    assert_eq!(config.database.as_deref(), Some("app"));
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
    use crate::db::{ColumnSchema, TableInfo, TableType};

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
