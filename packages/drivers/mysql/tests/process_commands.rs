use datazen_driver_api::DatabaseDriver;
use datazen_driver_mysql::MysqlDriver;

#[test]
fn command_definitions_include_process_commands() {
    let driver = MysqlDriver::new(false);
    let defs = driver.command_definitions();
    let ids: Vec<&str> = defs.iter().map(|d| d.id.as_str()).collect();
    assert!(ids.contains(&"list_processes"));
    assert!(ids.contains(&"kill_process"));
}
