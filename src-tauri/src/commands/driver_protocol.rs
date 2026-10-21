//! Driver protocol version lookup, shared by the migration wizards.
//!
//! The lookup lives in `datazen_driver_api::driver_protocol_version` next to
//! the factory registry it reads; this shim keeps the host-side import paths
//! (`crate::commands::driver_protocol`) stable. Data Sync and Data Transfer
//! must answer the same number from the same source.

pub(crate) use datazen_driver_api::driver_protocol_version;
