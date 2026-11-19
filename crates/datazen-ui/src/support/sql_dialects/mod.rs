//! Ports of `packages/frontend/src/lib/sqlDialects/` — dialect metadata the
//! editor, the Visual Query Builder and the host's strategy registry share
//! (dialectProfile and extra landed first; T123 adds the strategy types,
//! the base table-SQL generator, rowLimit and the postgresql/mysql/sqlite
//! dialects; identifierQuoting and the index registry land with T084).

pub(crate) mod base_table_sql;
pub(crate) mod dialect_profile;
pub(crate) mod extra;
pub(crate) mod mysql;
pub(crate) mod postgresql;
pub(crate) mod row_limit;
pub(crate) mod sqlite;
pub(crate) mod strategy;

pub(crate) use base_table_sql::BaseTableSqlGenerator;
pub(crate) use mysql::MYSQL_DIALECT;
pub(crate) use postgresql::POSTGRESQL_DIALECT;
pub(crate) use row_limit::{build_row_limited_select, RowLimitedSelectParts, LIMIT_ROW_LIMIT};
pub(crate) use sqlite::SQLITE_DIALECT;
pub(crate) use strategy::{
    BackupOption, CreateIndexOptions, DdlQuery, DialectDdl, DialectIndex, GeneratedSqlType,
    IndexMethod, RowLimitPlacement, RowLimitShape, SqlDialectStrategy,
};
