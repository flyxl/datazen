//! Database driver abstraction and shared types.
//!
//! Types and traits are re-exported from `datazen_driver_api` so that the rest
//! of the main crate can continue using `crate::db::*` unchanged.
//! Concrete drivers live in optional `packages/drivers/*` crates and register
//! via inventory.

pub use datazen_driver_api::*;

pub mod registry;

pub use registry::{init_drivers, DriverCapabilities, DriverRegistry};
