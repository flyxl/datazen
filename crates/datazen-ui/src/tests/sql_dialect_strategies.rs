//! T123: real assertions for the Rust ports of
//! `lib/sqlDialects/postgresql.ts`, `lib/sqlDialects/mysql.ts`,
//! `lib/sqlDialects/sqlite.ts`, `lib/sqlDialects/rowLimit.ts` and the
//! carried `BaseTableSqlGenerator` (driver-sdk
//! `sqlDialects/baseTableSql.ts`) + the strategy types
//! (extension-points `sqlDialectTypes.ts`, strategy half).
//!
//! Registered in `tests/mod.rs` as an ordinary test module, so its test
//! names are `tests::sql_dialect_strategies::*`.

use crate::relation_metadata::schema::{ColumnSchema, TableSchema};
use crate::support::sql_dialects as sd;

fn col(name: &str, ty: &str) -> ColumnSchema {
    ColumnSchema {
        name: name.to_string(),
        data_type: ty.to_string(),
        ..ColumnSchema::default()
    }
}

fn schema(columns: Vec<ColumnSchema>, primary_keys: Vec<&str>) -> TableSchema {
    TableSchema {
        table_name: "users".to_string(),
        columns,
        primary_keys: primary_keys.into_iter().map(str::to_string).collect(),
        indexes: Vec::new(),
        foreign_keys: Vec::new(),
        comment: None,
    }
}

/* -------------------------------------------------------------------------- */
/*  PostgreSQL                                                                */
/* -------------------------------------------------------------------------- */

#[test]
fn pg_table_ddl_query_is_the_catalog_template() {
    let q = (sd::POSTGRESQL_DIALECT.ddl.table_ddl)("users", None);
    assert_eq!(q.extract_column_index, 0);
    // The template's exact shape: leading newline, 6-space indent, the
    // E'\n' SQL escapes as backslash-n, the interpolated table name and
    // the trailing 4 spaces before the closing backtick.
    assert!(q.sql.starts_with("\n      SELECT\n        'CREATE TABLE ' || quote_ident(schemaname)"));
    assert!(q.sql.contains("E'(\\n' ||"));
    assert!(q.sql.contains("E',\\n' ORDER BY attnum"));
    assert!(q.sql.contains(") || E'\\n);' AS ddl"));
    assert!(q.sql.contains("AND tablename = 'users'"));
    assert!(q.sql.ends_with("GROUP BY schemaname, tablename;\n    "));
}

#[test]
fn pg_view_ddl_query_doubles_embedded_quotes() {
    let q = (sd::POSTGRESQL_DIALECT.ddl.view_ddl.unwrap())("we'ird", None);
    assert_eq!(q.extract_column_index, 0);
    assert!(q.sql.contains("viewname = 'we''ird'"), "single quotes double into the name");
    assert!(q.sql.starts_with("SELECT 'CREATE OR REPLACE VIEW '"));
}

#[test]
fn pg_index_rules() {
    // Drop is unqualified.
    assert_eq!(
        (sd::POSTGRESQL_DIALECT.index.drop_index)("idx_x", "users", '"'),
        "DROP INDEX \"idx_x\""
    );
    // Methods: gin/gist/hash/btree all listed.
    assert!(sd::POSTGRESQL_DIALECT
        .index
        .supported_methods
        .contains(&sd::IndexMethod::Gin));
    assert!(sd::POSTGRESQL_DIALECT
        .index
        .supported_methods
        .contains(&sd::IndexMethod::Gist));

    // GIN method spells USING gin.
    let sql = (sd::POSTGRESQL_DIALECT.index.create_index)(&sd::CreateIndexOptions {
        index_name: "idx_data",
        table_name: "docs",
        columns: &["data".to_string()],
        unique: false,
        method: Some(sd::IndexMethod::Gin),
        quote_char: '"',
    });
    assert!(sql.contains("USING gin"));
    assert_eq!(
        sql,
        "CREATE INDEX \"idx_data\" ON \"docs\" USING gin (\"data\")"
    );

    // btree (the default) never spells USING; unique prefixes UNIQUE.
    let sql = (sd::POSTGRESQL_DIALECT.index.create_index)(&sd::CreateIndexOptions {
        index_name: "idx_a",
        table_name: "t",
        columns: &["a".to_string(), "b".to_string()],
        unique: true,
        method: Some(sd::IndexMethod::Btree),
        quote_char: '"',
    });
    assert_eq!(sql, "CREATE UNIQUE INDEX \"idx_a\" ON \"t\" (\"a\", \"b\")");
}

/* -------------------------------------------------------------------------- */
/*  MySQL                                                                     */
/* -------------------------------------------------------------------------- */

