//! Driver Command API wiring for ClickHouse (no live server).

use datazen_driver_api::DatabaseDriver;
use datazen_driver_clickhouse::ClickHouseDriver;

#[test]
fn command_definitions_include_sql_commands() {
    let ids: Vec<String> = ClickHouseDriver::new()
        .command_definitions()
        .into_iter()
        .map(|d| d.id)
        .collect();
    for id in ["query", "execute", "query_stream"] {
        assert!(ids.contains(&id.to_string()), "missing command {id}");
    }
}
