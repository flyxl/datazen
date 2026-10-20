//! Resolving the `schema` argument for driver metadata calls.
//!
//! Every driver metadata entry point (`get_tables`, `get_table_schema`,
//! `get_columns`, `get_all_columns`, `dump_table_ddl`, `dump_view_ddl`) takes an
//! explicit `(database, schema)` target instead of relying on session state.
//! Drivers that declare [`DatabaseDriver::has_schema_level`] reject a missing
//! schema outright, so every host call site has to supply one.
//!
//! Call sites fall into two groups:
//!
//! * **They already know the schema** — anything that walked `get_tables` has
//!   it on `TableInfo::schema` (object tree, ER diagram, schema diff, data sync,
//!   data transfer, backup). These pass it straight through.
//! * **They do not** — a hand-typed MCP table name, or a table name scraped out
//!   of SQL text. For those, [`metadata_schema`] falls back to the connection
//!   config's schema and finally to the driver's own convention
//!   ([`DatabaseDriver::default_schema`]), so the host never hardcodes `public`
//!   or `dbo`.
//!
//! Schema-less drivers (`has_schema_level() == false`) always end up with
//! `None`, which is what their `validate_schema_target` requires.

use datazen_driver_api::DatabaseDriver;

/// The schema argument to use for a metadata call.
///
/// Priority: `explicit` → `table_schema` → `config_schema` → the driver's
/// [`default_schema`](DatabaseDriver::default_schema). Blank/whitespace values
/// are treated as absent at every level.
///
/// Returns `None` for a driver without a schema level, which is exactly what
/// `validate_schema_target(..., AnySchema)` and `ExactSchema` expect from such a
/// driver.
pub fn metadata_schema(
    driver: &dyn DatabaseDriver,
    explicit: Option<&str>,
    table_schema: Option<&str>,
    config_schema: Option<&str>,
) -> Option<String> {
    if !driver.has_schema_level() {
        return None;
    }
    [explicit, table_schema, config_schema]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|candidate| !candidate.is_empty())
        .map(str::to_string)
        .or_else(|| driver.default_schema().map(str::to_string))
}
