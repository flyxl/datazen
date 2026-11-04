//! SQL Server unit tests for the driver type, its SQL text and its session
//! behaviour. The module path is unchanged (`sqlserver::tests`), so every test
//! name stays byte-identical to the pre-split inventory.

use super::dialect::{
    apply_sqlserver_top, has_top_level_offset, needs_own_batch, parse_physical_database_identity,
    parse_schema_scope_identity, schema_migration_blockers_for_indexes, split_statements,
    PHYSICAL_DATABASE_IDENTITY_SQL,
};
use super::*;

fn config(port: Option<u16>) -> ConnectionConfig {
    serde_json::from_value(serde_json::json!({
        "id": "cfg-1",
        "name": "sqlserver",
        "databaseType": "sqlserver",
        "host": "db-a.example.com",
        "username": "sa",
        "database": "app",
    }))
    .map(|mut c: ConnectionConfig| {
        c.port = port;
        c
    })
    .expect("a minimal config deserializes")
}

/// 反漂移闸：`default_port()` 是宿主做端点物理身份摘要时唯一的「默认端口」
/// 来源。SQL Server 没有隐式 host（`build_config` 直接报 `host is required`），
/// 所以这条闸只需要端口；它一旦与 `dial_addr` 实际拨号的端口分家，省略
/// port 的连接与显式写全 1433 的连接就会算出两个不同的 service_key，
/// 自覆盖在 admission 静默漏判。这里不测常量本身，只测「声明 == 实拨」。
#[test]
fn the_declared_default_port_is_exactly_what_connect_dials() {
    let driver = SqlServerDriver::new();
    let declared = driver
        .default_port()
        .expect("SQL Server has an implicit port");
    assert_eq!(declared, DEFAULT_PORT);
    assert_eq!(
        driver.default_host(),
        None,
        "SQL Server requires an explicit host, so there is no implicit one"
    );

    assert_eq!(
        SqlServerDriver::dial_addr(&config(None)).expect("host present"),
        format!("db-a.example.com:{declared}"),
        "default_port() must be the port connect actually dials"
    );
    assert_eq!(
        SqlServerDriver::dial_addr(&config(Some(1440))).expect("host present"),
        "db-a.example.com:1440",
        "an explicit port must win over the declared default"
    );
    let mut hostless = config(None);
    hostless.host = None;
    assert!(SqlServerDriver::dial_addr(&hostless).is_err());

    // 协议层（tiberius）自己的默认端口也要与声明一致，否则 TCP 拨号与
    // 登录包会指向两个不同的端口。
    assert_eq!(
        SqlServerDriver::build_config(&config(None))
            .expect("build_config")
            .get_addr(),
        format!("db-a.example.com:{declared}"),
        "the login packet's own default port must agree with the declared one"
    );
}

#[test]
fn transaction_count_accepts_native_integer_values() {
    assert_eq!(
        SqlServerDriver::parse_transaction_count(&Value::Integer(0)),
        Some(0)
    );
    assert_eq!(
        SqlServerDriver::parse_transaction_count(&Value::Integer(2)),
        Some(2)
    );
    assert_eq!(
        SqlServerDriver::parse_transaction_count(&Value::String("3".into())),
        Some(3)
    );
    assert_eq!(
        SqlServerDriver::parse_transaction_count(&Value::String("Integer(0)".into())),
        None
    );
    assert_eq!(SqlServerDriver::parse_transaction_count(&Value::Null), None);
}

