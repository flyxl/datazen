//! T083 audit: the identifierQuoting + index-registry ports, verified
//! against the React sources re-read line by line —
//! `lib/sqlDialects/identifierQuoting.ts`, `lib/sqlDialects/index.ts`,
//! `lib/sqlDialects/extra.ts` (the assembled strategies) and the React
//! test `__tests__/dialects.test.ts` (the registry expectations).
//!
//! Tester-written: the coder's inline suites are treated as a black box;
//! every expectation below is re-derived from the React files.

use datazen_ui::support::sql_dialects as sd;
use datazen_ui::support::sql_dialects::dialect_profile::{
    DeclaredSqlDialectProfile, FoldCase, QuoteStyle, resolve_dialect_profile,
};

// ---- identifierQuoting (the React file's rules, one test each) ---------------

#[test]
fn quoting_rules_follow_react_identifier_quoting() {
    // foldIdentifier is the case fold only.
    assert_eq!(sd::fold_identifier("Users", FoldCase::Lower), "users");
    assert_eq!(sd::fold_identifier("Users", FoldCase::Upper), "USERS");
    assert_eq!(sd::fold_identifier("Users", FoldCase::Preserve), "Users");

    // needsQuoting: empty quotes; 'none' never quotes; non-plain quotes.
    assert!(sd::needs_quoting("", QuoteStyle::Double, FoldCase::Lower, None));
    assert!(!sd::needs_quoting("order", QuoteStyle::None, FoldCase::Lower, None));
    assert!(sd::needs_quoting("with space", QuoteStyle::Double, FoldCase::Lower, None));
    assert!(sd::needs_quoting("9lives", QuoteStyle::Double, FoldCase::Lower, None));
    assert!(sd::needs_quoting("a-b", QuoteStyle::Double, FoldCase::Lower, None));
    // `$` is a plain-identifier character in the React regex.
    assert!(!sd::needs_quoting("c$", QuoteStyle::Double, FoldCase::Lower, None));
    // Reserved words quote (checked through the lowercase).
    assert!(sd::needs_quoting("FROM", QuoteStyle::Double, FoldCase::Lower, None));
    assert!(sd::needs_quoting("from", QuoteStyle::Double, FoldCase::Lower, None));
    assert!(!sd::needs_quoting("fr om", QuoteStyle::None, FoldCase::Lower, None));
    // Extra reserved words only when provided.
    assert!(!sd::needs_quoting("lateral_join", QuoteStyle::Double, FoldCase::Lower, None));
    let extras = vec!["lateral_join".to_string()];
    assert!(sd::needs_quoting("lateral_join", QuoteStyle::Double, FoldCase::Lower, Some(&extras)));
    assert!(!sd::needs_quoting("other", QuoteStyle::Double, FoldCase::Lower, Some(&extras)));
    // Case that folding would change quotes; preserve never does.
    assert!(sd::needs_quoting("Col", QuoteStyle::Double, FoldCase::Lower, None));
    assert!(sd::needs_quoting("col", QuoteStyle::Double, FoldCase::Upper, None));
    assert!(!sd::needs_quoting("Col", QuoteStyle::Double, FoldCase::Preserve, None));

    // wrapQuoted doubles the inner delimiter for double and bracket.
    assert_eq!(sd::wrap_quoted("a\"b", QuoteStyle::Double), "\"a\"\"b\"");
    assert_eq!(sd::wrap_quoted("a]b", QuoteStyle::Bracket), "[a]]b]");
    assert_eq!(sd::wrap_quoted("a`b", QuoteStyle::Backtick), "`a`b`");
    // quoteIdentifierForDialect honours 'none' and the profile's style.
    let profile = resolve_dialect_profile("mysql", None);
    assert_eq!(sd::quote_identifier_for_dialect("users", &profile), "`users`");
    let declared_none = DeclaredSqlDialectProfile {
        quote_style: Some(QuoteStyle::None),
        ..DeclaredSqlDialectProfile::default()
    };
    let none_profile = resolve_dialect_profile("mysql", Some(&declared_none));
    assert_eq!(sd::quote_identifier_for_dialect("users", &none_profile), "users");
}

