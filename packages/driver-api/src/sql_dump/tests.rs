use super::parser::relation_already_exists;
use crate::traits::DatabaseDriver;
use crate::types::*;

use super::*;


#[test]
fn build_create_table_sql_includes_pk_and_not_null() {
    let schema = TableSchema {
        table_name: "users".into(),
        columns: vec![
            ColumnSchema {
                name: "id".into(),
                data_type: "integer".into(),
                nullable: false,
                default_value: None,
                comment: None,
                is_primary_key: true,
                is_auto_increment: false,
            },
            ColumnSchema {
                name: "name".into(),
                data_type: "text".into(),
                nullable: true,
                default_value: Some("'anon'".into()),
                comment: None,
                is_primary_key: false,
                is_auto_increment: false,
            },
        ],
        primary_keys: vec!["id".into()],
        indexes: vec![],
        foreign_keys: vec![],
    };
    let sql = build_create_table_sql(&|n| format!("\"{}\"", n), &schema);
    assert!(sql.contains("CREATE TABLE IF NOT EXISTS \"users\""));
    assert!(sql.contains("\"id\" integer NOT NULL"));
    assert!(sql.contains("DEFAULT 'anon'"));
    assert!(sql.contains("PRIMARY KEY (\"id\")"));
    assert!(sql.ends_with(";\n"));
}

#[test]
fn dump_header_single_transaction_flag() {
    let sql = "-- DataZen backup: app\n-- Options: clean, single-transaction\n";
    assert!(dump_header_requests_single_transaction(sql));
    assert!(!dump_header_requests_single_transaction(
        "-- Options: clean\n"
    ));
}

#[test]
fn partition_dump_objects_puts_views_after_tables() {
    let (base, views) = partition_dump_objects(vec![
        TableInfo {
            schema: Some("public".into()),
            name: "active_users".into(),
            table_type: TableType::View,
            row_count: None,
        },
        TableInfo {
            schema: Some("public".into()),
            name: "orders".into(),
            table_type: TableType::Table,
            row_count: None,
        },
    ]);
    assert_eq!(
        base.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["orders"]
    );
    assert_eq!(
        views.iter().map(|t| t.name.as_str()).collect::<Vec<_>>(),
        ["active_users"]
    );
}

#[test]
fn view_like_skips_insert_and_uses_drop_view() {
    assert!(is_view_like(&TableType::View));
    assert!(is_view_like(&TableType::MaterializedView));
    assert!(!is_view_like(&TableType::Table));
    assert_eq!(
        drop_object_sql(
            &|n| format!("\"{n}\""),
            "active_users",
            &TableType::View,
            None
        ),
        "DROP VIEW IF EXISTS \"active_users\";\n"
    );
    assert_eq!(
        drop_object_sql(
            &|n| format!("\"{n}\""),
            "users",
            &TableType::Table,
            Some("public"),
        ),
        "DROP TABLE IF EXISTS \"public\".\"users\";\n"
    );
}

#[test]
fn restore_statement_label_truncates_and_skips_comments() {
    assert_eq!(
        restore_statement_label("CREATE TABLE users (id int)"),
        "CREATE TABLE users (id int)"
    );
    assert_eq!(
        restore_statement_label("-- comment\nINSERT INTO t VALUES (1)"),
        "INSERT INTO t VALUES (1)"
    );
    let long = format!("INSERT INTO t VALUES ({})", "x".repeat(250));
    let label = restore_statement_label(&long);
    assert!(label.ends_with('…'));
    assert!(label.chars().count() <= 201);
}

#[test]
fn created_relation_ident_reads_schema_qualified_create() {
    assert_eq!(
        created_relation_ident("CREATE TABLE \"public\".\"users\" (\n  id int\n)"),
        Some("\"public\".\"users\"".into())
    );
    assert_eq!(
        created_relation_ident("CREATE OR REPLACE VIEW active_users AS SELECT 1"),
        Some("active_users".into())
    );
    assert_eq!(
        created_relation_ident("CREATE TABLE IF NOT EXISTS foo (id int)"),
        Some("foo".into())
    );
    assert_eq!(created_relation_ident("INSERT INTO t VALUES (1)"), None);
}

#[test]
fn relation_already_exists_detects_pg_and_mysql() {
    assert!(relation_already_exists(
        "error returned from database: 42P07 duplicate_table: relation \"users\" already exists"
    ));
    assert!(relation_already_exists(
        "1050 (42S01): Table 'users' already exists"
    ));
    assert!(!relation_already_exists("syntax error"));
}

/// Minimal driver whose table listing is configurable; every other method
/// keeps the default trait behaviour the dump pipeline relies on.
struct TableListingDriver {
    tables: Vec<TableInfo>,
}

impl TableListingDriver {
    fn new(tables: Vec<TableInfo>) -> Self {
        Self { tables }
    }
}

const LISTING_HANDLE: std::sync::LazyLock<ConnectionHandle> =
    std::sync::LazyLock::new(|| ConnectionHandle {
        id: "conn".into(),
        pool_id: "pool".into(),
    });

