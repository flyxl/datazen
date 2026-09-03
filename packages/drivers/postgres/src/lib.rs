//! DataZen path driver: postgres

use std::sync::Arc;

use datazen_driver_api::*;

mod admin_commands;
mod migration;
mod postgres;
mod sql_target;
mod structure;
mod sync_adapter;
pub use migration::PostgresMigrationRenderer;
pub use postgres::*;
pub use structure::{caps_for_version, plan_structure_changes_with_caps};
pub use sync_adapter::PgSyncAdapter;

struct PostgresFactory;
impl DatabaseDriverFactory for PostgresFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(PostgresDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "postgresql"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    fn supports_cancel_query(&self) -> bool {
        true
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&PostgresFactory);

struct QuestDbFactory;
impl DatabaseDriverFactory for QuestDbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new_with_precise_cancel(
            Arc::new(PostgresDriver::new()),
            "questdb",
            true,
        ))
    }
    fn driver_id(&self) -> &'static str {
        "questdb"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    fn supports_cancel_query(&self) -> bool {
        false
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&QuestDbFactory);

struct CloudberryFactory;
impl DatabaseDriverFactory for CloudberryFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        // Cloudberry is a PostgreSQL derivative and uses the same backend PID
        // and pg_cancel_backend control protocol as native PostgreSQL.
        Arc::new(ReuseDriver::new_with_precise_cancel(
            Arc::new(PostgresDriver::new()),
            "cloudberry",
            true,
        ))
    }
    fn driver_id(&self) -> &'static str {
        "cloudberry"
    }
    fn supports_explain(&self) -> bool {
        true
    }
    fn supports_cancel_query(&self) -> bool {
        false
    }
    fn supports_query_execution_cancel(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&CloudberryFactory);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_factory_advertises_precise_cancellation_by_backend_compatibility() {
        let factories: [&dyn DatabaseDriverFactory; 3] =
            [&PostgresFactory, &QuestDbFactory, &CloudberryFactory];

        assert!(factories[0].supports_cancel_query());
        assert!(factories[0].supports_query_execution_cancel());
        assert!(!factories[1].supports_cancel_query());
        assert!(factories[1].supports_query_execution_cancel());
        assert!(!factories[2].supports_cancel_query());
        assert!(factories[2].supports_query_execution_cancel());

        assert!(QuestDbFactory.create().supports_query_execution_cancel());
        assert!(CloudberryFactory.create().supports_query_execution_cancel());
    }
}
