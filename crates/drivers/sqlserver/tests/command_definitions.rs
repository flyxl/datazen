//! Driver Command API wiring for SQL Server (no live server).

use datazen_driver_api::DatabaseDriver;
use datazen_driver_sqlserver::SqlServerDriver;

#[test]
fn command_definitions_include_sql_and_admin_commands() {
    let ids: Vec<String> = SqlServerDriver::new()
        .command_definitions()
        .into_iter()
        .map(|d| d.id)
        .collect();
    for id in [
        "query",
        "execute",
        "query_stream",
        "create_database",
        "create_schema",
        "create_user",
        "drop_database",
    ] {
        assert!(ids.contains(&id.to_string()), "missing command {id}");
    }
}
