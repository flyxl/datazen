//! MySQL dialect strategy — the React `lib/sqlDialects/mysql.ts` port
//! (T123).

use super::strategy::{
    BackupOption, CreateIndexOptions, DdlQuery, DialectDdl, DialectIndex, IndexMethod,
    SqlDialectStrategy,
};

/// `MYSQL_OPTIONS`.
const MYSQL_OPTIONS: &[BackupOption] = &[
    BackupOption { id: "no-data", label: "--no-data" },
    BackupOption { id: "add-drop-table", label: "--add-drop-table" },
    BackupOption { id: "single-transaction", label: "--single-transaction" },
    BackupOption { id: "routines", label: "--routines" },
    BackupOption { id: "triggers", label: "--triggers" },
    BackupOption { id: "no-create-info", label: "--no-create-info" },
];

/// `mysqlDialect.ddl.getTableDdlQuery` — DDL is the second column of
/// `SHOW CREATE TABLE`.
fn table_ddl_query(table_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery {
        sql: format!("SHOW CREATE TABLE `{table_name}`"),
        extract_column_index: 1,
    }
}

/// `mysqlDialect.ddl.getViewDdlQuery` — same shape for views.
fn view_ddl_query(view_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery {
        sql: format!("SHOW CREATE VIEW `{view_name}`"),
        extract_column_index: 1,
    }
}

/// `mysqlDialect.index.getDropIndexSql` — MySQL qualifies the drop with
/// the table.
fn drop_index_sql(index_name: &str, table_name: &str, quote_char: char) -> String {
    format!("DROP INDEX {quote_char}{index_name}{quote_char} ON {quote_char}{table_name}{quote_char}")
}

/// `mysqlDialect.index.getCreateIndexSql` — `USING hash` only for the
/// hash method, and prefix-length columns (`name(10)`) keep their
/// length while the name quotes.
fn create_index_sql(opts: &CreateIndexOptions<'_>) -> String {
    let unique_kw = if opts.unique { "UNIQUE " } else { "" };
    let using_kw = if opts.method == Some(IndexMethod::Hash) {
        " USING hash"
    } else {
        ""
    };
    let q = opts.quote_char;
    let format_col = |col: &str| -> String {
        let trimmed = col.trim();
        let p_idx = trimmed.find('(');
        match p_idx {
            Some(idx) if trimmed.ends_with(')') && idx > 0 => {
                let name = trimmed[..idx]
                    .trim()
                    .trim_start_matches('`')
                    .trim_end_matches('`')
                    .to_string();
                let len = &trimmed[idx..];
                format!("{q}{name}{q}{len}")
            }
            _ => format!("{q}{trimmed}{q}"),
        }
    };
    let quoted_cols = opts.columns.iter().map(|c| format_col(c)).collect::<Vec<_>>().join(", ");
    format!(
        "CREATE {unique_kw}INDEX {q}{index_name}{q} ON {q}{table_name}{q}{using_kw} ({quoted_cols})",
        index_name = opts.index_name,
        table_name = opts.table_name,
    )
}

/// `mysqlDialect` — backtick table SQL.
pub(crate) static MYSQL_DIALECT: SqlDialectStrategy = SqlDialectStrategy {
    family: "mysql",
    ddl: DialectDdl {
        table_ddl: table_ddl_query,
        view_ddl: Some(view_ddl_query),
    },
    index: DialectIndex {
        supported_methods: &[IndexMethod::Btree, IndexMethod::Hash],
        drop_index: drop_index_sql,
        create_index: create_index_sql,
    },
    backup_options: MYSQL_OPTIONS,
    truncate_table_sql: None,
    table_sql: Some(super::base_table_sql::BaseTableSqlGenerator::new('`')),
    row_limit: None,
};
