//! Observe-category Driver Commands (server status / row estimates).

use datazen_driver_api::DatabaseDriver;
use datazen_driver_postgres::PostgresDriver;

#[test]
fn command_definitions_include_observe_commands() {
    let driver = PostgresDriver::new();
    let defs = driver.command_definitions();
    let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
    for id in ["server_status_snapshot", "estimate_table_rows"] {
        assert!(ids.contains(&id), "missing command {id}");
    }
}
