//! Independent audit for T035 (the sqlDialects dialectProfile + extra ports).
//!
//! Tester-written: the coder's module is treated as a black box and every
//! assertion below comes from re-reading
//! `packages/frontend/src/lib/sqlDialects/dialectProfile.ts`,
//! `packages/frontend/src/lib/sqlDialects/extra.ts` and their React tests
//! `__tests__/{dialects,extra}.test.ts` — the profile table row by row, the
//! declared-profile wholesale replacement, the alias map, the exact DDL /
//! index / row-limit / backup strings, and the ClickHouse update/delete
//! shapes (down to the trailing `= ` awaiting the user's value).
//!
//! These are pure functions; the mutation proof (out-of-tree scratch crate)
//! covers the same surface plus defect injection, and this file is the
//! in-tree pin the coordinator's reference machine runs.

use crate::support::sql_dialects::dialect_profile::{
    DeclaredParameterPolicy, DeclaredSqlDialectProfile, FoldCase, ProjectionAliasVisibility,
    QuoteStyle, builtin_dialect_profile, fold_identifier_for_dialect,
    list_builtin_dialect_profile_ids, normalize_declared_profile, resolve_dialect_profile,
};
use crate::support::sql_dialects::extra::{
    BackupOption, ColumnVm, DropIndexPattern, TableSchemaVm, base_delete_sql, base_update_sql,
    clickhouse_delete_sql, clickhouse_table_ddl_query, clickhouse_update_sql,
    create_index_sql, drop_index_sql, duckdb_table_ddl_query, duckdb_view_ddl_query,
    elasticsearch_delete_sql, elasticsearch_insert_sql, elasticsearch_update_sql,
    no_backup_options, oracle_backup_options, oracle_row_limit_clause, oracle_table_ddl_query,
    oracle_view_ddl_query, quote_identifier, sqlserver_backup_options,
    sqlserver_row_limit_clause, sqlserver_table_ddl_query, duckdb_backup_options,
    clickhouse_backup_options, NO_DDL_TABLE_QUERY, SUPPORTED_INDEX_METHODS,
};

// ---- the profile table, row by row ------------------------------------------

#[test]
fn the_builtin_keys_are_the_react_table_in_order() {
    assert_eq!(
        list_builtin_dialect_profile_ids(),
        [
            "standard", "generic", "postgresql", "postgres", "pg", "sqlite", "mysql", "mariadb",
            "sqlserver", "mssql", "duckdb", "clickhouse", "oracle",
        ]
    );
    for key in list_builtin_dialect_profile_ids() {
        assert!(builtin_dialect_profile(key).is_some(), "{key} resolves");
    }
    assert!(builtin_dialect_profile("redis").is_none(), "redis has no builtin");
}

#[test]
fn the_profile_rows_match_react() {
    let standard = resolve_dialect_profile("standard", None);
    assert_eq!(standard.quote_style, QuoteStyle::Double);
    assert_eq!(standard.fold_case, FoldCase::Lower);
    assert_eq!(standard.projection_alias_visibility, ProjectionAliasVisibility::SelectOnly);
    assert!(standard.parameter_policy.dollar_positional);
    assert!(!standard.parameter_policy.question);
    assert!(!standard.parameter_policy.at_named);

    let sqlite = resolve_dialect_profile("sqlite", None);
    assert!(sqlite.parameter_policy.question);
    assert!(!sqlite.parameter_policy.dollar_positional);

    let mysql = resolve_dialect_profile("mysql", None);
    assert_eq!(mysql.quote_style, QuoteStyle::Backtick);
    assert_eq!(mysql.projection_alias_visibility, ProjectionAliasVisibility::OrderGroup);
    assert!(mysql.parameter_policy.question);

    let sqlserver = resolve_dialect_profile("sqlserver", None);
    assert_eq!(sqlserver.quote_style, QuoteStyle::Bracket);
    assert_eq!(sqlserver.fold_case, FoldCase::Preserve);
    assert_eq!(sqlserver.projection_alias_visibility, ProjectionAliasVisibility::Broad);
    assert!(sqlserver.parameter_policy.at_named);

    let duckdb = resolve_dialect_profile("duckdb", None);
    assert!(duckdb.parameter_policy.question && duckdb.parameter_policy.dollar_positional);

    let clickhouse = resolve_dialect_profile("clickhouse", None);
    assert_eq!(clickhouse.quote_style, QuoteStyle::Backtick);
    assert_eq!(clickhouse.projection_alias_visibility, ProjectionAliasVisibility::Broad);
    assert!(!clickhouse.parameter_policy.question);

    let oracle = resolve_dialect_profile("oracle", None);
    assert_eq!(oracle.fold_case, FoldCase::Upper);
    assert_eq!(oracle.projection_alias_visibility, ProjectionAliasVisibility::Broad);
}

