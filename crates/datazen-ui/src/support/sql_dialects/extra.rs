//! Port of `packages/frontend/src/lib/sqlDialects/extra.ts`.
//!
//! The dialect strategies the host registers for the non-MySQL-family
//! engines: SQL Server, Oracle, ClickHouse, DuckDB, Elasticsearch,
//! MongoDB and the generic fallback — each as pure SQL-string builders
//! plus its metadata (backup option ids, row-limit clause shapes, index
//! DDL patterns).
//!
//! ## What maps where
//!
//! - React exports `SqlDialectStrategy` objects (interface from
//!   extension-points, host wiring). This module exports the same
//!   information as data and functions: the strategy assembly is the
//!   host's seam. Every SQL string below is byte-identical to the React
//!   closures'.
//! - `BaseTableSqlGenerator` is driver-sdk's (not part of the frontend
//!   migration ledger); the parts extra.ts reaches — `quote` and the
//!   `generateUpdate`/`generateDelete` bases the ClickHouse overrides
//!   delegate to on an empty schema — are ported here as the documented
//!   seam (`quote_identifier`, `base_update_sql`, `base_delete_sql`).
//! - `TableSchemaVm` is the slice of the kernel's `TableSchema` the
//!   generators read (name + `is_primary_key` per column, `primaryKeys`);
//!   `datazen-ui` cannot depend on `datazen-driver-api`, so the host
//!   converts — `effective_primary_keys` there already carries the
//!   pk-fallback rule the generators restate.
//! - Elasticsearch's throwing generators become `Result::Err` with the
//!   exact React error strings.
//! - `noDdl`'s query (`SELECT NULL AS sql WHERE 0`) is
//!   [`NO_DDL_TABLE_QUERY`], verbatim.
//!
//! Known deltas: none behavioural — every string, option list and clause
//! is the React file's verbatim.

/// The `TableSchema` slice the table-SQL generators read.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct TableSchemaVm {
    pub(crate) columns: Vec<ColumnVm>,
    pub(crate) primary_keys: Vec<String>,
}

/// One column's generator-relevant fields.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ColumnVm {
    pub(crate) name: String,
    pub(crate) is_primary_key: bool,
}

/// React `BaseTableSqlGenerator.quote`: backtick and double-quote double
/// themselves inside, bracket doubles `]`; any other char is a passthrough.
pub(crate) fn quote_identifier(quote_char: char, ident: &str) -> String {
    match quote_char {
        '`' => format!("`{}`", ident.replace('`', "``")),
        '"' => format!("\"{}\"", ident.replace('"', "\"\"")),
        '[' => format!("[{}]", ident.replace(']', "]]")),
        _ => ident.to_owned(),
    }
}

/// The generator's pk rule: prefer `primaryKeys`, fall back to columns
/// flagged `isPrimaryKey`.
fn effective_pks(schema: &TableSchemaVm) -> Vec<String> {
    if !schema.primary_keys.is_empty() {
        return schema.primary_keys.clone();
    }
    schema
        .columns
        .iter()
        .filter(|c| c.is_primary_key)
        .map(|c| c.name.clone())
        .collect()
}

/// The shared WHERE-clause builder of the base update/delete generators:
/// `"pk = "` per key joined by ` AND ` — the trailing `=` waits for the
/// user's value — or the warning comment with no keys.
fn where_clause(quote_char: char, pks: &[String]) -> String {
    if pks.is_empty() {
        return "/* WARNING: Primary Key not found. Specify condition */".to_owned();
    }
    pks.iter()
        .map(|pk| format!("{}{}{} = ", quote_char, escape_inner(quote_char, pk), quote_char))
        .collect::<Vec<_>>()
        .join(" AND ")
}

/// The inner-identifier escaping `quote_identifier` applies (shared with
/// the WHERE builder above so both stay byte-identical).
fn escape_inner(quote_char: char, ident: &str) -> String {
    match quote_char {
        '`' => ident.replace('`', "``"),
        '"' => ident.replace('"', "\"\""),
        '[' => ident.replace(']', "]]"),
        _ => ident.to_owned(),
    }
}

