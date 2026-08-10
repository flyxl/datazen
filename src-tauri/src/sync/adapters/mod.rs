//! Host-side sync adapter leftovers (roundtrip tests only).
//!
//! Concrete adapters live in path/git driver crates and self-register via inventory.

/// No residual host adapters; path/git drivers register via `inventory`.
#[inline(never)]
pub fn force_link() {}

#[cfg(test)]
mod roundtrip_tests;