#[async_trait::async_trait]
impl DatabaseDriver for TableListingDriver {
    fn driver_type(&self) -> DatabaseType {
        "listing".into()
    }

    async fn connect(
        &self,
        _config: &ConnectionConfig,
    ) -> Result<ConnectionHandle, DriverError> {
        Ok(LISTING_HANDLE.clone())
    }

    async fn test_connection(
        &self,
        _config: &ConnectionConfig,
    ) -> Result<ServerInfo, DriverError> {
        Ok(ServerInfo {
            server_version: String::new(),
            server_type: self.driver_type(),
        })
    }

    async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }

    async fn get_databases(
        &self,
        _handle: &ConnectionHandle,
    ) -> Result<Vec<String>, DriverError> {
        Ok(vec![])
    }

    async fn get_tables(
        &self,
        _handle: &ConnectionHandle,
        _database: &str,
    ) -> Result<Vec<TableInfo>, DriverError> {
        Ok(self.tables.clone())
    }

    async fn get_table_schema(
        &self,
        _handle: &ConnectionHandle,
        table: &str,
    ) -> Result<TableSchema, DriverError> {
        // Mirrors the real drivers: the requested name is echoed back
        // verbatim into `table_name` (a blank name would flow straight
        // into `build_create_table_sql`).
        Ok(TableSchema {
            table_name: table.to_string(),
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
            indexes: vec![],
            foreign_keys: vec![],
        })
    }

    async fn query(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<QueryResult, DriverError> {
        Ok(QueryResult {
            columns: vec![],
            rows: vec![],
            rows_affected: Some(0),
            execution_time_ms: 0,
        })
    }

    async fn query_multi(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
        _limit: Option<u32>,
    ) -> Result<MultiQueryResult, DriverError> {
        Ok(MultiQueryResult {
            results: vec![],
            total_time_ms: 0,
        })
    }

    async fn query_with_params(
        &self,
        handle: &ConnectionHandle,
        sql: &str,
        _params: &[Value],
    ) -> Result<QueryResult, DriverError> {
        self.query(handle, sql).await
    }

    async fn execute(
        &self,
        _handle: &ConnectionHandle,
        _sql: &str,
    ) -> Result<u64, DriverError> {
        Ok(0)
    }

    async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
        Ok(())
    }
}

fn listing_table(name: &str) -> TableInfo {
    TableInfo {
        schema: Some("public".into()),
        name: name.into(),
        table_type: TableType::Table,
        row_count: None,
    }
}

/// R2-BUG-001: PostgreSQL reports one navigation-only marker row with a
/// **blank** name per empty schema (`SystemTable`); the dump pipeline used
/// to turn each marker into `-- Table:` + `CREATE TABLE IF NOT EXISTS ""`,
/// which restore then rejects (`zero-length delimited identifier`).
#[tokio::test]
async fn dump_skips_blank_identifier_marker_rows() {
    let driver = TableListingDriver::new(vec![
        listing_table("orders"),
        TableInfo {
            schema: Some("e2e_sch_empty".into()),
            name: String::new(),
            table_type: TableType::SystemTable,
            row_count: None,
        },
    ]);
    let opts = BackupDumpOptions {
        clean: true,
        ..Default::default()
    };

    let out = dump_sql_database(&driver, &LISTING_HANDLE, "app", &opts)
        .await
        .unwrap();

    // The real table is still dumped completely.
    assert!(out.contains("-- Table: orders"), "{out}");
    assert!(
        out.contains("CREATE TABLE IF NOT EXISTS \"orders\""),
        "{out}"
    );
    // No per-object failure comments may appear for skipped rows.
    assert!(!out.contains("-- Error dumping data for"), "{out}");
    // Blank identifiers must never reach statement generation — neither as
    // CREATE, nor DROP (clean), nor data SELECT, nor progress comments.
    assert!(!out.contains("\"\""), "blank identifier in dump: {out}");
    assert!(!out.contains("-- Table: \n"), "{out}");
    assert!(!out.contains("-- View: \n"), "{out}");
}

/// System-table entries carry catalog objects, never business data; they
/// must not enter the dump either (MySQL `SYSTEM VIEW`, PG markers).
#[tokio::test]
async fn dump_skips_system_table_entries() {
    let driver = TableListingDriver::new(vec![
        listing_table("orders"),
        TableInfo {
            schema: None,
            name: "pg_catalog_overview".into(),
            table_type: TableType::SystemTable,
            row_count: None,
        },
    ]);

    let out = dump_sql_database(
        &driver,
        &LISTING_HANDLE,
        "app",
        &BackupDumpOptions::default(),
    )
    .await
    .unwrap();

    assert!(out.contains("-- Table: orders"), "{out}");
    assert!(
        !out.contains("pg_catalog_overview"),
        "system-table entry leaked into dump: {out}"
    );
}

