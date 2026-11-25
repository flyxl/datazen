//! Ports of `packages/frontend/src/lib/sqlDialects/` — dialect metadata the
//! editor, the Visual Query Builder and the host's strategy registry share
//! (dialectProfile and extra landed first; T123 adds the strategy types,
//! the base table-SQL generator, rowLimit and the postgresql/mysql/sqlite
//! dialects; identifierQuoting and the index registry land with T084;
//! T155 adds sqlParameterPolicy, the bind-placeholder resolution).
//!
//! [`get_builtin_dialect_strategy`] is this module's `index.ts` half: the
//! `BUILTIN_DIALECTS` table React's `getSqlDialect` reads after the
//! registry lookup. The registry itself (`DB_REGISTRY` from
//! `lib/databaseTypes.ts`, generated from driver manifests) and
//! `DRIVER_SQL_DIALECTS` (`extensions/generated`, never committed) are
//! host seams — the host resolves `meta.sqlDialectStrategy` / the
//! `sqlDialect` family and calls here for the built-in fallback, exactly
//! the pattern `dialect_profile::resolve_dialect_profile` established.

pub mod base_table_sql;
pub mod dialect_profile;
pub mod extra;
pub mod identifier_quoting;
pub mod mysql;
pub mod postgresql;
pub mod row_limit;
pub mod sql_parameter_policy;
pub mod sqlite;
pub mod strategy;

pub use base_table_sql::BaseTableSqlGenerator;
pub use extra::{
    CLICKHOUSE_DIALECT, DUCKDB_DIALECT, ELASTICSEARCH_DIALECT, GENERIC_DIALECT, MONGODB_DIALECT,
    ORACLE_DIALECT, SQLSERVER_DIALECT,
};
pub use identifier_quoting::{
    SQL_RESERVED_IDENTIFIER_KEYWORDS, fold_identifier, is_reserved_keyword, needs_quoting,
    quote_identifier_for_dialect, wrap_quoted,
};
pub use mysql::MYSQL_DIALECT;
pub use postgresql::POSTGRESQL_DIALECT;
pub use row_limit::{build_row_limited_select, RowLimitedSelectParts, LIMIT_ROW_LIMIT};
pub use sql_parameter_policy::{
    resolve_sql_parameter_policy, SqlParameterMeta, SqlParameterOccurrence, SqlParameterRegistry,
    SqlParameterStrategy, SqlParamDialectPolicy, SqlParamSyntax, GENERIC_SQL_PARAMETER_POLICY,
};
pub use sqlite::SQLITE_DIALECT;
pub use strategy::{
    BackupOption, CreateIndexOptions, DdlQuery, DialectDdl, DialectIndex, GeneratedSqlType,
    IndexMethod, RowLimitPlacement, RowLimitShape, SqlDialectStrategy,
};

/// React `index.ts`'s `BUILTIN_DIALECTS` — the built-in half of
/// `getSqlDialect`'s `DIALECTS` table (`DRIVER_SQL_DIALECTS` is the
/// host-resolved half; a driver's own strategy wins over these).
pub fn get_builtin_dialect_strategy(family: &str) -> Option<&'static SqlDialectStrategy> {
    match family {
        "postgresql" => Some(&POSTGRESQL_DIALECT),
        "mysql" => Some(&MYSQL_DIALECT),
        "sqlite" => Some(&SQLITE_DIALECT),
        "sqlserver" => Some(&SQLSERVER_DIALECT),
        "clickhouse" => Some(&CLICKHOUSE_DIALECT),
        "duckdb" => Some(&DUCKDB_DIALECT),
        "elasticsearch" => Some(&ELASTICSEARCH_DIALECT),
        "mongodb" => Some(&MONGODB_DIALECT),
        "generic" => Some(&GENERIC_DIALECT),
        "oracle" => Some(&ORACLE_DIALECT),
        _ => None,
    }
}

/// The `BUILTIN_DIALECTS` keys, in React's declaration order (the
/// coordinator's audit pin; unknown families resolve to `None`).
pub const BUILTIN_DIALECT_FAMILIES: &[&str] = &[
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
];
