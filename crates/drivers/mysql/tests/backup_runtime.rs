//! Live retained SQL backup; not a native desktop/picker E2E.
#[path = "../../http-support/tests/support/backup_journey.rs"]
mod backup_journey;
#[path = "../../http-support/tests/support/migration_gate.rs"]
mod migration_gate;
#[tokio::test]
async fn retained_backup_streams_gzip_and_cleans_cancelled_or_oversized_output() {
    let Some(config) =
        migration_gate::require_config("mysql", "backup-runtime", "mysql", "backup-live", &[])
    else {
        return;
    };
    let driver = std::sync::Arc::new(datazen_driver_mysql::MysqlDriver::new(false));
    let factory = datazen_driver_api::iter_driver_factories()
        .into_iter()
        .find(|f| f.driver_id() == "mysql")
        .unwrap();
    backup_journey::run(
        config,
        driver,
        datazen_driver_api::require_resource_provider(*factory).unwrap(),
    )
    .await;
}