#[test]
fn mysql_ddl_queries_use_show_create() {
    let t = (sd::MYSQL_DIALECT.ddl.table_ddl)("users", None);
    assert_eq!(t.sql, "SHOW CREATE TABLE `users`");
    assert_eq!(t.extract_column_index, 1, "DDL is the second column");
    let v = (sd::MYSQL_DIALECT.ddl.view_ddl.unwrap())("active", None);
    assert_eq!(v.sql, "SHOW CREATE VIEW `active`");
    assert_eq!(v.extract_column_index, 1);
}

#[test]
fn mysql_index_rules() {
    // Drop is table-qualified.
    assert_eq!(
        (sd::MYSQL_DIALECT.index.drop_index)("idx_x", "users", '`'),
        "DROP INDEX `idx_x` ON `users`"
    );
    // gin is not supported.
    assert!(!sd::MYSQL_DIALECT.index.supported_methods.contains(&sd::IndexMethod::Gin));

    // hash + unique.
    let sql = (sd::MYSQL_DIALECT.index.create_index)(&sd::CreateIndexOptions {
        index_name: "idx_email",
        table_name: "users",
        columns: &["email".to_string()],
        unique: true,
        method: Some(sd::IndexMethod::Hash),
        quote_char: '`',
    });
    assert!(sql.contains("UNIQUE INDEX"));
    assert!(sql.contains("USING hash"));

    // Prefix-length columns quote the name and keep the length — the
    // React test's exact expectation.
    let prefix_sql = (sd::MYSQL_DIALECT.index.create_index)(&sd::CreateIndexOptions {
        index_name: "idx_region",
        table_name: "demo_customers",
        columns: &["region(255)".to_string()],
        unique: false,
        method: None,
        quote_char: '`',
    });
    assert_eq!(prefix_sql, "CREATE INDEX `idx_region` ON `demo_customers` (`region`(255))");

    // Whitespace and inner backticks are trimmed off the name part.
    let padded = (sd::MYSQL_DIALECT.index.create_index)(&sd::CreateIndexOptions {
        index_name: "i",
        table_name: "t",
        columns: &["  `region`(10) ".to_string()],
        unique: false,
        method: None,
        quote_char: '`',
    });
    assert_eq!(padded, "CREATE INDEX `i` ON `t` (`region`(10))");
}

/* -------------------------------------------------------------------------- */
/*  SQLite                                                                    */
/* -------------------------------------------------------------------------- */

#[test]
fn sqlite_rules() {
    let t = (sd::SQLITE_DIALECT.ddl.table_ddl)("items", None);
    assert_eq!(
        t.sql,
        "SELECT sql FROM sqlite_master WHERE type='table' AND name='items'"
    );
    assert_eq!(t.extract_column_index, 0);
    let v = (sd::SQLITE_DIALECT.ddl.view_ddl.unwrap())("v", None);
    assert_eq!(
        v.sql,
        "SELECT sql FROM sqlite_master WHERE type='view' AND name='v'"
    );

    // btree only; create omits USING even when btree is asked for.
    assert_eq!(sd::SQLITE_DIALECT.index.supported_methods, &[sd::IndexMethod::Btree]);
    let sql = (sd::SQLITE_DIALECT.index.create_index)(&sd::CreateIndexOptions {
        index_name: "idx_name",
        table_name: "items",
        columns: &["name".to_string()],
        unique: false,
        method: Some(sd::IndexMethod::Btree),
        quote_char: '"',
    });
    assert!(!sql.contains("USING"));
    assert_eq!(sql, "CREATE INDEX \"idx_name\" ON \"items\" (\"name\")");

    // SQLite has no TRUNCATE — the override deletes.
    assert_eq!(sd::SQLITE_DIALECT.truncate_table_sql("\"items\""), "DELETE FROM \"items\"");
    // The others take the TRUNCATE TABLE default.
    assert_eq!(
        sd::POSTGRESQL_DIALECT.truncate_table_sql("\"users\""),
        "TRUNCATE TABLE \"users\""
    );
    assert_eq!(
        sd::MYSQL_DIALECT.truncate_table_sql("`users`"),
        "TRUNCATE TABLE `users`"
    );
}

#[test]
fn backup_option_lists_match_react() {
    let pg: Vec<&str> = sd::POSTGRESQL_DIALECT.backup_options.iter().map(|o| o.label).collect();
    assert_eq!(pg.len(), 7, "format-custom is removed upstream");
    assert_eq!(pg[0], "--data-only");
    assert_eq!(pg.last(), Some(&"--triggers"));

    let my: Vec<&str> = sd::MYSQL_DIALECT.backup_options.iter().map(|o| o.label).collect();
    assert_eq!(my.len(), 6);
    assert_eq!(my[0], "--no-data");

    let lite: Vec<&str> = sd::SQLITE_DIALECT.backup_options.iter().map(|o| o.label).collect();
    assert_eq!(lite.len(), 5);
    assert_eq!(lite[0], "--schema-only");
}