#[test]
fn reserved_keyword_table_matches_react_rows() {
    // Every keyword the React Set carries answers true, exactly once each.
    for kw in sd::SQL_RESERVED_IDENTIFIER_KEYWORDS {
        assert!(sd::is_reserved_keyword(kw), "{kw} must be in the table");
    }
    // Near-misses and casing.
    assert!(!sd::is_reserved_keyword("selects"));
    assert!(!sd::is_reserved_keyword(""));
    assert!(!sd::is_reserved_keyword("SELECT"), "the table is lowercase-exact");
    // The full React count (identifierQuoting.ts's Set literal size).
    assert_eq!(sd::SQL_RESERVED_IDENTIFIER_KEYWORDS.len(), 108);
}

// ---- index.ts: the built-in registry ------------------------------------------

#[test]
fn registry_covers_exactly_the_builtin_families() {
    for family in sd::BUILTIN_DIALECT_FAMILIES {
        let dialect =
            sd::get_builtin_dialect_strategy(family).expect("every built-in family resolves");
        assert_eq!(dialect.family, *family, "the entry's family is its key");
    }
    // The React DIALECTS keys, in declaration order.
    assert_eq!(
        sd::BUILTIN_DIALECT_FAMILIES,
        &[
            "postgresql",
            "mysql",
            "sqlite",
            "sqlserver",
            "clickhouse",
            "duckdb",
            "elasticsearch",
            "mongodb",
            "generic",
            "oracle",
        ]
    );
    // Unknown families resolve to null (React's `DIALECTS[family] ?? null`).
    assert!(sd::get_builtin_dialect_strategy("mssql").is_none(), "no family aliases in the table");
    assert!(sd::get_builtin_dialect_strategy("postgres").is_none());
    assert!(sd::get_builtin_dialect_strategy("").is_none());
}

/// The React dialects.test.ts expectations, read through the registry.
#[test]
fn registry_dialects_match_the_react_test() {
    // sqlite DDL query uses sqlite_master.
    let sqlite = sd::get_builtin_dialect_strategy("sqlite").expect("sqlite");
    let sql = (sqlite.ddl.table_ddl)("users", None).sql;
    assert!(sql.contains("sqlite_master"), "{sql}");

    // postgresql index supports gin; mysql does not.
    let pg = sd::get_builtin_dialect_strategy("postgresql").expect("postgresql");
    let mysql = sd::get_builtin_dialect_strategy("mysql").expect("mysql");
    assert!(pg.index.supported_methods.contains(&sd::IndexMethod::Gin));
    assert!(!mysql.index.supported_methods.contains(&sd::IndexMethod::Gin));

    // mysql drop index includes ON table; postgresql does not.
    assert!((mysql.index.drop_index)("idx_foo", "users", '`').contains(" ON "));
    assert!(!(pg.index.drop_index)("idx_foo", "users", '"').contains(" ON "));

    // mysql create index: unique + USING hash; prefix-length columns.
    let mysql_idx = (mysql.index.create_index)(&sd::CreateIndexOptions {
        index_name: "idx_email",
        table_name: "users",
        columns: &["email".to_string()],
        unique: true,
        method: Some(sd::IndexMethod::Hash),
        quote_char: '`',
    });
    assert!(mysql_idx.contains("UNIQUE INDEX"), "{mysql_idx}");
    assert!(mysql_idx.contains("USING hash"), "{mysql_idx}");
    let prefix_sql = (mysql.index.create_index)(&sd::CreateIndexOptions {
        index_name: "idx_region",
        table_name: "demo_customers",
        columns: &["region(255)".to_string()],
        unique: false,
        method: None,
        quote_char: '`',
    });
    assert_eq!(prefix_sql, "CREATE INDEX `idx_region` ON `demo_customers` (`region`(255))");

    // postgresql create index supports gin method.
    let pg_gin = (pg.index.create_index)(&sd::CreateIndexOptions {
        index_name: "idx_data",
        table_name: "docs",
        columns: &["data".to_string()],
        unique: false,
        method: Some(sd::IndexMethod::Gin),
        quote_char: '"',
    });
    assert!(pg_gin.contains("USING gin"), "{pg_gin}");

    // sqlite create index omits the method clause.
    let sqlite_idx = sd::get_builtin_dialect_strategy("sqlite").expect("sqlite");
    let sqlite_sql = (sqlite_idx.index.create_index)(&sd::CreateIndexOptions {
        index_name: "idx_name",
        table_name: "items",
        columns: &["name".to_string()],
        unique: false,
        method: Some(sd::IndexMethod::Btree),
        quote_char: '"',
    });
    assert!(!sqlite_sql.contains("USING"), "{sqlite_sql}");
    assert!(sqlite_sql.contains("CREATE INDEX"), "{sqlite_sql}");
}

