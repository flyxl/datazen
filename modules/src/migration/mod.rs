//! Data migration package.
//!
//! The four migration engines share one package boundary while remaining
//! independently feature-gated: `common`, `data_sync`, `data_transfer`, and
//! `schema_diff`.

#[cfg(feature = "common")]
pub mod common;
#[cfg(feature = "data_sync")]
pub mod data_sync;
#[cfg(feature = "data_transfer")]
pub mod data_transfer;
#[cfg(feature = "schema_diff")]
pub mod schema_diff;
