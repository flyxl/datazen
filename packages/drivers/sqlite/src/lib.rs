//! DataZen path driver: sqlite

use std::sync::Arc;

use datazen_driver_api::*;

mod adb;
mod sql_target;
mod sqlite;
mod migration;
mod structure;
mod sync_adapter;
pub use sqlite::*;
pub use migration::SqliteMigrationRenderer;
pub use sync_adapter::SqliteSyncAdapter;

struct SqliteFactory;
impl DatabaseDriverFactory for SqliteFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(SqliteDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "sqlite"
    }
    fn supports_explain(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&SqliteFactory);
