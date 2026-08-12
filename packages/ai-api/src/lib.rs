//! DataZen AI Provider API
//!
//! Public crate defining the `AiProvider` trait and related types.
//! Mirrors `datazen-driver-api` for database drivers.

pub const AI_PROTOCOL_VERSION: u32 = 1;

/// Minimum AI protocol version the host still supports.
///
/// Plugins with version < MIN will be rejected; those between MIN and current
/// will run in degraded mode (missing capabilities default to `false`).
pub const MIN_AI_PROTOCOL_VERSION: u32 = 1;

mod factory;
mod traits;
mod types;

pub use factory::*;
pub use traits::*;
pub use types::*;

pub use async_trait::async_trait;
pub use inventory;
