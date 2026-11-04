//! Driver Command API wiring for DuckDB (no live server).

use datazen_driver_api::{schema_object_command_definitions, DatabaseDriver};
use datazen_driver_duckdb::DuckDbDriver;

#[test]
fn command_definitions_include_sql_and_schema_object_commands() {
    let ids: Vec<String> = DuckDbDriver::new()
        .command_definitions()
        .into_iter()
        .map(|d| d.id)
        .collect();
    for id in ["query", "execute", "query_stream"] {
        assert!(ids.contains(&id.to_string()), "missing command {id}");
    }
    for def in schema_object_command_definitions() {
        assert!(
            ids.contains(&def.id),
            "missing schema object command {}",
            def.id
        );
    }
}
