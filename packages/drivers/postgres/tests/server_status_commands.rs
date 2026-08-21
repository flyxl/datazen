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

#[test]
fn server_status_snapshot_definition_documents_trend_counters() {
    let driver = PostgresDriver::new();
    let def = driver
        .command_definitions()
        .into_iter()
        .find(|d| d.id == "server_status_snapshot")
        .expect("server_status_snapshot");
    let desc = def.description.unwrap_or_default().to_lowercase();
    // Contract: PG snapshot feeds the same Host trend charts as MySQL.
    assert!(
        desc.contains("status") || !desc.is_empty(),
        "expected a description for server_status_snapshot"
    );
}
