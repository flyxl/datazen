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

/// The schema embedded in a qualified table reference (`sales.orders` → `sales`).
///
/// Metadata callers may pass either an explicit `schema` argument or a
/// qualified `table`; the driver resolves the embedded form itself, so the host
/// must not shadow it with a fallback default.
///
/// Shared with the host so exactly one implementation exists: a second copy in
/// a host module is how "the table is in `public`" and "the table is in
/// `sales`" drift apart.
pub fn embedded_schema_of(table: &str) -> Option<&str> {
    let (prefix, _) = table.rsplit_once('.')?;
    let prefix = prefix.trim().trim_matches('"');
    (!prefix.is_empty()).then_some(prefix)
}

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

/// The database argument for a session-bound, table-targeted read.
///
/// Priority: `requested` (trimmed, non-blank) → the connection profile's
/// configured database → `"default"`. This is the ladder the row-estimate
/// pipeline and the grid's own reads climb, so a table opened from the object
/// tree and the same table estimated or searched can never land on two
/// databases. Shared with the host so exactly one implementation exists —
/// a second copy is how "the table is in `app`" and "the table is in
/// `default`" drift apart.
pub fn table_read_database<'a>(
    requested: Option<&'a str>,
    config_database: Option<&'a str>,
) -> &'a str {
    requested
        .map(str::trim)
        .filter(|database| !database.is_empty())
        .or(config_database)
        .unwrap_or("default")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_read_database_prefers_the_request_then_the_profile() {
        assert_eq!(
            table_read_database(Some("analytics"), Some("app")),
            "analytics"
        );
        assert_eq!(table_read_database(None, Some("app")), "app");
        assert_eq!(table_read_database(Some("  "), Some("app")), "app");
        assert_eq!(table_read_database(None, None), "default");
    }
}
