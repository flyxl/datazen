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

pub use types::*;
pub use traits::*;
pub use factory::*;
pub use reuse::ReuseDriver;

/// Protocol version for the driver API.
///
/// Bump this when making breaking changes to `DatabaseDriver`, `KeyValueDriver`,
/// or `DatabaseDriverFactory` traits.
pub const PROTOCOL_VERSION: u32 = 1;

/// Minimum protocol version the host still supports.
///
/// Plugins with version < MIN will be rejected; those between MIN and current
/// will run in degraded mode (missing capabilities default to `false`).
pub const MIN_PROTOCOL_VERSION: u32 = 1;