/// React `BaseTableSqlGenerator.generateUpdate` (the `super()` the
/// ClickHouse override reaches on an empty schema).
pub(crate) fn base_update_sql(quote_char: char, table_ref: &str, schema: &TableSchemaVm) -> String {
    if schema.columns.is_empty() {
        return format!("/* No column metadata available for UPDATE {table_ref} */");
    }
    let non_pk: Vec<&ColumnVm> = schema
        .columns
        .iter()
        .filter(|c| !schema.primary_keys.contains(&c.name) && !c.is_primary_key)
        .collect();
    let target: Vec<&ColumnVm> = if non_pk.is_empty() { schema.columns.iter().collect() } else { non_pk };
    let set_clauses = target
        .iter()
        .map(|c| format!("  {} = ''", quote_identifier(quote_char, &c.name)))
        .collect::<Vec<_>>()
        .join(",\n");
    let where_ = where_clause(quote_char, &effective_pks(schema));
    format!("UPDATE {table_ref}\nSET\n{set_clauses}\nWHERE {where_};")
}

/// React `BaseTableSqlGenerator.generateDelete`.
pub(crate) fn base_delete_sql(quote_char: char, table_ref: &str, schema: &TableSchemaVm) -> String {
    if schema.columns.is_empty() {
        return format!("/* No column metadata available for DELETE {table_ref} */");
    }
    let where_ = where_clause(quote_char, &effective_pks(schema));
    format!("DELETE FROM {table_ref}\nWHERE {where_};")
}

// ---- ClickHouse --------------------------------------------------------------

/// React `ClickHouseTableSqlGenerator.generateUpdate`:
/// `ALTER TABLE … UPDATE … WHERE …;` (the quote char is backtick).
pub(crate) fn clickhouse_update_sql(table_ref: &str, schema: &TableSchemaVm) -> String {
    if schema.columns.is_empty() {
        return base_update_sql('`', table_ref, schema);
    }
    let non_pk: Vec<&ColumnVm> = schema
        .columns
        .iter()
        .filter(|c| !schema.primary_keys.contains(&c.name) && !c.is_primary_key)
        .collect();
    let target: Vec<&ColumnVm> = if non_pk.is_empty() { schema.columns.iter().collect() } else { non_pk };
    let set_clauses = target
        .iter()
        .map(|c| format!("  {} = ''", quote_identifier('`', &c.name)))
        .collect::<Vec<_>>()
        .join(",\n");
    let where_ = where_clause('`', &effective_pks(schema));
    format!("ALTER TABLE {table_ref}\nUPDATE\n{set_clauses}\nWHERE {where_};")
}

/// React `ClickHouseTableSqlGenerator.generateDelete`:
/// `ALTER TABLE … DELETE WHERE …;`.
pub(crate) fn clickhouse_delete_sql(table_ref: &str, schema: &TableSchemaVm) -> String {
    if schema.columns.is_empty() {
        return base_delete_sql('`', table_ref, schema);
    }
    let where_ = where_clause('`', &effective_pks(schema));
    format!("ALTER TABLE {table_ref}\nDELETE WHERE {where_};")
}

// ---- Elasticsearch -----------------------------------------------------------

/// React `ElasticsearchTableSqlGenerator` — every write throws; the exact
/// error strings are the contract.
pub(crate) fn elasticsearch_insert_sql() -> Result<String, String> {
    Err("Elasticsearch SQL does not support INSERT statements".to_owned())
}

pub(crate) fn elasticsearch_update_sql() -> Result<String, String> {
    Err("Elasticsearch SQL does not support UPDATE statements".to_owned())
}

pub(crate) fn elasticsearch_delete_sql() -> Result<String, String> {
    Err("Elasticsearch SQL does not support DELETE statements".to_owned())
}

// ---- noDdl -------------------------------------------------------------------

/// React `noDdl.getTableDdlQuery().sql` — no rows, keeps the DDL tab
/// harmless for non-SQL engines.
pub(crate) const NO_DDL_TABLE_QUERY: &str = "SELECT NULL AS sql WHERE 0";

// ---- the standard index DDL --------------------------------------------------

/// Which `DROP INDEX` shape `standardIndex` emits.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DropIndexPattern {
    /// `DROP INDEX <index> ON <table>` (sqlserver / clickhouse).
    Table,
    /// `DROP INDEX <index>` (oracle / duckdb / elasticsearch / mongodb / generic).
    Bare,
}

/// React `standardIndex(dropPattern).getDropIndexSql`.
pub(crate) fn drop_index_sql(
    pattern: DropIndexPattern,
    index_name: &str,
    table_name: &str,
    quote_char: char,
) -> String {
    match pattern {
        DropIndexPattern::Bare => {
            format!("DROP INDEX {quote_char}{index_name}{quote_char}")
        }
        DropIndexPattern::Table => format!(
            "DROP INDEX {quote_char}{index_name}{quote_char} ON {quote_char}{table_name}{quote_char}"
        ),
    }
}

