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
