//! The schema-dimension rule every driver call site has to apply.
//!
//! Listing operations legitimately span every schema in a database, while
//! single-table resolution must be exact. The validation lives beside its scope
//! enum instead of inside the trait, because it is a free function that drivers
//! call directly rather than a default body they override.

use super::DatabaseDriver;
use crate::types::DriverError;

/// How precisely a call site must pin the schema dimension.
///
/// Listing operations legitimately span every schema in a database (the
/// connection tree groups tables by their own schema), while single-table
/// resolution must be exact — an ambiguous table identity is what allowed
/// same-named tables from different schemas to be merged into one column set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaScope {
    /// Listing call: `None` means "every schema in the database".
    AnySchema,
    /// Resolution call: a schema-aware driver must receive `Some(schema)`.
    ExactSchema,
}

/// Validate the `(database, schema)` target pair against a driver's capability.
///
/// **This is the single source of truth for the schema-dimension rule**, and it
/// is deliberately a free function: the trait method is the only chokepoint
/// shared by *every* caller (GUI IPC, MCP server, Workflow, `sql_dump`, reuse
/// wrappers), so drivers must call it at the top of their
/// `get_tables`/`get_table_schema`/`get_columns`/`get_all_columns`
/// implementations. Host call sites may call it too, purely to fail earlier
/// with a friendlier message — that is an optimization, not the guarantee.
///
/// | `has_schema_level()` | `schema` | [`SchemaScope::AnySchema`] | [`SchemaScope::ExactSchema`] |
/// | --- | --- | --- | --- |
/// | `true` | `Some(s)` non-blank | `Ok` | `Ok` |
/// | `true` | `None` / blank | `Ok` (all schemas) | `Err(InvalidConfig)` |
/// | `false` | `None` / blank | `Ok` | `Ok` |
/// | `false` | `Some(s)` | `Err(InvalidConfig)` | `Err(InvalidConfig)` |
///
/// `database` is intentionally not validated here: engines differ on whether a
/// blank database means "the session's current catalog" (PostgreSQL's listing
/// path) or is simply invalid.
pub fn validate_schema_target<D: DatabaseDriver + ?Sized>(
    driver: &D,
    database: &str,
    schema: Option<&str>,
    scope: SchemaScope,
) -> Result<(), DriverError> {
    let schema = schema.map(str::trim).filter(|s| !s.is_empty());
    let driver_type = driver.driver_type();
    match (driver.has_schema_level(), schema, scope) {
        (false, Some(schema), _) => Err(DriverError::InvalidConfig(format!(
            "driver '{driver_type}' has no schema level: schema '{schema}' is not allowed \
             (database '{database}')"
        ))),
        (true, None, SchemaScope::ExactSchema) => Err(DriverError::InvalidConfig(format!(
            "driver '{driver_type}' has a schema level: an explicit schema is required \
             (database '{database}')"
        ))),
        _ => Ok(()),
    }
}