/// The supported index methods every `standardIndex` strategy declares.
pub(crate) const SUPPORTED_INDEX_METHODS: &[&str] = &["btree", "hash"];

/// React `standardIndex(…).getCreateIndexSql`.
pub(crate) fn create_index_sql(
    unique: bool,
    index_name: &str,
    table_name: &str,
    columns: &[&str],
    quote_char: char,
) -> String {
    let unique_kw = if unique { "UNIQUE " } else { "" };
    let quoted_cols = columns
        .iter()
        .map(|c| format!("{quote_char}{c}{quote_char}"))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CREATE {unique_kw}INDEX {quote_char}{index_name}{quote_char} ON {quote_char}{table_name}{quote_char} ({quoted_cols})"
    )
}

// ---- the DDL queries (every `'` doubles) --------------------------------------

fn escape_single_quotes(value: &str) -> String {
    value.replace('\'', "''")
}

/// React `sqlserverDialect.ddl.getTableDdlQuery`.
pub(crate) fn sqlserver_table_ddl_query(table_name: &str) -> String {
    format!(
        "SELECT OBJECT_DEFINITION(OBJECT_ID('{}'))",
        escape_single_quotes(table_name)
    )
}

/// React `oracleDialect.ddl.getTableDdlQuery`.
pub(crate) fn oracle_table_ddl_query(table_name: &str) -> String {
    format!(
        "SELECT DBMS_METADATA.GET_DDL('TABLE', '{}') FROM DUAL",
        escape_single_quotes(table_name)
    )
}

/// React `oracleDialect.ddl.getViewDdlQuery`.
pub(crate) fn oracle_view_ddl_query(view_name: &str) -> String {
    format!(
        "SELECT DBMS_METADATA.GET_DDL('VIEW', '{}') FROM DUAL",
        escape_single_quotes(view_name)
    )
}

/// React `clickhouseDialect.ddl.getTableDdlQuery`.
pub(crate) fn clickhouse_table_ddl_query(table_name: &str) -> String {
    format!(
        "SELECT create_table_query FROM system.tables WHERE database = currentDatabase() AND name = '{}'",
        escape_single_quotes(table_name)
    )
}

/// React `duckdbDialect.ddl.getTableDdlQuery`.
pub(crate) fn duckdb_table_ddl_query(table_name: &str) -> String {
    format!(
        "SELECT sql FROM duckdb_tables() WHERE table_name = '{}'",
        escape_single_quotes(table_name)
    )
}

/// React `duckdbDialect.ddl.getViewDdlQuery`.
pub(crate) fn duckdb_view_ddl_query(view_name: &str) -> String {
    format!(
        "SELECT sql FROM duckdb_views() WHERE view_name = '{}'",
        escape_single_quotes(view_name)
    )
}

// ---- row limits ---------------------------------------------------------------

/// React `sqlserverDialect.rowLimit`: `TOP n` inside the select list.
pub(crate) fn sqlserver_row_limit_clause(limit: usize) -> String {
    format!("TOP {limit}")
}

/// React `oracleDialect.rowLimit`: `FETCH FIRST n ROWS ONLY` at the end.
pub(crate) fn oracle_row_limit_clause(limit: usize) -> String {
    format!("FETCH FIRST {limit} ROWS ONLY")
}

// ---- backup options ------------------------------------------------------------

/// React `sqlserverDialect.backupOptions`.
pub(crate) const SQLSERVER_BACKUP_OPTIONS: &[super::strategy::BackupOption] = &[
    super::strategy::BackupOption { id: "schema-only", label: "--schema-only" },
    super::strategy::BackupOption { id: "data-only", label: "--data-only" },
    super::strategy::BackupOption { id: "clean", label: "--clean" },
    super::strategy::BackupOption { id: "no-owner", label: "--no-owner" },
    super::strategy::BackupOption { id: "single-transaction", label: "--single-transaction" },
];

/// React `oracleDialect.backupOptions`.
pub(crate) const ORACLE_BACKUP_OPTIONS: &[super::strategy::BackupOption] = &[
    super::strategy::BackupOption { id: "schema-only", label: "--schema-only" },
    super::strategy::BackupOption { id: "data-only", label: "--data-only" },
    super::strategy::BackupOption { id: "clean", label: "--clean" },
];

