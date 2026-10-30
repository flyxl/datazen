//! Dependency-free SQL text helpers for DataZen drivers and hosts.
//!
//! Host-boundary F3, first slice: [`sql_split`] and [`sql_identifiers`]
//! moved here out of `driver-api`. Both modules are pure string/byte
//! scanning with no crate or external dependencies, so driver plugins can
//! take the statement scanner and identifier quoting without pulling in the
//! full protocol crate. `driver-api` re-exports both modules — existing
//! `datazen_driver_api::sql_split` / `::sql_identifiers` paths keep
//! resolving — and [`datazen_driver_api::PROTOCOL_VERSION`] is untouched.
//!
//! Anything that needs `Value`, `DatabaseDriver` or serde stays in
//! `driver-api`; only provably pure text helpers move here.

pub mod sql_identifiers;
pub mod sql_split;
