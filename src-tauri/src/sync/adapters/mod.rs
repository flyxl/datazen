//! Residual host sync adapters (no path driver crate yet).

pub mod trino;

/// Ensure adapter modules stay linked so their `inventory::submit!` statics are present.
#[inline(never)]
pub fn force_link() {
    let _ = std::any::type_name::<trino::TrinoSyncAdapter>();
}

#[cfg(test)]
mod roundtrip_tests;
