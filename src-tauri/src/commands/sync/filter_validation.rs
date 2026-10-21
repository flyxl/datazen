//! Thin adapter for the shared Data Sync filter validation rules.
//!
//! The validation itself (filter schemas, key contracts, endpoint rules) is
//! feature logic and lives in `datazen_data_sync::filter_validation`; the
//! desktop host only re-exports it.

pub use datazen_data_sync::filter_validation::*;