#[test]
fn extract_nextval_sequence_names_from_create_table() {
    let sql = r#"CREATE TABLE public.categories (
  "id" integer NOT NULL DEFAULT nextval('categories_id_seq'::regclass)
)"#;
    assert_eq!(extract_nextval_sequence_names(sql), ["categories_id_seq"]);
    assert_eq!(
        extract_nextval_sequence_names(r#"DEFAULT nextval('public.orders_id_seq'::regclass)"#),
        ["public.orders_id_seq"]
    );
    assert!(extract_nextval_sequence_names("CREATE TABLE t (id int)").is_empty());
    assert_eq!(
        quote_sequence_ident("categories_id_seq"),
        "\"categories_id_seq\""
    );
    assert_eq!(
        quote_sequence_ident("public.categories_id_seq"),
        "\"public\".\"categories_id_seq\""
    );
}

#[test]
fn append_batched_inserts_groups_rows_and_respects_limits() {
    let rows: Vec<String> = (1..=5).map(|i| format!("({i})")).collect();
    let mut out = String::new();
    append_batched_inserts(&mut out, "t", "id", &rows, 2, 10_000);
    assert_eq!(
        out,
        "INSERT INTO t (id) VALUES (1), (2);\nINSERT INTO t (id) VALUES (3), (4);\nINSERT INTO t (id) VALUES (5);\n"
    );

    let mut small = String::new();
    append_batched_inserts(
        &mut small,
        "t",
        "id",
        &["(1)".into(), "(2)".into(), "(3)".into()],
        100,
        40,
    );
    assert!(small.matches("INSERT INTO").count() >= 2);
    assert!(small.contains("VALUES (1)"));
}

#[tokio::test]
async fn restore_feed_rejects_write_sql_when_guard_read_only() {
    use crate::traits::DatabaseDriver;
    use async_trait::async_trait;

    struct StubDriver;

    #[async_trait]
    impl DatabaseDriver for StubDriver {
        fn driver_type(&self) -> DatabaseType {
            "stub".into()
        }

        async fn connect(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ConnectionHandle, DriverError> {
            Ok(ConnectionHandle {
                id: "conn".into(),
                pool_id: "pool".into(),
            })
        }

        async fn test_connection(
            &self,
            _config: &ConnectionConfig,
        ) -> Result<ServerInfo, DriverError> {
            Ok(ServerInfo {
                server_version: String::new(),
                server_type: self.driver_type(),
            })
        }

        async fn disconnect(&self, _handle: ConnectionHandle) -> Result<(), DriverError> {
            Ok(())
        }

        async fn get_databases(
            &self,
            _handle: &ConnectionHandle,
        ) -> Result<Vec<String>, DriverError> {
            Ok(vec![])
        }

        async fn get_tables(
            &self,
            _handle: &ConnectionHandle,
            _database: &str,
        ) -> Result<Vec<TableInfo>, DriverError> {
            Ok(vec![])
        }

        async fn get_table_schema(
            &self,
            _handle: &ConnectionHandle,
            _table: &str,
        ) -> Result<TableSchema, DriverError> {
            Ok(TableSchema {
                table_name: String::new(),
                columns: vec![],
                primary_keys: vec![],
                indexes: vec![],
                foreign_keys: vec![],
            })
        }

        async fn query(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
        ) -> Result<QueryResult, DriverError> {
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                execution_time_ms: 0,
            })
        }

        async fn query_multi(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
            _limit: Option<u32>,
        ) -> Result<MultiQueryResult, DriverError> {
            Ok(MultiQueryResult {
                results: vec![],
                total_time_ms: 0,
            })
        }

        async fn query_with_params(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
            _params: &[Value],
        ) -> Result<QueryResult, DriverError> {
            Ok(QueryResult {
                columns: vec![],
                rows: vec![],
                rows_affected: None,
                execution_time_ms: 0,
            })
        }

        async fn execute(
            &self,
            _handle: &ConnectionHandle,
            _sql: &str,
        ) -> Result<u64, DriverError> {
            Ok(0)
        }

        async fn cancel_query(&self, _handle: &ConnectionHandle) -> Result<(), DriverError> {
            Ok(())
        }
    }

    let driver = StubDriver;
    let handle = ConnectionHandle {
        id: "conn".into(),
        pool_id: "pool".into(),
    };
    let guard = Box::new(|stmt: &str| {
        let upper = stmt.trim_start().to_ascii_uppercase();
        if upper.starts_with("INSERT")
            || upper.starts_with("UPDATE")
            || upper.starts_with("DELETE")
            || upper.starts_with("CREATE")
            || upper.starts_with("DROP")
        {
            Err(DriverError::QueryFailed(
                "Connection is read-only; write statements are not allowed".into(),
            ))
        } else {
            Ok(())
        }
    });
    let mut session = RestoreSession::new(&driver, &handle, SqlStatementScanner::new(), None)
        .with_statement_guard(guard);
    let err = session
        .feed("SELECT 1;\nINSERT INTO t VALUES (1);", &mut |_| {})
        .await
        .unwrap_err();
    assert!(
        matches!(err, DriverError::QueryFailed(ref msg) if msg.contains("read-only")),
        "{err:?}"
    );
}