/* -------------------------------------------------------------------------- */
/*  BaseTableSqlGenerator                                                     */
/* -------------------------------------------------------------------------- */

#[test]
fn base_generator_quotes_and_formats_refs() {
    let pg = sd::BaseTableSqlGenerator::new('"');
    assert_eq!(pg.quote("we\"ird"), "\"we\"\"ird\"");
    let my = sd::BaseTableSqlGenerator::new('`');
    assert_eq!(my.quote("we`ird"), "`we``ird`");
    let ms = sd::BaseTableSqlGenerator::new('[');
    assert_eq!(ms.quote("we]ird"), "[we]]ird]");

    // formatTableRef: prefix joins when non-blank (trimmed); blank and
    // missing prefixes collapse to the bare quoted table.
    assert_eq!(pg.format_table_ref("users", Some("public")), "\"public\".\"users\"");
    assert_eq!(pg.format_table_ref("users", Some("  ")), "\"users\"");
    assert_eq!(pg.format_table_ref("users", None), "\"users\"");
}

#[test]
fn base_generator_select() {
    let pg = sd::BaseTableSqlGenerator::new('"');
    let with_cols = schema(vec![col("id", "int"), col("name", "text")], vec![]);
    assert_eq!(
        pg.generate_select("\"users\"", &with_cols),
        "SELECT \"id\", \"name\"\nFROM \"users\";"
    );
    let empty = schema(vec![], vec![]);
    assert_eq!(pg.generate_select("\"users\"", &empty), "SELECT *\nFROM \"users\";");
}

#[test]
fn base_generator_insert_placeholders() {
    let pg = sd::BaseTableSqlGenerator::new('"');
    let mut with_cols = schema(
        vec![
            ColumnSchema { default_value: Some("nextval('s')".to_string()), ..col("id", "bigint") },
            col("name", "text"),
            col("count", "integer"),
            ColumnSchema { is_auto_increment: true, ..col("seq", "integer") },
        ],
        vec![],
    );
    // Declared default wins; int/numeric types get 0; others ''.
    assert_eq!(
        pg.generate_insert("\"users\"", &with_cols),
        Ok("INSERT INTO \"users\" (\n  \"id\",\n  \"name\",\n  \"count\"\n) VALUES (\n  nextval('s'),\n  '',\n  0\n);".to_string())
    );

    // An empty-string default is falsy in React: it falls through to the
    // type placeholder.
    with_cols.columns[0].default_value = Some(String::new());
    assert!(
        pg.generate_insert("\"users\"", &with_cols)
            .expect("the ANSI generator never errors")
            .contains("\n  0,\n  '',\n  0\n);"),
        "empty defaults fall through to the type placeholder"
    );

    // Auto-increment-only → DEFAULT VALUES; no columns → DEFAULT VALUES.
    let auto_only = schema(
        vec![ColumnSchema { is_auto_increment: true, ..col("id", "integer") }],
        vec![],
    );
    assert_eq!(
        pg.generate_insert("\"users\"", &auto_only),
        Ok("INSERT INTO \"users\"\nDEFAULT VALUES;".to_string())
    );
    assert_eq!(
        pg.generate_insert("\"users\"", &schema(vec![], vec![])),
        Ok("INSERT INTO \"users\"\nDEFAULT VALUES;".to_string())
    );
}