/// React `clickhouseDialect.backupOptions`.
pub(crate) const CLICKHOUSE_BACKUP_OPTIONS: &[super::strategy::BackupOption] = &[
    super::strategy::BackupOption { id: "schema-only", label: "--schema-only" },
    super::strategy::BackupOption { id: "data-only", label: "--data-only" },
    super::strategy::BackupOption { id: "clean", label: "--clean" },
    super::strategy::BackupOption { id: "no-owner", label: "--no-owner" },
];

/// React `duckdbDialect.backupOptions`.
pub(crate) const DUCKDB_BACKUP_OPTIONS: &[super::strategy::BackupOption] = &[
    super::strategy::BackupOption { id: "schema-only", label: "--schema-only" },
    super::strategy::BackupOption { id: "data-only", label: "--data-only" },
    super::strategy::BackupOption { id: "clean", label: "--clean" },
    super::strategy::BackupOption { id: "no-owner", label: "--no-owner" },
    super::strategy::BackupOption { id: "single-transaction", label: "--single-transaction" },
];

/// The engines with no backup options (elasticsearch, mongodb, generic).
pub(crate) const NO_BACKUP_OPTIONS: &[super::strategy::BackupOption] = &[];

// ---- the assembled strategies (React's `export const xDialect`) ---------------

use super::strategy::{
    CreateIndexOptions, DdlQuery, DialectDdl, DialectIndex, IndexMethod, RowLimitPlacement,
    RowLimitShape, SqlDialectStrategy,
};

/// The supported methods every `standardIndex` declares.
const STANDARD_METHODS: &[IndexMethod] = &[IndexMethod::Btree, IndexMethod::Hash];

/// `standardIndex('bare').getDropIndexSql`.
fn drop_index_bare(index_name: &str, _table_name: &str, quote_char: char) -> String {
    drop_index_sql(DropIndexPattern::Bare, index_name, _table_name, quote_char)
}

/// `standardIndex('table').getDropIndexSql`.
fn drop_index_table(index_name: &str, table_name: &str, quote_char: char) -> String {
    drop_index_sql(DropIndexPattern::Table, index_name, table_name, quote_char)
}

/// `standardIndex(…).getCreateIndexSql` (no method in the CREATE string).
fn create_index(opts: &CreateIndexOptions<'_>) -> String {
    let columns: Vec<&str> = opts.columns.iter().map(String::as_str).collect();
    create_index_sql(opts.unique, opts.index_name, opts.table_name, &columns, opts.quote_char)
}

/// `noDdl.getTableDdlQuery` — no rows, the DDL tab stays harmless.
fn no_ddl_query(_table_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery { sql: NO_DDL_TABLE_QUERY.to_owned(), extract_column_index: 0 }
}

/// `sqlserverDialect.ddl.getTableDdlQuery`.
fn sqlserver_ddl(table_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery { sql: sqlserver_table_ddl_query(table_name), extract_column_index: 0 }
}

/// `oracleDialect.ddl.getTableDdlQuery`.
fn oracle_ddl(table_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery { sql: oracle_table_ddl_query(table_name), extract_column_index: 0 }
}

/// `oracleDialect.ddl.getViewDdlQuery`.
fn oracle_view_ddl(view_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery { sql: oracle_view_ddl_query(view_name), extract_column_index: 0 }
}

/// `clickhouseDialect.ddl.getTableDdlQuery`.
fn clickhouse_ddl(table_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery { sql: clickhouse_table_ddl_query(table_name), extract_column_index: 0 }
}

/// `duckdbDialect.ddl.getTableDdlQuery`.
fn duckdb_ddl(table_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery { sql: duckdb_table_ddl_query(table_name), extract_column_index: 0 }
}

/// `duckdbDialect.ddl.getViewDdlQuery`.
fn duckdb_view_ddl(view_name: &str, _schema: Option<&str>) -> DdlQuery {
    DdlQuery { sql: duckdb_view_ddl_query(view_name), extract_column_index: 0 }
}

/// React `sqlserverDialect` — `TOP n` inside the select list, bracket
/// quoting, double-quote table SQL.
pub(crate) static SQLSERVER_DIALECT: SqlDialectStrategy = SqlDialectStrategy {
    family: "sqlserver",
    ddl: DialectDdl { table_ddl: sqlserver_ddl, view_ddl: None },
    index: DialectIndex {
        supported_methods: STANDARD_METHODS,
        drop_index: drop_index_table,
        create_index,
    },
    backup_options: SQLSERVER_BACKUP_OPTIONS,
    truncate_table_sql: None,
    table_sql: Some(super::base_table_sql::BaseTableSqlGenerator::new('"')),
    row_limit: Some(RowLimitShape {
        placement: RowLimitPlacement::SelectList,
        clause: sqlserver_row_limit_clause,
    }),
};

