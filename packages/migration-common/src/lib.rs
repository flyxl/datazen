//! Pure algorithms shared by migration engines.
//!
//! No job runtime, host, transport or concrete driver dependencies. Each domain
//! owns its plans, validation policy and execution; this crate only supplies
//! typed recordset bounds and endpoint category/family classification.

pub mod pairing;
pub mod recordset_bounds;
