//! Host-side sync adapter leftovers.
//!
//! Concrete adapters live in path/git driver crates and self-register via inventory.

/// No residual host adapters; path/git drivers register via `inventory`.
#[inline(never)]
pub fn force_link() {}