/// The seven extra.ts strategies, assembled: each family, DDL, table SQL
/// kind and row-limit shape re-read from extra.ts.
#[test]
fn extra_dialect_strategies_assemble_their_react_shapes() {
    let sqlserver = sd::get_builtin_dialect_strategy("sqlserver").expect("sqlserver");
    assert_eq!(
        (sqlserver.ddl.table_ddl)("T", None).sql,
        "SELECT OBJECT_DEFINITION(OBJECT_ID('T'))"
    );
    // SQL Server has no LIMIT: TOP n sits in the select list.
    let shape = sqlserver.row_limit_shape();
    assert_eq!(shape.placement, sd::RowLimitPlacement::SelectList);
    assert_eq!((shape.clause)(10), "TOP 10");
    // Double-quote ANSI table SQL.
    let ts = sqlserver.table_sql.expect("sqlserver tableSql");
    assert_eq!(ts.kind(), sd::base_table_sql::GeneratorKind::Ansi);
    assert_eq!(ts.quote_char(), '"');

    let oracle = sd::get_builtin_dialect_strategy("oracle").expect("oracle");
    assert_eq!(
        (oracle.ddl.table_ddl)("T", None).sql,
        "SELECT DBMS_METADATA.GET_DDL('TABLE', 'T') FROM DUAL"
    );
    assert_eq!(
        (oracle.ddl.view_ddl.expect("oracle view ddl"))("V", None).sql,
        "SELECT DBMS_METADATA.GET_DDL('VIEW', 'V') FROM DUAL"
    );
    let oracle_shape = oracle.row_limit_shape();
    assert_eq!(oracle_shape.placement, sd::RowLimitPlacement::Trailing);
    assert_eq!((oracle_shape.clause)(3), "FETCH FIRST 3 ROWS ONLY");

    let clickhouse = sd::get_builtin_dialect_strategy("clickhouse").expect("clickhouse");
    assert_eq!(
        (clickhouse.ddl.table_ddl)("T", None).sql,
        "SELECT create_table_query FROM system.tables WHERE database = currentDatabase() AND name = 'T'"
    );
    // The ClickHouse table-SQL overrides spell ALTER TABLE.
    let ch_ts = clickhouse.table_sql.expect("clickhouse tableSql");
    assert_eq!(ch_ts.kind(), sd::base_table_sql::GeneratorKind::ClickHouse);
    assert_eq!(ch_ts.quote_char(), '`');

    let duckdb = sd::get_builtin_dialect_strategy("duckdb").expect("duckdb");
    assert_eq!(
        (duckdb.ddl.table_ddl)("T", None).sql,
        "SELECT sql FROM duckdb_tables() WHERE table_name = 'T'"
    );
    assert_eq!(
        (duckdb.ddl.view_ddl.expect("duckdb view ddl"))("V", None).sql,
        "SELECT sql FROM duckdb_views() WHERE view_name = 'V'"
    );

    // elasticsearch: no DDL rows, every write throws with React's texts.
    let es = sd::get_builtin_dialect_strategy("elasticsearch").expect("elasticsearch");
    assert_eq!(((es.ddl.table_ddl))("T", None).sql, "SELECT NULL AS sql WHERE 0");
    assert_eq!((es.ddl.table_ddl)("T", None).extract_column_index, 0);
    let es_ts = es.table_sql.expect("elasticsearch tableSql");
    assert_eq!(es_ts.kind(), sd::base_table_sql::GeneratorKind::Elasticsearch);
    assert_eq!(
        es_ts.generate_insert("i", &datazen_ui::relation_metadata::schema::TableSchema::default()),
        Err("Elasticsearch SQL does not support INSERT statements".to_string())
    );
    assert_eq!(
        es_ts.generate_update("i", &datazen_ui::relation_metadata::schema::TableSchema::default()),
        Err("Elasticsearch SQL does not support UPDATE statements".to_string())
    );
    assert_eq!(
        es_ts.generate_delete("i", &datazen_ui::relation_metadata::schema::TableSchema::default()),
        Err("Elasticsearch SQL does not support DELETE statements".to_string())
    );

    // mongodb: no DDL, no tableSql, no backup flags.
    let mongo = sd::get_builtin_dialect_strategy("mongodb").expect("mongodb");
    assert_eq!((mongo.ddl.table_ddl)("T", None).sql, "SELECT NULL AS sql WHERE 0");
    assert!(mongo.table_sql.is_none());
    assert!(mongo.backup_options.is_empty());

    // generic: the ANSI fallback with double quotes.
    let generic = sd::get_builtin_dialect_strategy("generic").expect("generic");
    let gen_ts = generic.table_sql.expect("generic tableSql");
    assert_eq!(gen_ts.kind(), sd::base_table_sql::GeneratorKind::Ansi);
    assert_eq!(gen_ts.quote_char(), '"');
}

