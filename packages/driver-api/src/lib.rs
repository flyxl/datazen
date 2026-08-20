//! Public API for DataZen database driver plugins.
//!
//! External drivers depend on this crate and implement [`DatabaseDriver`].
//! They register themselves at link time via the [`register_driver!`] macro,
//! which uses the [`inventory`] crate for zero-code discovery in the host binary.

pub use async_trait::async_trait;
pub use inventory;

pub mod command;
mod factory;
mod query_stream;
mod reuse;
pub mod schema_object_commands;
pub mod schema_objects;
pub mod sql_dump;
pub mod sql_split;
pub mod sync;
mod traits;
mod types;

pub use command::{
    check_command_access, execute_command_definition, execute_command_definition_for,
    query_command_definition, query_command_definition_for, query_only_command_definitions,
    required_access_level, statement_command_definitions, validate_command_input,
    CommandAccessLevel, CommandCategory, CommandResult, DriverCommandDefinition,
    DriverCommandMetadata,
};
pub use factory::*;
pub use query_stream::{
    append_select_limit, emit_execute_statement, emit_multi_query_as_stream, stream_decoded_rows,
    QueryRowBatcher, QueryStreamCallback, QueryStreamEvent, QUERY_STREAM_BATCH_SIZE,
};
pub use reuse::ReuseDriver;
pub use schema_object_commands::{
    execute_schema_object_command, is_schema_object_command, schema_object_command_definitions,
};
pub use schema_objects::{
    dialect_family, list_objects_sql, list_privileges_sql, object_ddl_sql, DatabaseObject,
    ObjectKind, PrivilegeGrant,
};
pub use sql_dump::{RestoreSession, RestoreStatementGuard};
pub use sql_split::{SqlStatementScanner, Utf8ChunkDecoder};
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
pub const PROTOCOL_VERSION: u32 = 2;

/// Minimum protocol version the host still supports.
///
/// Plugins with version < MIN will be rejected; those between MIN and current
/// will run in degraded mode (missing capabilities default to `false`).
pub const MIN_PROTOCOL_VERSION: u32 = 1;
