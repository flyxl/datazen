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

pub use types::*;
pub use traits::*;
pub use factory::*;

/// Protocol version for the driver API.
///
/// Bump this when making breaking changes to `DatabaseDriver`, `KeyValueDriver`,
/// or `DatabaseDriverFactory` traits. Plugins compiled against a different
/// protocol version will be rejected at startup.
pub const PROTOCOL_VERSION: u32 = 1;