#[test]
fn aliases_trim_case_and_unknown_ids_resolve_like_react() {
    // Family aliases share the family row.
    assert_eq!(resolve_dialect_profile("pg", None), resolve_dialect_profile("postgresql", None));
    assert_eq!(resolve_dialect_profile("mariadb", None), resolve_dialect_profile("mysql", None));
    assert_eq!(resolve_dialect_profile("mssql", None), resolve_dialect_profile("sqlserver", None));
    assert_eq!(resolve_dialect_profile("generic", None), resolve_dialect_profile("standard", None));
    // Trim + case-insensitive + empty + unknown -> standard.
    assert_eq!(resolve_dialect_profile("  PostgreSQL ", None), resolve_dialect_profile("postgresql", None));
    assert_eq!(resolve_dialect_profile("", None), resolve_dialect_profile("standard", None));
    assert_eq!(resolve_dialect_profile("   ", None), resolve_dialect_profile("standard", None));
    assert_eq!(resolve_dialect_profile("redis", None), resolve_dialect_profile("standard", None));
}

#[test]
fn a_declared_profile_replaces_the_family_default_wholesale() {
    let declared = DeclaredSqlDialectProfile {
        quote_style: Some(QuoteStyle::Backtick),
        fold_case: Some(FoldCase::Preserve),
        projection_alias_visibility: Some(ProjectionAliasVisibility::Broad),
        parameter_policy: DeclaredParameterPolicy {
            at_named: None, // omitted -> not accepted
            question: None, // omitted -> not accepted
            dollar_positional: Some(false),
            dollar_named: Some(true),
            template: None,
        },
        reserved_keywords: Some(vec!["select".to_owned()]),
    };
    let profile = normalize_declared_profile(&declared);
    assert_eq!(profile.quote_style, QuoteStyle::Backtick);
    assert_eq!(profile.fold_case, FoldCase::Preserve);
    assert_eq!(profile.projection_alias_visibility, ProjectionAliasVisibility::Broad);
    assert!(!profile.parameter_policy.at_named, "omitted flags default false");
    assert!(!profile.parameter_policy.question, "omitted flags default false");
    assert!(!profile.parameter_policy.dollar_positional);
    assert!(profile.parameter_policy.dollar_named);
    assert!(!profile.parameter_policy.template);
    assert_eq!(profile.reserved_keywords.as_deref(), Some(&["select".to_owned()][..]));

    // Resolution prefers the declaration over the builtin row (postgres here
    // would fold lower; the declaration preserves).
    let resolved = resolve_dialect_profile("postgresql", Some(&declared));
    assert_eq!(resolved.fold_case, FoldCase::Preserve);
    assert_eq!(resolved.quote_style, QuoteStyle::Backtick);
}

#[test]
fn folding_follows_the_dialect() {
    assert_eq!(fold_identifier_for_dialect("Users", "postgresql", None), "users");
    assert_eq!(fold_identifier_for_dialect("Users", "oracle", None), "USERS");
    assert_eq!(fold_identifier_for_dialect("Users", "sqlserver", None), "Users");
    // A declared override changes the fold.
    let declared = DeclaredSqlDialectProfile {
        fold_case: Some(FoldCase::Upper),
        ..Default::default()
    };
    assert_eq!(fold_identifier_for_dialect("Users", "postgresql", Some(&declared)), "USERS");
}

