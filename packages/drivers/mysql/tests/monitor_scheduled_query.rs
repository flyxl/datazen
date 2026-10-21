//! Live Monitor policy → Workflow resource execution → real mysql provider.
//! This slice does not cover the desktop scheduler or SQLite occurrence receipts.
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;
#[path = "../../http-support/tests/support/monitor_journey.rs"]
mod monitor_journey;
#[tokio::test]
async fn monitor_grant_read_only_query_runs_through_runtime_and_real_provider() {
    let Some(config) = migration_gate::require_config(
        "mysql",
        "monitor-scheduled-query",
        "mysql",
        "monitor-live",
        &[],
    ) else {
        return;
    };
    let factory = datazen_driver_api::iter_driver_factories()
        .into_iter()
        .find(|factory| factory.driver_id() == "mysql")
        .unwrap();
    monitor_journey::run(
        config,
        std::sync::Arc::new(datazen_driver_mysql::MysqlDriver::new(false)),
        datazen_driver_api::require_resource_provider(*factory).unwrap(),
    )
    .await;
}
