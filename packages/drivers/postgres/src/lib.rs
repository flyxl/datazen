//! DataZen path driver: postgres

use std::sync::Arc;

use datazen_driver_api::*;

mod admin_commands;
mod postgres;
mod sql_target;
mod structure;
mod sync_adapter;
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
        Arc::new(ReuseDriver::new(Arc::new(PostgresDriver::new()), "questdb"))
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
}
datazen_driver_api::register_driver!(&QuestDbFactory);

struct CloudberryFactory;
impl DatabaseDriverFactory for CloudberryFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ReuseDriver::new(
            Arc::new(PostgresDriver::new()),
            "cloudberry",
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
}
datazen_driver_api::register_driver!(&CloudberryFactory);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_factory_advertises_precise_cancellation_only_for_native_postgres() {
        let factories: [&dyn DatabaseDriverFactory; 3] =
            [&PostgresFactory, &QuestDbFactory, &CloudberryFactory];

        assert!(factories[0].supports_cancel_query());
        assert!(factories[0].supports_query_execution_cancel());
        for factory in &factories[1..] {
            assert!(!factory.supports_cancel_query());
            assert!(!factory.supports_query_execution_cancel());
        }
    }
}