/// React `oracleDialect` — `FETCH FIRST n ROWS ONLY` trailing, DDL from
/// `DBMS_METADATA`.
pub(crate) static ORACLE_DIALECT: SqlDialectStrategy = SqlDialectStrategy {
    family: "oracle",
    ddl: DialectDdl { table_ddl: oracle_ddl, view_ddl: Some(oracle_view_ddl) },
    index: DialectIndex {
        supported_methods: STANDARD_METHODS,
        drop_index: drop_index_bare,
        create_index,
    },
    backup_options: ORACLE_BACKUP_OPTIONS,
    truncate_table_sql: None,
    table_sql: Some(super::base_table_sql::BaseTableSqlGenerator::new('"')),
    row_limit: Some(RowLimitShape {
        placement: RowLimitPlacement::Trailing,
        clause: oracle_row_limit_clause,
    }),
};

/// React `clickhouseDialect` — backticks, `ALTER TABLE` write shapes.
pub(crate) static CLICKHOUSE_DIALECT: SqlDialectStrategy = SqlDialectStrategy {
    family: "clickhouse",
    ddl: DialectDdl { table_ddl: clickhouse_ddl, view_ddl: None },
    index: DialectIndex {
        supported_methods: STANDARD_METHODS,
        drop_index: drop_index_table,
        create_index,
    },
    backup_options: CLICKHOUSE_BACKUP_OPTIONS,
    truncate_table_sql: None,
    table_sql: Some(super::base_table_sql::BaseTableSqlGenerator::clickhouse()),
    row_limit: None,
};

/// React `duckdbDialect` — DDL from the `duckdb_tables()`/`duckdb_views()`
/// catalogs.
pub(crate) static DUCKDB_DIALECT: SqlDialectStrategy = SqlDialectStrategy {
    family: "duckdb",
    ddl: DialectDdl { table_ddl: duckdb_ddl, view_ddl: Some(duckdb_view_ddl) },
    index: DialectIndex {
        supported_methods: STANDARD_METHODS,
        drop_index: drop_index_bare,
        create_index,
    },
    backup_options: DUCKDB_BACKUP_OPTIONS,
    truncate_table_sql: None,
    table_sql: Some(super::base_table_sql::BaseTableSqlGenerator::new('"')),
    row_limit: None,
};

/// React `elasticsearchDialect` — no DDL, every write throws.
pub(crate) static ELASTICSEARCH_DIALECT: SqlDialectStrategy = SqlDialectStrategy {
    family: "elasticsearch",
    ddl: DialectDdl { table_ddl: no_ddl_query, view_ddl: None },
    index: DialectIndex {
        supported_methods: STANDARD_METHODS,
        drop_index: drop_index_bare,
        create_index,
    },
    backup_options: NO_BACKUP_OPTIONS,
    truncate_table_sql: None,
    table_sql: Some(super::base_table_sql::BaseTableSqlGenerator::elasticsearch()),
    row_limit: None,
};

/// React `mongodbDialect` — no DDL, no table SQL, no backup flags.
pub(crate) static MONGODB_DIALECT: SqlDialectStrategy = SqlDialectStrategy {
    family: "mongodb",
    ddl: DialectDdl { table_ddl: no_ddl_query, view_ddl: None },
    index: DialectIndex {
        supported_methods: STANDARD_METHODS,
        drop_index: drop_index_bare,
        create_index,
    },
    backup_options: NO_BACKUP_OPTIONS,
    truncate_table_sql: None,
    table_sql: None,
    row_limit: None,
};

/// React `genericDialect` — the ANSI fallback.
pub(crate) static GENERIC_DIALECT: SqlDialectStrategy = SqlDialectStrategy {
    family: "generic",
    ddl: DialectDdl { table_ddl: no_ddl_query, view_ddl: None },
    index: DialectIndex {
        supported_methods: STANDARD_METHODS,
        drop_index: drop_index_bare,
        create_index,
    },
    backup_options: NO_BACKUP_OPTIONS,
    truncate_table_sql: None,
    table_sql: Some(super::base_table_sql::BaseTableSqlGenerator::new('"')),
    row_limit: None,
};
