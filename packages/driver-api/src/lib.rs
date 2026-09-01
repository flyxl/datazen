//! Public API for DataZen database driver plugins.
//!
//! External drivers depend on this crate and implement [`DatabaseDriver`].
//! They register themselves at link time via the [`register_driver!`] macro,
//! which uses the [`inventory`] crate for zero-code discovery in the host binary.

pub use async_trait::async_trait;
pub use inventory;

pub mod command;
mod explain_plan;
mod factory;
mod query_stream;
mod reuse;
pub mod schema_catalog_commands;
pub mod schema_object_commands;
pub mod schema_objects;
pub mod sql_dump;
pub mod sql_split;
pub mod sql_target;
pub mod sqlite_structure;
pub mod sync;
mod traits;
mod types;

pub use command::{
    check_command_access, execute_command_definition, execute_command_definition_for,
    query_command_definition, query_command_definition_for, query_only_command_definitions,
    query_stream_command_definition, required_access_level, statement_command_definitions,
    validate_command_input, CommandAccessLevel, CommandCategory, CommandResult,
    DriverCommandDefinition, DriverCommandMetadata, DriverSaveDialogSpec,
};
pub use explain_plan::{normalize_mysql_explain_plan, normalize_postgres_explain_plan};
pub use factory::*;
pub use query_stream::{
    append_select_limit, emit_execute_statement, emit_multi_query_as_stream, stream_decoded_rows,
    QueryRowBatcher, QueryStreamCallback, QueryStreamEvent, QUERY_STREAM_BATCH_SIZE,
};
pub use reuse::ReuseDriver;
pub use schema_catalog_commands::{
    execute_schema_catalog_command, is_schema_catalog_command, parse_databases_from_command,
    parse_table_schema_from_command, parse_tables_from_command, schema_catalog_command_definitions,
    try_execute_schema_catalog_command,
};
pub use schema_object_commands::{
    execute_schema_object_command, is_schema_object_command, schema_object_command_definitions,
};
pub use schema_objects::{
    dialect_family, list_objects_sql, list_privileges_sql, object_ddl_sql, DatabaseObject,
    ObjectKind, PrivilegeGrant,
};
pub use sql_dump::{RestoreSession, RestoreStatementGuard};
pub use sql_split::{SqlStatementScanner, Utf8ChunkDecoder};
pub use sql_target::{qualify_sql_with, QualifiedSql, QualifierQuote, SqlTarget};
pub use sync::{
    BoxedSyncAdapter, IRColumn, IRDefault, IRTable, IRType, SyncAdapterFactory, SyncSourceAdapter,
    SyncTargetAdapter,
};
pub use traits::*;
pub use types::*;

/// Protocol version for the driver API.
///
/// Bump this when making breaking changes to `DatabaseDriver`, `KeyValueDriver`,
/// or `DatabaseDriverFactory` traits.
pub const PROTOCOL_VERSION: u32 = 3;

/// Minimum protocol version the host still supports.
///
/// Plugins with version < MIN will be rejected; those between MIN and current
/// will run in degraded mode (missing capabilities default to `false`).
pub const MIN_PROTOCOL_VERSION: u32 = 1;
