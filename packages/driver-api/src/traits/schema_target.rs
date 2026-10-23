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

/// Whether this driver has a **real schema level** in its namespace hierarchy.
///
/// `true` for engines whose relations are addressed as `schema.table`
/// (PostgreSQL, SQL Server, DuckDB). `false` for engines where the database
/// *is* the namespace (MySQL/MariaDB, ClickHouse) or that have no schema
/// concept at all (SQLite, Redis, MongoDB, …).
///
/// This drives [`validate_schema_target`]: a schema-aware driver **must**
/// receive `Some(schema)` (including the default `public`/`dbo`), and a
/// schema-less driver **must** receive `None`. Both mismatches are errors —
/// there is no implicit fallback, because an implicit schema is exactly the
/// kind of hidden session state that produced BUG-003.
///
/// The default is `false`: a driver that says nothing is treated as a
/// namespace-less engine.
pub(crate) fn has_schema_level() -> bool {
    false
}

/// Whether one connection can address more than one `database`.
///
/// Engines that resolve a relation against whatever database the session
/// landed on (PostgreSQL silently defaults to `postgres`) need the target
/// stated explicitly, because a missing value cannot be told apart from a
/// deliberate one. Drivers with a single fixed database return `false`.
///
/// The default is `false`: a driver that says nothing is treated as a
/// single-database engine.
pub(crate) fn has_multi_database() -> bool {
    false
}

/// Conventional schema this driver resolves unqualified relations in.
///
/// Used only as the last resort when a caller has **no** schema to offer
/// (a hand-typed MCP table name, a table name scraped out of SQL text) and
/// the driver declares `has_schema_level`. Every caller that already knows
/// the schema — the object tree, the ER diagram, schema diff, sync and
/// transfer all carry it on `TableInfo` — must pass that real value instead.
/// Keeping the fallback here rather than in the host is what stops `public` /
/// `dbo` from being hardcoded per driver outside the driver that owns the
/// convention.
///
/// The default is `None`: a driver that says nothing applies no implicit
/// schema.
pub(crate) fn default_schema() -> Option<&'static str> {
    None
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
