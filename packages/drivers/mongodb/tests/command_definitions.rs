//! Driver Command API wiring for MongoDB (no live server).

use datazen_driver_api::DatabaseDriver;
use datazen_driver_mongodb::MongodbDriver;

#[test]
fn command_definitions_include_json_query_commands() {
    let ids: Vec<String> = MongodbDriver::new()
        .command_definitions()
        .into_iter()
        .map(|d| d.id)
        .collect();
    for id in ["query", "execute", "query_stream"] {
        assert!(ids.contains(&id.to_string()), "missing command {id}");
    }
}
