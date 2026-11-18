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

/// One `{ id, label }` backup option.
pub(crate) type BackupOption = (&'static str, &'static str);

pub(crate) fn sqlserver_backup_options() -> &'static [BackupOption] {
    &[
        ("schema-only", "--schema-only"),
        ("data-only", "--data-only"),
        ("clean", "--clean"),
        ("no-owner", "--no-owner"),
        ("single-transaction", "--single-transaction"),
    ]
}

pub(crate) fn oracle_backup_options() -> &'static [BackupOption] {
    &[("schema-only", "--schema-only"), ("data-only", "--data-only"), ("clean", "--clean")]
}

pub(crate) fn clickhouse_backup_options() -> &'static [BackupOption] {
    &[
        ("schema-only", "--schema-only"),
        ("data-only", "--data-only"),
        ("clean", "--clean"),
        ("no-owner", "--no-owner"),
    ]
}

pub(crate) fn duckdb_backup_options() -> &'static [BackupOption] {
    &[
        ("schema-only", "--schema-only"),
        ("data-only", "--data-only"),
        ("clean", "--clean"),
        ("no-owner", "--no-owner"),
        ("single-transaction", "--single-transaction"),
    ]
}

/// The engines with no backup options (elasticsearch, mongodb, generic).
pub(crate) fn no_backup_options() -> &'static [BackupOption] {
    &[]
}
