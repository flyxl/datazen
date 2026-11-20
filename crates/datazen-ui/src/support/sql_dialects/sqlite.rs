//! SQLite dialect strategy — the React `lib/sqlDialects/sqlite.ts` port
//! (T123).

use super::strategy::{
    BackupOption, CreateIndexOptions, DdlQuery, DialectDdl, DialectIndex, IndexMethod,
    SqlDialectStrategy,
};

/// `sqliteDialect.ddl.getTableDdlQuery` — the schema is in
/// `sqlite_master`, DDL in the first column.
fn table_ddl_query(table_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery {
        sql: format!(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='{table_name}'"
        ),
        extract_column_index: 0,
    }
}

/// `sqliteDialect.ddl.getViewDdlQuery`.
fn view_ddl_query(view_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery {
        sql: format!(
            "SELECT sql FROM sqlite_master WHERE type='view' AND name='{view_name}'"
        ),
        extract_column_index: 0,
    }
}

/// `sqliteDialect.index.getDropIndexSql` — unqualified.
fn drop_index_sql(index_name: &str, _table_name: &str, quote_char: char) -> String {
    format!("DROP INDEX {quote_char}{index_name}{quote_char}")
}

/// `sqliteDialect.index.getCreateIndexSql` — plain quoted columns, no
/// methods beyond btree.
fn create_index_sql(opts: &CreateIndexOptions<'_>) -> String {
    let unique_kw = if opts.unique { "UNIQUE " } else { "" };
    let q = opts.quote_char;
    let quoted_cols = opts
        .columns
        .iter()
        .map(|c| format!("{q}{c}{q}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CREATE {unique_kw}INDEX {q}{index_name}{q} ON {q}{table_name}{q} ({quoted_cols})",
        index_name = opts.index_name,
        table_name = opts.table_name,
    )
}

/// `sqliteDialect.getTruncateTableSql` — SQLite has no TRUNCATE.
fn truncate_table_sql(quoted_name: &str) -> String {
    format!("DELETE FROM {quoted_name}")
}

/// `sqliteDialect` — double-quote table SQL.
pub(crate) static SQLITE_DIALECT: SqlDialectStrategy = SqlDialectStrategy {
    family: "sqlite",
    ddl: DialectDdl {
        table_ddl: table_ddl_query,
        view_ddl: Some(view_ddl_query),
    },
    index: DialectIndex {
        supported_methods: &[IndexMethod::Btree],
        drop_index: drop_index_sql,
        create_index: create_index_sql,
    },
    backup_options: &[
        BackupOption { id: "schema-only", label: "--schema-only" },
        BackupOption { id: "data-only", label: "--data-only" },
        BackupOption { id: "clean", label: "--clean" },
        BackupOption { id: "no-owner", label: "--no-owner" },
        BackupOption { id: "single-transaction", label: "--single-transaction" },
    ],
    truncate_table_sql: Some(truncate_table_sql),
    table_sql: Some(super::base_table_sql::BaseTableSqlGenerator::new('"')),
    row_limit: None,
};