#[test]
fn physical_database_identity_uses_server_name_and_current_catalog_database_id() {
    assert!(PHYSICAL_DATABASE_IDENTITY_SQL.contains("SERVERPROPERTY('ServerName')"));
    assert!(PHYSICAL_DATABASE_IDENTITY_SQL.contains("d.database_id AS database_id"));
    assert!(PHYSICAL_DATABASE_IDENTITY_SQL.contains("FROM sys.databases AS d"));
    assert!(PHYSICAL_DATABASE_IDENTITY_SQL.contains("d.name = DB_NAME()"));
    assert!(PHYSICAL_DATABASE_IDENTITY_SQL.contains("DB_ID(NULLIF(@P1, N'')) = DB_ID()"));
    assert!(!PHYSICAL_DATABASE_IDENTITY_SQL.contains("password"));

    let result = QueryResult {
        columns: Vec::new(),
        rows: vec![vec![
            Some(Value::String("sqlserver-prod\\instance1".into())),
            Some(Value::Integer(23)),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    assert_eq!(
        parse_physical_database_identity(&result).as_deref(),
        Some("sqlserver:server:24:sqlserver-prod\\instance1:database:23")
    );

    let unknown = QueryResult {
        columns: Vec::new(),
        rows: vec![vec![
            Some(Value::String("server".into())),
            Some(Value::Null),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    assert_eq!(parse_physical_database_identity(&unknown), None);

    let no_server_name = QueryResult {
        columns: Vec::new(),
        rows: vec![vec![
            Some(Value::String(" ".into())),
            Some(Value::Integer(23)),
        ]],
        rows_affected: None,
        execution_time_ms: 0,
    };
    assert_eq!(parse_physical_database_identity(&no_server_name), None);

    let ambiguous = QueryResult {
        columns: Vec::new(),
        rows: vec![
            vec![
                Some(Value::String("server-a".into())),
                Some(Value::Integer(23)),
            ],
            vec![
                Some(Value::String("server-b".into())),
                Some(Value::Integer(23)),
            ],
        ],
        rows_affected: None,
        execution_time_ms: 0,
    };
    assert_eq!(parse_physical_database_identity(&ambiguous), None);
}

#[test]
fn schema_scope_identity_requires_one_positive_catalog_id() {
    let identity = |rows| QueryResult {
        columns: Vec::new(),
        rows,
        rows_affected: None,
        execution_time_ms: 0,
    };

    assert_eq!(
        parse_schema_scope_identity(&identity(vec![vec![Some(Value::Integer(5))]])).as_deref(),
        Some("sqlserver:schema-id:5")
    );
    assert_eq!(
        parse_schema_scope_identity(&identity(vec![vec![Some(Value::String("7".into()))]]))
            .as_deref(),
        Some("sqlserver:schema-id:7")
    );
    assert_eq!(parse_schema_scope_identity(&identity(Vec::new())), None);
    assert_eq!(
        parse_schema_scope_identity(&identity(vec![
            vec![Some(Value::Integer(1))],
            vec![Some(Value::Integer(2))]
        ])),
        None
    );
    assert_eq!(
        parse_schema_scope_identity(&identity(vec![vec![Some(Value::Integer(0))]])),
        None
    );
    assert_eq!(
        parse_schema_scope_identity(&identity(vec![vec![Some(Value::Null)]])),
        None
    );
}

#[test]
fn schema_migration_blockers_cover_unrepresented_clustered_layouts() {
    let ordinary_layout = vec![
        IndexInfo {
            name: "PK_t".into(),
            columns: vec!["id".into()],
            is_unique: true,
            is_primary: true,
            index_type: "CLUSTERED".into(),
        },
        IndexInfo {
            name: "IX_t_value".into(),
            columns: vec!["value".into()],
            is_unique: false,
            is_primary: false,
            index_type: "NONCLUSTERED".into(),
        },
    ];
    assert!(schema_migration_blockers_for_indexes(&ordinary_layout).is_empty());

    let nonclustered_primary = vec![IndexInfo {
        index_type: "NONCLUSTERED".into(),
        ..ordinary_layout[0].clone()
    }];
    assert!(schema_migration_blockers_for_indexes(&nonclustered_primary)
        .iter()
        .any(|blocker| blocker.contains("primary-key clustering is nonclustered")));

    let clustered_secondary = vec![IndexInfo {
        name: "UQ_t_value".into(),
        columns: vec!["value".into()],
        is_unique: true,
        is_primary: false,
        index_type: "UNIQUE_CONSTRAINT:CLUSTERED".into(),
    }];
    assert!(schema_migration_blockers_for_indexes(&clustered_secondary)
        .iter()
        .any(|blocker| blocker.contains("clustered secondary index")));
}

#[test]
fn ssl_disable_is_plaintext() {
    assert_eq!(
        SqlServerDriver::ssl_settings(&SslMode::Disable),
        (EncryptionLevel::NotSupported, false)
    );
}

#[test]
fn ssl_require_trusts_cert() {
    assert_eq!(
        SqlServerDriver::ssl_settings(&SslMode::Require),
        (EncryptionLevel::Required, true)
    );
}

#[test]
fn ssl_verify_full_requires_encryption_without_trust() {
    assert_eq!(
        SqlServerDriver::ssl_settings(&SslMode::VerifyFull),
        (EncryptionLevel::Required, false)
    );
}

#[test]
fn sqlserver_declares_a_schema_level_with_dbo_default() {
    let driver = SqlServerDriver::new();
    assert!(driver.has_schema_level());
    assert_eq!(driver.default_schema(), Some("dbo"));
}

#[test]
fn effective_schema_prefers_the_argument_then_the_convention() {
    let driver = SqlServerDriver::new();
    assert_eq!(driver.effective_schema(Some("sales")), Some("sales"));
    assert_eq!(driver.effective_schema(Some("  sales ")), Some("sales"));
    assert_eq!(driver.effective_schema(Some("   ")), Some("dbo"));
    assert_eq!(driver.effective_schema(None), Some("dbo"));
}

#[test]
fn exact_schema_reads_require_an_explicit_schema() {
    let driver = SqlServerDriver::new();
    assert!(validate_schema_target(&driver, "app", Some("dbo"), SchemaScope::ExactSchema).is_ok());
    // Replaces the old `use_database` session-switch coverage: the target is
    // now the explicit argument, and a missing schema is a hard error
    // instead of silently landing on whatever the session pointed at.
    assert!(
        validate_schema_target(&driver, "app", None, SchemaScope::ExactSchema).is_err(),
        "a schema-aware driver must reject an exact-schema read without a schema"
    );
    assert!(validate_schema_target(&driver, "app", None, SchemaScope::AnySchema).is_ok());
}

#[test]
fn build_table_schema_sql_filters_schema_and_qualifies_catalog() {
    let sql = SqlServerDriver::build_table_schema_sql("sales", "dbo", "users");
    assert!(sql.contains("FROM [sales].sys.columns c"));
    assert!(sql.contains("s.name = @P1 AND o.name = @P2"));
    assert!(sql.contains("is_primary_key = 1"));
    assert!(sql.contains("is_pk"));
    assert!(sql.contains("pk.key_ordinal"));
    assert!(sql.contains("c.max_length"));
    // No session switch may be embedded in a read path.
    assert!(!sql.to_uppercase().contains("USE ["));
}

#[test]
fn build_table_schema_sql_stays_local_when_database_is_blank() {
    let sql = SqlServerDriver::build_table_schema_sql("", "dbo", "users");
    assert!(sql.contains("FROM sys.columns c"));
    assert!(!sql.contains("[]."));
}

#[test]
fn build_table_schema_sql_escapes_quotes() {
    let sql = SqlServerDriver::build_table_schema_sql("db]", "d'bo", "us'ers");
    assert!(sql.contains("FROM [db]]].sys.columns c"));
    assert!(sql.contains("@P1"));
    assert!(sql.contains("@P2"));
    assert!(!sql.contains("d'bo"));
    assert!(!sql.contains("us'ers"));
}

#[test]
fn build_tables_sql_populates_schema_and_optionally_filters() {
    let all = SqlServerDriver::build_tables_sql("sales", None);
    assert!(all.contains("JOIN [sales].sys.schemas s"));
    assert!(all.contains("s.name AS schema_name"));
    assert!(!all.contains("WHERE s.name"));

    let filtered = SqlServerDriver::build_tables_sql("sales", Some("dbo"));
    assert!(filtered.contains("WHERE s.name = 'dbo'"));
    assert_eq!(filtered.matches("WHERE s.name = 'dbo'").count(), 2);
}

#[test]
fn build_all_columns_sql_filters_schema_and_qualifies_catalog() {
    let sql = SqlServerDriver::build_all_columns_sql("sales", Some("dbo"));
    assert!(sql.contains("FROM [sales].sys.columns c"));
    assert!(sql.contains("AND s.name = 'dbo'"));
    assert!(sql.contains("s.name AS schema_name"));
}

#[test]
fn apply_sqlserver_top_inserts_plus_one() {
    assert_eq!(
        apply_sqlserver_top("SELECT * FROM t", None),
        ("SELECT * FROM t".into(), None)
    );
    assert_eq!(
        apply_sqlserver_top("SELECT * FROM t", Some(10)),
        ("SELECT TOP 11 * FROM t".into(), Some(10))
    );
    // A hand-written `TOP` is the dialect's own row limit: respected, and
    // no cap is reported as applied.
    assert_eq!(
        apply_sqlserver_top("SELECT TOP 5 * FROM t", Some(10)),
        ("SELECT TOP 5 * FROM t".into(), None)
    );
    assert_eq!(
        apply_sqlserver_top("SELECT DISTINCT name FROM t", Some(3)),
        ("SELECT DISTINCT TOP 4 name FROM t".into(), Some(3))
    );
    assert_eq!(
        apply_sqlserver_top("SELECT DISTINCT TOP 2 name FROM t", Some(3)),
        ("SELECT DISTINCT TOP 2 name FROM t".into(), None)
    );
    assert_eq!(
        apply_sqlserver_top("INSERT INTO t VALUES (1)", Some(10)),
        ("INSERT INTO t VALUES (1)".into(), None)
    );
    assert_eq!(
        apply_sqlserver_top("select id from t", Some(1)),
        ("SELECT TOP 2 id from t".into(), Some(1))
    );

    // `TOP` cannot share a query with `OFFSET … FETCH` (error 10741), so a
    // statement that pages itself is never rewritten — this is the statement
    // the Visual Query Builder emits for SQL Server.
    let paged = "SELECT [u].[id] FROM [users] AS [u] \
                 ORDER BY (SELECT NULL) OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY";
    assert_eq!(apply_sqlserver_top(paged, Some(100)), (paged.into(), None));
    // The same is true for an offset-only page and for either keyword case.
    assert_eq!(
        apply_sqlserver_top("select id from t order by id offset 2 rows", Some(10)),
        ("select id from t order by id offset 2 rows".into(), None)
    );
    assert_eq!(
        apply_sqlserver_top("SELECT id FROM t OFFSET @skip ROWS", Some(10)),
        ("SELECT id FROM t OFFSET @skip ROWS".into(), None)
    );

    // A nested `OFFSET` is legal next to an outer `TOP`, so the cap stays.
    assert_eq!(
        apply_sqlserver_top(
            "SELECT * FROM (SELECT id FROM t ORDER BY id OFFSET 2 ROWS) AS inner_q",
            Some(10)
        ),
        (
            "SELECT TOP 11 * FROM (SELECT id FROM t ORDER BY id OFFSET 2 ROWS) AS inner_q".into(),
            Some(10)
        )
    );
    // …and a column named `offset` is not an OFFSET clause.
    assert_eq!(
        apply_sqlserver_top("SELECT offset FROM t", Some(10)),
        ("SELECT TOP 11 offset FROM t".into(), Some(10))
    );
}

#[test]
fn has_top_level_offset_ignores_literals_comments_and_identifiers() {
    assert!(has_top_level_offset(
        "SELECT id FROM t ORDER BY id OFFSET 5 ROWS FETCH NEXT 10 ROWS ONLY"
    ));
    assert!(has_top_level_offset("SELECT id FROM t OFFSET 5 ROWS"));

    // Inside a string literal, a bracketed/quoted identifier or a comment it
    // is data, not a clause.
    assert!(!has_top_level_offset(
        "SELECT 'x OFFSET 5 ROWS' AS [offset 3] FROM t"
    ));
    assert!(!has_top_level_offset("SELECT id FROM t -- OFFSET 5 ROWS\n"));
    assert!(!has_top_level_offset(
        "SELECT id /* OFFSET 5 ROWS */ FROM t"
    ));
    assert!(!has_top_level_offset("\"OFFSET 5\" AS c FROM t"));
    // A doubled bracket inside an identifier must not end it early.
    assert!(!has_top_level_offset(
        "SELECT [we]]ird OFFSET 5 ROWS] FROM t"
    ));
    // Depth matters: a sub-query's OFFSET is not the outer query's.
    assert!(!has_top_level_offset(
        "SELECT * FROM (SELECT id FROM t OFFSET 5 ROWS) AS q"
    ));
    assert!(has_top_level_offset(
        "SELECT * FROM (SELECT id FROM t) AS q OFFSET 1 ROWS"
    ));
}

/// Regression: the host used to append `LIMIT n OFFSET m`, which T-SQL
/// rejects with "Incorrect syntax near 'LIMIT'".
#[test]
fn pagination_syntax_is_offset_fetch_and_never_limit() {
    let driver = SqlServerDriver::new();
    assert!(driver.supports_offset());

    let first_page = driver.pagination_syntax(25, 0);
    assert_eq!(
        first_page.clause,
        "OFFSET 0 ROWS FETCH NEXT 25 ROWS ONLY".to_string()
    );
    assert!(first_page.requires_order_by);
    assert_eq!(first_page.order_by_fallback, Some("(SELECT NULL)"));
    assert!(!first_page.clause.contains("LIMIT"));

    let deep_page = driver.pagination_syntax(50, 150);
    assert_eq!(
        deep_page.clause,
        "OFFSET 150 ROWS FETCH NEXT 50 ROWS ONLY".to_string()
    );
    assert!(!deep_page.clause.contains("LIMIT"));
}

#[test]
fn batch_only_ddl_is_detected() {
    // Statements SQL Server rejects through `sp_executesql`.
    for stmt in [
        "CREATE SCHEMA [reporting]",
        "create schema reporting",
        "CREATE VIEW [dbo].[v] AS SELECT 1 AS c",
        "ALTER VIEW [dbo].[v] AS SELECT 1 AS c",
        "CREATE PROCEDURE [dbo].[p] AS SELECT 1",
        "CREATE PROC [dbo].[p] AS SELECT 1",
        "CREATE FUNCTION [dbo].[f]() RETURNS INT AS BEGIN RETURN 1 END",
        "CREATE TRIGGER [dbo].[tr] ON [dbo].[t] AFTER INSERT AS SELECT 1",
        "CREATE OR ALTER PROCEDURE [dbo].[p] AS SELECT 1",
        "CREATE OR ALTER VIEW [dbo].[v] AS SELECT 1 AS c",
        "  -- installs the reporting schema\nCREATE SCHEMA [reporting]",
        "/* bootstrap */ CREATE TRIGGER [dbo].[tr] ON [dbo].[t] AFTER INSERT AS SELECT 1",
        // Session-scoped statements must also bypass sp_executesql.
        "SET NOCOUNT ON",
        "SET IDENTITY_INSERT [dbo].[t] ON",
        "BEGIN TRAN; SELECT 1; COMMIT",
        "BEGIN TRANSACTION",
        "COMMIT",
        "ROLLBACK",
        "SAVE TRAN savepoint_one",
    ] {
        assert!(needs_own_batch(stmt), "expected own batch: {stmt}");
    }
}

#[test]
fn statement_splitting_respects_literals_and_comments() {
    assert_eq!(
        split_statements("SELECT ';' AS [a]"),
        vec!["SELECT ';' AS [a]"]
    );
    assert_eq!(
        split_statements("SELECT 1; SELECT ';' AS [b]; -- trailing\n"),
        vec!["SELECT 1", "SELECT ';' AS [b]"]
    );
    assert_eq!(
        split_statements("SELECT '[;]' AS [c] /* ; */; SELECT 2"),
        vec!["SELECT '[;]' AS [c] /* ; */", "SELECT 2"]
    );
    assert!(split_statements("   ").is_empty());
}

#[test]
fn statement_splitting_keeps_routine_bodies_in_one_batch() {
    let function = "CREATE FUNCTION [dbo].[normalize] (@value NVARCHAR(64))\n\
                    RETURNS NVARCHAR(64)\n\
                    AS\n\
                    BEGIN\n\
                        RETURN UPPER(LTRIM(RTRIM(@value)));\n\
                    END;";
    assert_eq!(split_statements(function), vec![function]);

    let procedure = "/* migration */ CREATE OR ALTER PROCEDURE [dbo].[p] AS\n\
                     BEGIN SELECT 1; END;";
    assert_eq!(split_statements(procedure), vec![procedure]);
}

#[test]
fn preparable_statements_stay_on_the_rpc_path() {
    for stmt in [
        "CREATE TABLE [dbo].[t] ([id] INT NOT NULL)",
        "ALTER TABLE [dbo].[t] ADD [c] INT NULL",
        "DROP TABLE [dbo].[t]",
        "DROP SCHEMA [reporting]",
        "CREATE SEQUENCE [dbo].[s] AS INT START WITH 1",
        "CREATE TYPE [dbo].[ty] FROM INT",
        "INSERT INTO [dbo].[t] ([id]) VALUES (1)",
        "UPDATE [dbo].[t] SET [id] = 2",
        "DELETE FROM [dbo].[t]",
        "MERGE [dbo].[t] AS t USING [dbo].[s] AS s ON t.id = s.id WHEN MATCHED THEN DELETE;",
        "SELECT * FROM [dbo].[t]",
        "",
    ] {
        assert!(!needs_own_batch(stmt), "expected RPC path: {stmt}");
    }
}

/// Render a temporal cell and unwrap it to its text payload.
fn temporal_text(data: &ColumnData<'static>) -> Option<String> {
    match SqlServerDriver::value_from_column(data) {
        Some(Value::String(text)) => Some(text),
        None => None,
        other => panic!("expected a text value, got {other:?}"),
    }
}

#[test]
fn temporal_columns_render_as_text_not_debug_output() {
    use tiberius::time::{Date, DateTime2, DateTimeOffset, Time};

    // 2026-01-02 is 739617 days after 0001-01-01; 03:04:05 is 11 045 s.
    let date = temporal_text(&ColumnData::Date(Some(Date::new(739_617))));
    assert_eq!(date.as_deref(), Some("2026-01-02"));

    let time = temporal_text(&ColumnData::Time(Some(Time::new(110_450_000_000, 7))));
    assert_eq!(time.as_deref(), Some("03:04:05"));

    let datetime2 = temporal_text(&ColumnData::DateTime2(Some(DateTime2::new(
        Date::new(739_617),
        Time::new(110_450_000_000, 7),
    ))));
    assert_eq!(datetime2.as_deref(), Some("2026-01-02 03:04:05"));

    // TDS carries the UTC instant plus the original offset: the live wire
    // value for `CAST('2026-01-02T03:04:05+08:00' AS DATETIMEOFFSET)` is the
    // datetime2 `2026-01-01 19:04:05` (day 739616, 68 645 s) with offset 480.
    let offset = temporal_text(&ColumnData::DateTimeOffset(Some(DateTimeOffset::new(
        DateTime2::new(Date::new(739_616), Time::new(686_450_000_000, 7)),
        480,
    ))));
    assert_eq!(offset.as_deref(), Some("2026-01-02T03:04:05+08:00"));

    for text in [date, time, datetime2, offset].into_iter().flatten() {
        assert!(
            !text.contains("Date(") && !text.contains("Time {") && !text.contains("increments"),
            "temporal value must not leak tiberius' Debug output: {text}"
        );
    }
}

#[test]
fn null_temporal_columns_stay_null() {
    assert_eq!(temporal_text(&ColumnData::Date(None)), None);
    assert_eq!(temporal_text(&ColumnData::DateTimeOffset(None)), None);
}

#[test]
fn reports_sql_server_bound_parameter_limit() {
    assert_eq!(SqlServerDriver::new().max_bound_parameters(), 2100);
}

#[test]
fn identity_insert_relation_quotes_every_sql_server_identifier() {
    assert_eq!(
        SqlServerDriver::identity_insert_relation("data]zen", Some("odd.schema"), "order]details")
            .unwrap(),
        "[data]]zen].[odd.schema].[order]]details]"
    );
    assert_eq!(
        SqlServerDriver::identity_insert_relation("", None, "items").unwrap(),
        "[dbo].[items]"
    );
    assert!(SqlServerDriver::identity_insert_relation("db", Some(""), "").is_err());
}

#[test]
fn sql_file_identity_wrapper_checks_catalog_and_cleans_up_on_error() {
    let script = SqlServerDriver::render_sql_file_identity_insert(
        "INSERT INTO [odd.schema].[order]]details] ([id]) VALUES (1)",
        "[data]]zen].[odd.schema].[order]]details]",
        &["id".into()],
    )
    .unwrap();
    assert!(script.contains("OBJECT_ID(N'[data]]zen].[odd.schema].[order]]details]', N'U')"));
    assert!(script.contains(
        "FROM [data]]zen].sys.identity_columns WHERE object_id = OBJECT_ID(N'[data]]zen].[odd.schema].[order]]details]', N'U') AND name IN (N'id')"
    ));
    assert!(script.contains("SET IDENTITY_INSERT [data]]zen].[odd.schema].[order]]details] ON;"));
    assert!(script.contains("SET IDENTITY_INSERT [data]]zen].[odd.schema].[order]]details] OFF;"));
    assert!(script.contains("BEGIN CATCH\n        BEGIN TRY\n            SET IDENTITY_INSERT"));
    assert!(script.contains("        THROW;"));
    assert!(script.contains("ELSE\nBEGIN\n    INSERT INTO [odd.schema]"));
    let local_script = SqlServerDriver::render_sql_file_identity_insert(
        "INSERT INTO [dbo].[items] ([id]) VALUES (1)",
        "[dbo].[items]",
        &["id".into()],
    )
    .unwrap();
    assert!(local_script.contains("FROM sys.identity_columns WHERE object_id"));
    let apostrophe_script = SqlServerDriver::render_sql_file_identity_insert(
        "INSERT INTO [O'Brien].[items] ([id]) VALUES (1)",
        "[db].[O'Brien].[items]",
        &["user'id".into()],
    )
    .unwrap();
    assert!(apostrophe_script.contains("OBJECT_ID(N'[db].[O''Brien].[items]'"));
    assert!(apostrophe_script.contains("name IN (N'user''id')"));
}

#[test]
fn sql_file_identity_wrapper_only_checks_inserted_mapped_columns() {
    // The first case represents a regular source column mapped to the
    // target identity column. The target metadata predicate discovers
    // that mapped identity at execution time, independently of source
    // auto-increment metadata.
    let mapped_identity = SqlServerDriver::render_sql_file_identity_insert(
        "INSERT INTO [dbo].[items] ([id], [label]) VALUES (7, N'x')",
        "[dbo].[items]",
        &["id".into(), "label".into()],
    )
    .unwrap();
    assert!(mapped_identity.contains("AND name IN (N'id', N'label')"));
    assert!(mapped_identity.contains("SET IDENTITY_INSERT [dbo].[items] ON;"));

    // Here the source identity maps to a regular target column while a
    // different target identity column is omitted from the INSERT. The
    // script's predicate can only match the actual mapped target column,
    // so that unrelated identity cannot enable IDENTITY_INSERT.
    let unrelated_identity = SqlServerDriver::render_sql_file_identity_insert(
        "INSERT INTO [dbo].[items] ([external_id]) VALUES (7)",
        "[dbo].[items]",
        &["external_id".into()],
    )
    .unwrap();
    assert!(unrelated_identity.contains("AND name IN (N'external_id')"));
    assert!(!unrelated_identity.contains("name IN (N'id'"));
    assert!(unrelated_identity.contains("ELSE\nBEGIN\n    INSERT INTO [dbo].[items]"));
}

#[test]
fn sql_file_identity_wrapper_skips_empty_columns_and_rejects_invalid_names() {
    let insert_sql = "INSERT INTO [dbo].[items] DEFAULT VALUES";
    assert_eq!(
        SqlServerDriver::render_sql_file_identity_insert(insert_sql, "[dbo].[items]", &[],)
            .unwrap(),
        insert_sql
    );
    assert!(SqlServerDriver::render_sql_file_identity_insert(
        "INSERT INTO [dbo].[items] ([id]) VALUES (1)",
        "[dbo].[items]",
        &["bad\0name".into()],
    )
    .is_err());
}

#[test]
fn sql_file_identity_wrapper_rejects_unquoted_relation_fragments() {
    assert!(SqlServerDriver::render_sql_file_identity_insert(
        "INSERT INTO t (id) VALUES (1)",
        "dbo.t; DROP TABLE users",
        &["id".into()],
    )
    .is_err());
    assert!(SqlServerDriver::render_sql_file_identity_insert(
        "INSERT INTO [dbo].[t] ([id]) VALUES (1)",
        "[dbo].",
        &["id".into()],
    )
    .is_err());
}

#[test]
fn explicit_identity_insert_uses_a_quoted_session_toggle_statement() {
    let driver = SqlServerDriver::new();
    assert!(driver.explicit_identity_insert_requires_session_toggle());
    assert_eq!(
        SqlServerDriver::identity_insert_statement("db]name", Some("dbo"), "t]able", true).unwrap(),
        "SET IDENTITY_INSERT [db]]name].[dbo].[t]]able] ON"
    );
    assert_eq!(
        SqlServerDriver::identity_insert_statement("db", None, "items", false).unwrap(),
        "SET IDENTITY_INSERT [db].[dbo].[items] OFF"
    );
    assert_eq!(
        driver.transfer_sql_file_begin_transaction(),
        "BEGIN TRANSACTION;"
    );
    assert_eq!(
        driver.transfer_sql_file_commit_transaction(),
        "COMMIT TRANSACTION;"
    );
}

#[tokio::test]
async fn reuse_driver_forwards_identity_session_and_sql_file_hooks() {
    let inner: Arc<dyn DatabaseDriver> = Arc::new(SqlServerDriver::new());
    let driver = ReuseDriver::new(inner, "sqlserver-alias");
    assert!(driver.explicit_identity_insert_requires_session_toggle());
    let script = driver
        .render_transfer_sql_file_identity_insert(
            "INSERT INTO [dbo].[items] ([id]) VALUES (1)",
            "[dbo].[items]",
            &["id".into()],
        )
        .unwrap();
    assert!(script.contains("SET IDENTITY_INSERT [dbo].[items] ON;"));
    assert_eq!(
        driver.transfer_sql_file_begin_transaction(),
        "BEGIN TRANSACTION;"
    );
    assert_eq!(
        driver.transfer_sql_file_commit_transaction(),
        "COMMIT TRANSACTION;"
    );

    let handle = ConnectionHandle {
        id: "missing".into(),
        pool_id: "missing".into(),
    };
    let error = driver
        .set_identity_insert(&handle, "db", Some("dbo"), "items", true)
        .await
        .unwrap_err();
    assert!(matches!(error, DriverError::ConnectionFailed(_)));
    driver.discard_connection(&handle).await.unwrap();
}
