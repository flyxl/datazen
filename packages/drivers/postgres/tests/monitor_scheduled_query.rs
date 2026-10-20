//! Live Monitor policy → Workflow resource execution → real postgresql provider.
//! This slice does not cover the desktop scheduler or SQLite occurrence receipts.
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;
#[path = "../../http-support/tests/support/monitor_journey.rs"]
mod monitor_journey;
#[tokio::test]
async fn monitor_grant_read_only_query_runs_through_runtime_and_real_provider() {
    let Some(mut config) = migration_gate::require_config(
        "postgresql",
        "monitor-scheduled-query",
        "postgresql",
        "monitor-live",
        &[],
    ) else {
        return;
    };
    config.schema = Some("public".into());
    let factory = datazen_driver_api::iter_driver_factories()
        .into_iter()
        .find(|factory| factory.driver_id() == "postgresql")
        .unwrap();
    monitor_journey::run(
        config,
        std::sync::Arc::new(datazen_driver_postgres::PostgresDriver::new()),
        datazen_driver_api::require_resource_provider(*factory).unwrap(),
    )
    .await;
}
