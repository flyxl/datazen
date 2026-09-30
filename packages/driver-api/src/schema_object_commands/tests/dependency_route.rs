use crate::schema_object_commands::execute_schema_object_command;
use crate::{
    async_trait, ConnectionConfig, ConnectionHandle, DatabaseDriver, DatabaseType, DriverError,
    MultiQueryResult, QueryResult, ServerInfo, SqlTarget, TableInfo, TableSchema, Value,
};
use serde_json::{json, Value as JsonValue};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

enum QueryReply {
    Error,
    MalformedCatalog,
    TableCatalog,
    Grants(Vec<String>),
    Catalog(QueryResult),
    UdfCount(i64),
}

struct TrackingDriver {
    db_type: String,
    queries: AtomicUsize,
    queried_sql: Mutex<Vec<String>>,
    query_databases: Mutex<Vec<Option<String>>>,
    replies: Mutex<VecDeque<QueryReply>>,
}

impl TrackingDriver {
    fn new(reply: QueryReply) -> Self {
        Self::for_type("mysql", [reply])
    }

    fn for_type(db_type: &str, replies: impl IntoIterator<Item = QueryReply>) -> Self {
        Self {
            db_type: db_type.into(),
            queries: AtomicUsize::new(0),
            queried_sql: Mutex::new(Vec::new()),
            query_databases: Mutex::new(Vec::new()),
            replies: Mutex::new(replies.into_iter().collect()),
        }
    }

    fn scripted(replies: impl IntoIterator<Item = QueryReply>) -> Self {
        Self::for_type("mysql", replies)
    }

    fn query_count(&self) -> usize {
        self.queries.load(Ordering::SeqCst)
    }

    fn queried_sql(&self) -> Vec<String> {
        self.queried_sql.lock().unwrap().clone()
    }

    fn query_databases(&self) -> Vec<Option<String>> {
        self.query_databases.lock().unwrap().clone()
    }
}

#[async_trait]
impl DatabaseDriver for TrackingDriver {
    fn driver_type(&self) -> DatabaseType {
        self.db_type.clone()
    }

    async fn connect(&self, _: &ConnectionConfig) -> Result<ConnectionHandle, DriverError> {
        Ok(handle())
    }