/// The ClickHouse ALTER TABLE write shapes through the assembled strategy
/// (the React subclass overrides), against a schema with a declared pk.
#[test]
fn clickhouse_strategy_writes_alter_table_shapes() {
    use datazen_ui::relation_metadata::schema::{ColumnSchema, TableSchema};
    let schema = TableSchema {
        table_name: "users".to_string(),
        columns: vec![
            ColumnSchema { name: "id".to_string(), data_type: "int".to_string(), is_primary_key: true, ..ColumnSchema::default() },
            ColumnSchema { name: "name".to_string(), data_type: "text".to_string(), ..ColumnSchema::default() },
        ],
        primary_keys: vec!["id".to_string()],
        ..TableSchema::default()
    };
    let ch = sd::get_builtin_dialect_strategy("clickhouse").expect("clickhouse");
    let ts = ch.table_sql.expect("clickhouse tableSql");
    assert_eq!(
        ts.generate_update("`users`", &schema),
        Ok("ALTER TABLE `users`\nUPDATE\n  `name` = ''\nWHERE `id` = ;".to_string())
    );
    assert_eq!(
        ts.generate_delete("`users`", &schema),
        Ok("ALTER TABLE `users`\nDELETE WHERE `id` = ;".to_string())
    );
    // The select stays the ANSI shape.
    assert_eq!(ts.generate_select("`users`", &schema), "SELECT `id`, `name`\nFROM `users`;");
}

/// `standardIndex` DDL through the assembled strategies: sqlserver and
/// clickhouse qualify the drop with the table; the bare families do not.
#[test]
fn standard_index_shapes_through_the_strategies() {
    let columns = vec!["a".to_string(), "b".to_string()];
    let opts = |unique: bool, name: &'static str, table: &'static str| {
        sd::CreateIndexOptions {
            index_name: name,
            table_name: table,
            columns: &columns,
            unique,
            method: None,
            quote_char: '"',
        }
    };
    for family in ["sqlserver", "clickhouse"] {
        let d = sd::get_builtin_dialect_strategy(family).expect("family");
        assert!(
            (d.index.drop_index)("idx", "users", '"').contains(" ON "),
            "{family} drop index qualifies with the table"
        );
    }
    for family in ["oracle", "duckdb", "elasticsearch", "mongodb", "generic"] {
        let d = sd::get_builtin_dialect_strategy(family).expect("family");
        assert!(
            !(d.index.drop_index)("idx", "users", '"').contains(" ON "),
            "{family} drop index is bare"
        );
    }
    // The CREATE INDEX string is the standard shape everywhere.
    let generic = sd::get_builtin_dialect_strategy("generic").expect("generic");
    assert_eq!(
        (generic.index.create_index)(&opts(true, "idx_a", "users")),
        "CREATE UNIQUE INDEX \"idx_a\" ON \"users\" (\"a\", \"b\")"
    );
    assert_eq!(
        (generic.index.create_index)(&opts(false, "idx_a", "users")),
        "CREATE INDEX \"idx_a\" ON \"users\" (\"a\", \"b\")"
    );
    // Every standardIndex strategy declares btree+hash.
    for family in sd::BUILTIN_DIALECT_FAMILIES {
        let d = sd::get_builtin_dialect_strategy(family).expect("family");
        if ["sqlserver", "oracle", "clickhouse", "duckdb", "elasticsearch", "mongodb", "generic"]
            .contains(family)
        {
            assert_eq!(d.index.supported_methods, &[sd::IndexMethod::Btree, sd::IndexMethod::Hash]);
        }
    }
}
