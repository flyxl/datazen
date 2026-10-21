//! Thin adapter for the shared Data Sync command contract.
//!
//! The request/response shapes of the Data Sync commands belong to the
//! feature itself, so they live in `datazen_data_sync::api`; the desktop
//! host only re-exports them.

pub use datazen_data_sync::api::*;
