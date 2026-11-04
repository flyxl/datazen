//! SQL Server Schema Diff rendering.
//!
//! Only the catalog features represented by the shared migration IR are
//! emitted. Metadata that cannot be represented, such as filtered or
//! descending indexes, is rejected by the SQL Server catalog reader before a
//! plan can be produced.

use datazen_driver_api::*;
pub struct SqlServerMigrationRenderer;

pub struct SqlServerMigrationCapabilities;

mod renderer;
mod sql;
mod validation;
mod views;

#[cfg(test)]
#[path = "migration/tests.rs"]
mod tests;