#[test]
fn base_generator_update_and_delete() {
    let pg = sd::BaseTableSqlGenerator::new('"');
    // SET the non-PK columns — PK means the declared list OR the column
    // flag, so the flagged-but-undeclared column is excluded too; WHERE
    // the effective pks (the declared list wins); the trailing `= `
    // waits for a value.
    let mut s = schema(
        vec![
            ColumnSchema { is_primary_key: true, ..col("fake_pk", "int") },
            col("id", "int"),
            col("name", "text"),
        ],
        vec!["id"],
    );
    assert_eq!(
        pg.generate_update("\"users\"", &s),
        Ok("UPDATE \"users\"\nSET\n  \"name\" = ''\nWHERE \"id\" = ;".to_string())
    );
    assert_eq!(
        pg.generate_delete("\"users\"", &s),
        Ok("DELETE FROM \"users\"\nWHERE \"id\" = ;".to_string())
    );

    // No declared list: the column flags become the WHERE keys, and the
    // flagged column is excluded from the SET.
    s.primary_keys.clear();
    assert_eq!(
        pg.generate_update("\"users\"", &s),
        Ok("UPDATE \"users\"\nSET\n  \"id\" = '',\n  \"name\" = ''\nWHERE \"fake_pk\" = ;".to_string())
    );

    // No keys at all: the warning comment, both statements.
    let no_pk = schema(vec![col("a", "int")], vec![]);
    assert!(pg
        .generate_update("\"t\"", &no_pk)
        .expect("the ANSI generator never errors")
        .ends_with("WHERE /* WARNING: Primary Key not found. Specify condition */;"));
    assert!(pg
        .generate_delete("\"t\"", &no_pk)
        .expect("the ANSI generator never errors")
        .ends_with("WHERE /* WARNING: Primary Key not found. Specify condition */;"));

    // Composite keys join with AND — note the double space: each key's
    // trailing `= ` plus the ` AND ` separator, exactly React's output.
    let two_pks = schema(vec![col("tenant_id", "int"), col("id", "int"), col("v", "text")], vec!["tenant_id", "id"]);
    assert_eq!(
        pg.generate_delete("\"t\"", &two_pks),
        Ok("DELETE FROM \"t\"\nWHERE \"tenant_id\" =  AND \"id\" = ;".to_string())
    );

    // Empty schema: the metadata comments.
    assert_eq!(
        pg.generate_update("\"t\"", &schema(vec![], vec![])),
        Ok("/* No column metadata available for UPDATE \"t\" */".to_string())
    );
    assert_eq!(
        pg.generate_delete("\"t\"", &schema(vec![], vec![])),
        Ok("/* No column metadata available for DELETE \"t\" */".to_string())
    );

    // generateSql dispatches.
    assert_eq!(
        pg.generate_sql(sd::GeneratedSqlType::Select, "\"t\"", &no_pk),
        Ok(pg.generate_select("\"t\"", &no_pk))
    );
    assert_eq!(
        pg.generate_sql(sd::GeneratedSqlType::Delete, "\"t\"", &two_pks),
        pg.generate_delete("\"t\"", &two_pks)
    );
}

/* -------------------------------------------------------------------------- */
/*  rowLimit                                                                  */
/* -------------------------------------------------------------------------- */

/// A SQL Server-shaped strategy for the select-list placement proof.
fn top_limit_strategy() -> sd::SqlDialectStrategy {
    fn top_clause(limit: usize) -> String {
        format!("TOP {limit}")
    }
    sd::SqlDialectStrategy {
        family: "test-top",
        ddl: sd::POSTGRESQL_DIALECT.ddl,
        index: sd::POSTGRESQL_DIALECT.index,
        backup_options: &[],
        truncate_table_sql: None,
        table_sql: None,
        row_limit: Some(sd::RowLimitShape {
            placement: sd::RowLimitPlacement::SelectList,
            clause: top_clause,
        }),
    }
}

#[test]
fn row_limit_weaves_the_dialects_shape() {
    let columns = vec!["\"a\"".to_string(), "\"b\"".to_string()];

    // Standard trailing LIMIT n — the declared shape and the fallback.
    let parts = sd::RowLimitedSelectParts {
        distinct: false,
        columns: &columns,
        table: "\"t\"",
        limit: 200,
    };
    assert_eq!(
        sd::build_row_limited_select(Some(&sd::POSTGRESQL_DIALECT), &parts),
        "SELECT \"a\", \"b\" FROM \"t\" LIMIT 200"
    );
    assert_eq!(
        sd::build_row_limited_select(None, &parts),
        "SELECT \"a\", \"b\" FROM \"t\" LIMIT 200",
        "no dialect falls back to the standard shape"
    );

    // Select-list placement sits right behind SELECT [DISTINCT].
    let top = top_limit_strategy();
    assert_eq!(
        sd::build_row_limited_select(Some(&top), &parts),
        "SELECT TOP 200 \"a\", \"b\" FROM \"t\""
    );
    let distinct_parts = sd::RowLimitedSelectParts { distinct: true, ..parts };
    assert_eq!(
        sd::build_row_limited_select(Some(&top), &distinct_parts),
        "SELECT DISTINCT TOP 200 \"a\", \"b\" FROM \"t\""
    );
    assert_eq!(
        sd::build_row_limited_select(Some(&sd::POSTGRESQL_DIALECT), &distinct_parts),
        "SELECT DISTINCT \"a\", \"b\" FROM \"t\" LIMIT 200"
    );
}

#[test]
fn strategy_accessors_apply_the_documented_defaults() {
    // The view fallback: a strategy without a view query uses its table
    // query (modeled through sqlite's, exercising the accessor path).
    let fallback = sd::SqlDialectStrategy {
        family: "fallback",
        ddl: sd::DialectDdl { table_ddl: sd::SQLITE_DIALECT.ddl.table_ddl, view_ddl: None },
        index: sd::SQLITE_DIALECT.index,
        backup_options: &[],
        truncate_table_sql: None,
        table_sql: None,
        row_limit: None,
    };
    assert_eq!(
        fallback.view_ddl_query("items", None).sql,
        (sd::SQLITE_DIALECT.ddl.table_ddl)("items", None).sql
    );
    assert_eq!(fallback.row_limit_shape().placement, sd::RowLimitPlacement::Trailing);
}