// ---- quoting and the table-SQL bases ----------------------------------------

#[test]
fn quoting_doubles_the_escapables() {
    assert_eq!(quote_identifier('"', "us\"ers"), "\"us\"\"ers\"");
    assert_eq!(quote_identifier('`', "us`ers"), "`us``ers`");
    assert_eq!(quote_identifier('[', "us]ers"), "[us]]ers]");
    assert_eq!(quote_identifier('"', "users"), "\"users\"");
}

fn users_schema() -> TableSchemaVm {
    TableSchemaVm {
        columns: vec![
            ColumnVm { name: "id".to_owned(), is_primary_key: true },
            ColumnVm { name: "name".to_owned(), is_primary_key: false },
        ],
        primary_keys: vec!["id".to_owned()],
    }
}

#[test]
fn the_base_update_and_delete_shapes_match_the_sdk_base() {
    let schema = users_schema();
    assert_eq!(
        base_update_sql('"', "\"users\"", &schema),
        "UPDATE \"users\"\nSET\n  \"name\" = ''\nWHERE \"id\" = ;"
    );
    assert_eq!(
        base_delete_sql('"', "\"users\"", &schema),
        "DELETE FROM \"users\"\nWHERE \"id\" = ;"
    );
    // No columns: the base emits its comment placeholders.
    let empty = TableSchemaVm::default();
    assert_eq!(
        base_update_sql('"', "\"t\"", &empty),
        "/* No column metadata available for UPDATE \"t\" */"
    );
    assert_eq!(
        base_delete_sql('"', "\"t\"", &empty),
        "/* No column metadata available for DELETE \"t\" */"
    );
}

#[test]
fn pk_flagged_columns_standin_when_the_list_is_empty() {
    let mut schema = users_schema();
    schema.primary_keys.clear(); // fall back to is_primary_key flags
    assert_eq!(
        base_delete_sql('"', "\"users\"", &schema),
        "DELETE FROM \"users\"\nWHERE \"id\" = ;"
    );
}

#[test]
fn no_keys_at_all_leaves_the_warning_where_clause() {
    let mut schema = users_schema();
    schema.primary_keys.clear();
    schema.columns[0].is_primary_key = false;
    assert_eq!(
        base_delete_sql('"', "\"users\"", &schema),
        "DELETE FROM \"users\"\nWHERE /* WARNING: Primary Key not found. Specify condition */;"
    );
}

// ---- ClickHouse --------------------------------------------------------------

#[test]
fn clickhouse_update_and_delete_use_alter_table() {
    let schema = users_schema();
    assert_eq!(
        clickhouse_update_sql("`users`", &schema),
        "ALTER TABLE `users`\nUPDATE\n  `name` = ''\nWHERE `id` = ;"
    );
    assert_eq!(
        clickhouse_delete_sql("`users`", &schema),
        "ALTER TABLE `users`\nDELETE WHERE `id` = ;"
    );
    // Empty schema delegates to the base comment (React's super() call).
    let empty = TableSchemaVm::default();
    assert_eq!(
        clickhouse_update_sql("`t`", &empty),
        "/* No column metadata available for UPDATE `t` */"
    );
    // Multiple pks join with AND.
    let mut two = users_schema();
    two.primary_keys.push("tenant".to_owned());
    assert!(clickhouse_delete_sql("`users`", &two).contains("`id` =  AND `tenant` = "));
}

// ---- Elasticsearch -----------------------------------------------------------

#[test]
fn elasticsearch_write_statements_are_errors_with_exact_messages() {
    assert_eq!(
        elasticsearch_insert_sql(),
        Err("Elasticsearch SQL does not support INSERT statements".to_owned())
    );
    assert_eq!(
        elasticsearch_update_sql(),
        Err("Elasticsearch SQL does not support UPDATE statements".to_owned())
    );
    assert_eq!(
        elasticsearch_delete_sql(),
        Err("Elasticsearch SQL does not support DELETE statements".to_owned())
    );
}

