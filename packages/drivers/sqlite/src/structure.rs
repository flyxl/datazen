//! Table structure editor for SQLite — delegates to the shared SQLite-family
//! planner in driver-api (also used by RQLite and Turso/libSQL).

use datazen_driver_api::{
    sqlite_structure, DriverError, StructureCapabilities, StructureChangePlan,
    StructureChangeRequest,
};

/// Static capability flags for SQLite (additive ALTER + index ops only).
pub fn capabilities(dialect_id: &str) -> StructureCapabilities {
    sqlite_structure::capabilities(dialect_id)
}

/// Plan DDL statements from a structure change request.
pub fn plan_changes(
    request: &StructureChangeRequest,
    caps: &StructureCapabilities,
) -> Result<StructureChangePlan, DriverError> {
    sqlite_structure::plan_changes(request, caps)
}