    async fn test_connection(&self, _: &ConnectionConfig) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: self.driver_type(),
        })
    }

    async fn disconnect(&self, _: ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn get_databases(&self, _: &ConnectionHandle) -> Result<Vec<String>, DriverError> {
        Ok(Vec::new())
    }

    async fn get_tables(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: Option<&str>,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Ok(Vec::new())
    }

    async fn get_table_schema(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: &str,
        _: Option<&str>,
    ) -> Result<TableSchema, DriverError> {
        Ok(TableSchema {
            table_name: String::new(),
            columns: Vec::new(),
            primary_keys: Vec::new(),
            indexes: Vec::new(),
            foreign_keys: Vec::new(),
            check_constraints: Vec::new(),
            table_options: Default::default(),
        })
    }

    async fn query(&self, _: &ConnectionHandle, sql: &str) -> Result<QueryResult, DriverError> {
        self.queries.fetch_add(1, Ordering::SeqCst);
        self.queried_sql.lock().unwrap().push(sql.to_string());
        let reply = self.replies.lock().unwrap().pop_front();
        match reply {
            Some(QueryReply::Error) => Err(DriverError::QueryFailed("catalog unavailable".into())),
            Some(QueryReply::MalformedCatalog) => Ok(QueryResult {
                columns: Vec::new(),
                rows: Vec::new(),
                rows_affected: None,
                execution_time_ms: 0,
            }),
            Some(QueryReply::TableCatalog) => Ok(QueryResult {
                columns: [
                    "selected_count",
                    "unsupported_count",
                    "kind",
                    "dependency_schema",
                    "name",
                    "signature",
                ]
                .into_iter()
                .map(super::col)
                .collect(),
                rows: vec![vec![
                    Some(Value::Integer(1)),
                    Some(Value::Integer(0)),
                    Some(Value::String("table".into())),
                    Some(Value::String("parent_db".into())),
                    Some(Value::String("parent".into())),
                    None,
                ]],
                rows_affected: None,
                execution_time_ms: 0,
            }),
            Some(QueryReply::Grants(grants)) => Ok(QueryResult {
                columns: vec![super::col("Grants for dependency-test")],
                rows: grants
                    .into_iter()
                    .map(|grant| vec![Some(Value::String(grant))])
                    .collect(),
                rows_affected: None,
                execution_time_ms: 0,
            }),
            Some(QueryReply::Catalog(result)) => Ok(result),
            Some(QueryReply::UdfCount(count)) => Ok(QueryResult {
                columns: vec![super::col("udf_count")],
                rows: vec![vec![Some(Value::Integer(count))]],
                rows_affected: None,
                execution_time_ms: 0,
            }),
            None => Err(DriverError::QueryFailed(
                "unexpected query without a scripted response".into(),
            )),
        }
    }

    async fn query_at(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        target: SqlTarget<'_>,
    ) -> Result<QueryResult, DriverError> {
        self.query_databases
            .lock()
            .unwrap()
            .push(target.database().map(str::to_string));
        self.query(handle, sql).await
    }

    async fn query_multi(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        Ok(MultiQueryResult {
            results: Vec::new(),
            total_time_ms: 0,
        })
    }

    async fn query_with_params(
        &self,
        _: &ConnectionHandle,
        _: &str,
        _: &[Value],
    ) -> Result<QueryResult, DriverError> {
        Ok(QueryResult {
            columns: Vec::new(),
            rows: Vec::new(),
            rows_affected: None,
            execution_time_ms: 0,
        })
    }

    async fn execute(&self, _: &ConnectionHandle, _: &str) -> Result<u64, DriverError> {
        Ok(0)
    }

    async fn cancel_query(&self, _: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

fn handle() -> ConnectionHandle {
    ConnectionHandle {
        id: "dependency-test".into(),
        pool_id: "dependency-test-pool".into(),
    }
}

async fn run_dependency_command(driver: &TrackingDriver, input: JsonValue) -> JsonValue {
    run_dependency_command_for(driver, &driver.db_type, input).await
}

async fn run_dependency_command_for(
    driver: &TrackingDriver,
    db_type: &str,
    input: JsonValue,
) -> JsonValue {
    execute_schema_object_command(driver, db_type, &handle(), "get_object_dependencies", input)
        .await
        .expect("dependency command returns an incomplete result rather than failing open")
        .data
}

fn assert_incomplete_without_edges(result: &JsonValue) {
    assert_eq!(
        result["complete"], false,
        "catalog must fail closed: {result}"
    );
    assert_eq!(result["dependencies"], json!([]));
}

#[tokio::test]
async fn postgres_object_catalog_query_uses_database_selected_in_tree() {
    let catalog = QueryResult {
        columns: ["schema", "name", "signature"]
            .into_iter()
            .map(super::col)
            .collect(),
        rows: Vec::new(),
        rows_affected: None,
        execution_time_ms: 0,
    };
    let driver = TrackingDriver::for_type("postgresql", [QueryReply::Catalog(catalog)]);

    let result = execute_schema_object_command(
        &driver,
        "postgresql",
        &handle(),
        "list_objects",
        json!({"kind":"function", "database":"manual_fixture_db"}),
    )
    .await
    .unwrap();

    assert_eq!(result.data["objects"], json!([]));
    assert_eq!(
        driver.query_databases(),
        vec![Some("manual_fixture_db".into())]
    );
}

#[tokio::test]
async fn postgres_object_ddl_query_uses_database_selected_in_tree() {
    let ddl = "CREATE FUNCTION public.lookup_code(integer) RETURNS text";
    let result = QueryResult {
        columns: vec![super::col("ddl")],
        rows: vec![vec![Some(Value::String(ddl.into()))]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let driver = TrackingDriver::for_type("postgresql", [QueryReply::Catalog(result)]);

    let result = execute_schema_object_command(
        &driver,
        "postgresql",
        &handle(),
        "get_object_ddl",
        json!({
            "kind":"function",
            "name":"lookup_code",
            "schema":"public",
            "signature":"integer",
            "database":"manual_fixture_db"
        }),
    )
    .await
    .unwrap();

    assert_eq!(result.data["ddl"], json!(ddl));
    assert_eq!(
        driver.query_databases(),
        vec![Some("manual_fixture_db".into())]
    );
}

#[tokio::test]
async fn mysql_object_ddl_uses_database_when_schema_metadata_is_absent() {
    let result = QueryResult {
        columns: vec![super::col("Create Procedure")],
        rows: vec![vec![Some(Value::String(
            "CREATE PROCEDURE `target_db`.`refresh_cache`() SELECT 1".into(),
        ))]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    let driver = TrackingDriver::for_type("mysql", [QueryReply::Catalog(result)]);

    execute_schema_object_command(
        &driver,
        "mysql",
        &handle(),
        "get_object_ddl",
        json!({
            "kind":"procedure",
            "name":"refresh_cache",
            "database":"target_db"
        }),
    )
    .await
    .unwrap();

    assert_eq!(
        driver.queried_sql(),
        vec!["SHOW CREATE PROCEDURE `target_db`.`refresh_cache`"]
    );
}

#[tokio::test]
async fn unsupported_and_invalid_dependency_inputs_return_incomplete_without_querying() {
    let driver = TrackingDriver::new(QueryReply::MalformedCatalog);
    for input in [
        json!({"kind":"trigger","name":"before_insert"}),
        json!({"kind":"not-a-kind","name":"object"}),
        json!({"name":"object"}),
        json!({"kind":"table"}),
        json!({"kind":"table","name":""}),
        json!({"kind":"table","name":"  \t  "}),
    ] {
        let result = run_dependency_command(&driver, input).await;
        assert_incomplete_without_edges(&result);
    }
    assert_eq!(
        driver.query_count(),
        0,
        "early blockers must not query the database"
    );
}

#[tokio::test]
async fn dependency_catalog_query_error_returns_incomplete_without_edges() {
    let driver = TrackingDriver::new(QueryReply::Error);
    let result = run_dependency_command(&driver, json!({"kind":"table","name":"child"})).await;

    assert_incomplete_without_edges(&result);
    assert_eq!(driver.query_count(), 1);
}

#[tokio::test]
async fn malformed_dependency_catalog_returns_incomplete_without_edges() {
    let driver = TrackingDriver::new(QueryReply::MalformedCatalog);
    let result = run_dependency_command(&driver, json!({"kind":"table","name":"child"})).await;

    assert_incomplete_without_edges(&result);
    assert_eq!(driver.query_count(), 1);
}

#[tokio::test]
async fn mysql_table_catalog_and_global_select_grant_return_exact_parent_edge() {
    let driver = TrackingDriver::scripted([
        QueryReply::TableCatalog,
        QueryReply::Grants(vec!["GRANT SELECT ON *.* TO 'migration'@'%'".into()]),
    ]);
    let result = run_dependency_command(
        &driver,
        json!({"kind":"table","schema":"child_db","name":"child"}),
    )
    .await;

    assert_eq!(
        result["complete"], true,
        "complete visible catalog: {result}"
    );
    assert_eq!(
        result["dependencies"],
        json!([{"kind":"table","schema":"parent_db","name":"parent"}])
    );
    let sql = driver.queried_sql();
    assert_eq!(sql.len(), 2);
    assert!(sql[0].contains("information_schema.TABLE_CONSTRAINTS"));
    assert_eq!(sql[1], "SHOW GRANTS FOR CURRENT_USER()");
    assert_eq!(driver.query_count(), 2);
}

#[tokio::test]
async fn mysql_table_partial_revoke_keeps_catalog_incomplete() {
    let driver = TrackingDriver::scripted([
        QueryReply::TableCatalog,
        QueryReply::Grants(vec![
            "GRANT SELECT ON *.* TO 'migration'@'%'".into(),
            "REVOKE SELECT ON hidden_db.* FROM 'migration'@'%'".into(),
        ]),
    ]);
    let result = run_dependency_command(
        &driver,
        json!({"kind":"table","schema":"child_db","name":"child"}),
    )
    .await;

    assert_eq!(
        result["complete"], false,
        "partial revoke blocks proof: {result}"
    );
    assert_eq!(
        result["dependencies"],
        json!([{"kind":"table","schema":"parent_db","name":"parent"}]),
        "observed edges remain visible even though the catalog is incomplete"
    );
    assert_eq!(driver.query_count(), 2);
}

fn empty_dependency_catalog() -> QueryResult {
    QueryResult {
        columns: [
            "selected_count",
            "unsupported_count",
            "kind",
            "dependency_schema",
            "name",
            "signature",
            "type_usage",
            "column_name",
            "sequence_usage",
            "sequence_schema",
            "sequence_name",
            "owner_table_schema",
            "owner_table_name",
            "owner_column_name",
        ]
        .into_iter()
        .map(super::col)
        .collect(),
        rows: vec![vec![
            Some(Value::Integer(1)),
            Some(Value::Integer(0)),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    }
}

#[tokio::test]
async fn postgres_dependency_dispatch_builds_filtered_catalog_for_every_supported_kind() {
    let cases = [
        ("view", "pg_catalog.pg_rewrite"),
        ("trigger", "pg_catalog.pg_trigger"),
        ("function", "pg_catalog.pg_proc"),
        ("table", "pg_catalog.pg_attrdef"),
        ("sequence", "pg_catalog.pg_class"),
        ("type", "pg_catalog.pg_type"),
    ];

    for (kind, expected_catalog) in cases {
        let driver = TrackingDriver::for_type(
            "postgresql",
            [QueryReply::Catalog(empty_dependency_catalog())],
        );
        let mut input = json!({"kind":kind,"name":"object","schema":" app "});
        if kind == "function" {
            input["signature"] = json!("integer");
        }
        if kind == "trigger" {
            input["schema"] = json!("  ");
            input["targetSchema"] = json!("  ");
            input["targetName"] = json!("");
        }

        let result = run_dependency_command_for(&driver, "postgresql", input).await;

        assert_eq!(result["complete"], true, "{kind}: {result}");
        assert_eq!(result["dependencies"], json!([]));
        let queries = driver.queried_sql();
        assert_eq!(queries.len(), 1, "{kind}");
        assert!(
            queries[0].contains(expected_catalog),
            "{kind}: {}",
            queries[0]
        );
        match kind {
            "view" => assert!(queries[0].contains("view_ns.nspname = 'app'")),
            "trigger" => {
                assert!(queries[0].contains("trigger.tgname = 'object'"));
                assert!(!queries[0].contains("target_ns.nspname = 'app'"));
                assert!(!queries[0].contains("target_rel.relname = ''"));
            }
            "function" => assert!(
                queries[0].contains("pg_get_function_identity_arguments(proc.oid) = 'integer'")
            ),
            "table" => assert!(queries[0].contains("relation_ns.nspname = 'app'")),
            "sequence" => assert!(queries[0].contains("sequence_ns.nspname = 'app'")),
            "type" => assert!(queries[0].contains("type_ns.nspname = 'app'")),
            _ => unreachable!(),
        }
    }
}

#[tokio::test]
async fn postgres_table_catalog_parses_type_and_column_default_sequence_usage() {
    let mut catalog = empty_dependency_catalog();
    catalog.rows.push(vec![
        Some(Value::Integer(1)),
        Some(Value::Integer(0)),
        Some(Value::String("type".into())),
        Some(Value::String("app".into())),
        Some(Value::String("order_state".into())),
        None,
        Some(Value::String("column_type".into())),
        Some(Value::String("state".into())),
        None,
        None,
        None,
        None,
        None,
        None,
    ]);
    catalog.rows.push(vec![
        Some(Value::Integer(1)),
        Some(Value::Integer(0)),
        Some(Value::String("sequence".into())),
        Some(Value::String("app".into())),
        Some(Value::String("orders_id_seq".into())),
        None,
        None,
        None,
        Some(Value::String("column_default".into())),
        Some(Value::String("app".into())),
        Some(Value::String("orders_id_seq".into())),
        Some(Value::String("app".into())),
        Some(Value::String("orders".into())),
        Some(Value::String("id".into())),
    ]);
    let driver = TrackingDriver::for_type("postgresql", [QueryReply::Catalog(catalog)]);

    let result = run_dependency_command_for(
        &driver,
        "postgresql",
        json!({"kind":"table","schema":"app","name":"orders"}),
    )
    .await;

    assert_eq!(result["complete"], true, "{result}");
    assert_eq!(
        result["dependencies"],
        json!([
            {"kind":"sequence","schema":"app","name":"orders_id_seq"},
            {"kind":"type","schema":"app","name":"order_state"}
        ])
    );
    assert_eq!(
        result["typeDependencyUsages"],
        json!([{
            "dependency":{"kind":"type","schema":"app","name":"order_state"},
            "usage":"column_type",
            "columnName":"state"
        }])
    );
    assert_eq!(
        result["sequenceDependencyUsages"],
        json!([{
            "sequence":{"kind":"sequence","schema":"app","name":"orders_id_seq"},
            "ownerTable":{"kind":"table","schema":"app","name":"orders"},
            "columnName":"id",
            "usage":"column_default"
        }])
    );
    assert!(driver.queried_sql()[0].contains("relation_ns.nspname = 'app'"));
}

#[tokio::test]
async fn postgres_sequence_catalog_returns_exact_owned_by_usage() {
    let mut catalog = empty_dependency_catalog();
    catalog.rows.push(vec![
        Some(Value::Integer(1)),
        Some(Value::Integer(0)),
        Some(Value::String("table".into())),
        Some(Value::String("app".into())),
        Some(Value::String("orders".into())),
        None,
        None,
        None,
        Some(Value::String("owned_by".into())),
        Some(Value::String("app".into())),
        Some(Value::String("orders_id_seq".into())),
        Some(Value::String("app".into())),
        Some(Value::String("orders".into())),
        Some(Value::String("id".into())),
    ]);
    let driver = TrackingDriver::for_type("postgresql", [QueryReply::Catalog(catalog)]);

    let result = run_dependency_command_for(
        &driver,
        "postgresql",
        json!({"kind":"sequence","schema":"app","name":"orders_id_seq"}),
    )
    .await;

    assert_eq!(result["complete"], true, "{result}");
    assert_eq!(
        result["sequenceDependencyUsages"],
        json!([{
            "sequence":{"kind":"sequence","schema":"app","name":"orders_id_seq"},
            "ownerTable":{"kind":"table","schema":"app","name":"orders"},
            "columnName":"id",
            "usage":"owned_by"
        }])
    );
    assert!(driver.queried_sql()[0].contains("'owned_by'::text AS sequence_usage"));
}

#[tokio::test]
async fn mysql_view_dependency_catalog_requires_visible_grants_and_empty_udf_catalog() {
    let mut catalog = empty_dependency_catalog();
    catalog.rows[0][2] = Some(Value::String("table".into()));
    catalog.rows[0][3] = Some(Value::String("app".into()));
    catalog.rows[0][4] = Some(Value::String("child".into()));
    let driver = TrackingDriver::scripted([
        QueryReply::Catalog(catalog),
        QueryReply::Grants(vec![
            "GRANT SELECT, EXECUTE ON *.* TO 'migration'@'%'".into()
        ]),
        QueryReply::UdfCount(0),
    ]);

    let result = run_dependency_command(
        &driver,
        json!({"kind":"view","schema":"  ","name":"view_one"}),
    )
    .await;

    assert_eq!(result["complete"], true, "{result}");
    assert_eq!(
        result["dependencies"],
        json!([{"kind":"table","schema":"app","name":"child"}])
    );
    let queries = driver.queried_sql();
    assert_eq!(queries.len(), 3);
    assert!(queries[0].contains("information_schema.VIEW_TABLE_USAGE"));
    assert!(!queries[0].contains("selected.schema_name = ''"));
    assert_eq!(queries[1], "SHOW GRANTS FOR CURRENT_USER()");
    assert_eq!(queries[2], "SELECT COUNT(*) AS udf_count FROM mysql.func");
}