// ---- index DDL ---------------------------------------------------------------

#[test]
fn drop_index_shapes_follow_the_pattern() {
    // React test: duckdb (bare) omits ON; sqlserver (table) includes it.
    assert_eq!(drop_index_sql(DropIndexPattern::Bare, "idx", "users", '"'), "DROP INDEX \"idx\"");
    assert!(drop_index_sql(DropIndexPattern::Table, "idx", "users", '"').contains("ON \"users\""));
    assert_eq!(SUPPORTED_INDEX_METHODS, &["btree", "hash"]);
}

#[test]
fn create_index_shapes_unique_and_plain() {
    // React test: unique + column list.
    let sql = create_index_sql(true, "idx_a", "t", &["a", "b"], '`');
    assert!(sql.contains("UNIQUE INDEX"));
    assert!(sql.contains("`a`, `b`"));
    assert_eq!(sql, "CREATE UNIQUE INDEX `idx_a` ON `t` (`a`, `b`)");
    // The whole column spec is quoted — the `region`(255)` prefix-index form
    // the React suite pins belongs to the mysql dialect's own strategy
    // (lib/sqlDialects/index.ts, T084), not to standardIndex.
    let plain = create_index_sql(false, "idx_region", "demo_customers", &["region(255)"], '`');
    assert_eq!(plain, "CREATE INDEX `idx_region` ON `demo_customers` (`region(255)`)");
}

// ---- the DDL queries ---------------------------------------------------------

#[test]
fn ddl_queries_escape_single_quotes_and_target_the_catalogs() {
    // React test: sqlserver escapes the quote in the table name.
    let q = sqlserver_table_ddl_query("users'; DROP--");
    assert!(q.contains("users''; DROP--"), "{q}");
    assert!(q.starts_with("SELECT OBJECT_DEFINITION(OBJECT_ID('"));

    assert!(clickhouse_table_ddl_query("events").contains("system.tables"));
    assert!(clickhouse_table_ddl_query("events").contains("events"));
    assert!(duckdb_table_ddl_query("t1").contains("duckdb_tables()"));
    assert!(duckdb_view_ddl_query("v1").contains("duckdb_views()"));

    assert_eq!(
        oracle_table_ddl_query("emp"),
        "SELECT DBMS_METADATA.GET_DDL('TABLE', 'emp') FROM DUAL"
    );
    assert_eq!(
        oracle_view_ddl_query("v"),
        "SELECT DBMS_METADATA.GET_DDL('VIEW', 'v') FROM DUAL"
    );
    // The no-op DDL query keeps the tab harmless (React: WHERE 0).
    assert!(NO_DDL_TABLE_QUERY.contains("WHERE 0"));
}

// ---- row limits and backup options -------------------------------------------

#[test]
fn row_limit_clauses_match_their_dialects() {
    assert_eq!(sqlserver_row_limit_clause(5), "TOP 5");
    assert_eq!(oracle_row_limit_clause(10), "FETCH FIRST 10 ROWS ONLY");
}

fn option_ids(options: &[BackupOption]) -> Vec<&str> {
    options.iter().map(|(id, _)| *id).collect()
}

#[test]
fn backup_option_tables_match_react() {
    assert_eq!(
        option_ids(sqlserver_backup_options()),
        vec!["schema-only", "data-only", "clean", "no-owner", "single-transaction"]
    );
    assert_eq!(option_ids(oracle_backup_options()), vec!["schema-only", "data-only", "clean"]);
    assert_eq!(
        option_ids(clickhouse_backup_options()),
        vec!["schema-only", "data-only", "clean", "no-owner"]
    );
    assert_eq!(
        option_ids(duckdb_backup_options()),
        vec!["schema-only", "data-only", "clean", "no-owner", "single-transaction"]
    );
    assert!(no_backup_options().is_empty());
}
