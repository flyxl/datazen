//! Public API for DataZen database driver plugins.
//!
//! External drivers depend on this crate and implement [`DatabaseDriver`].
//! They register themselves at link time via the [`register_driver!`] macro,
//! which uses the [`inventory`] crate for zero-code discovery in the host binary.

pub use async_trait::async_trait;
pub use inventory;

mod types;
mod traits;
mod factory;
mod reuse;
pub mod command;
pub mod sql_dump;
pub mod sync;

pub use types::*;
pub use traits::*;
pub use factory::*;
pub use reuse::ReuseDriver;
pub use command::{
    check_command_access, execute_command_definition, query_command_definition,
    required_access_level, validate_command_input, CommandAccessLevel, CommandResult,
    DriverCommandDefinition,
};
pub use sync::{
    BoxedSyncAdapter, IRColumn, IRDefault, IRTable, IRType, SyncAdapterFactory, SyncSourceAdapter,
    SyncTargetAdapter,
};

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
