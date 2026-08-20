//! Driver Command wiring for schema object browser queries.

use datazen_driver_api::{schema_object_command_definitions, DatabaseDriver};
use datazen_driver_postgres::PostgresDriver;

#[test]
fn command_definitions_include_schema_object_commands() {
    let driver = PostgresDriver::new();
    let defs = driver.command_definitions();
    let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
    for id in ["list_objects", "get_object_ddl", "list_privileges"] {
        assert!(ids.contains(&id), "missing command {id}");
    }
    assert!(
        schema_object_command_definitions()
            .iter()
            .all(|def| ids.contains(&def.id.as_str())),
        "driver should expose all shared schema object commands"
    );
}
