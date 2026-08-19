//! DataZen path driver: postgres

use std::sync::Arc;

use datazen_driver_api::*;

mod admin_commands;
mod postgres;
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
}
datazen_driver_api::register_driver!(&CloudberryFactory);
