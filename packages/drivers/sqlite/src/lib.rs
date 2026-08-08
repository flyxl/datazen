//! DataZen path driver: sqlite

use std::sync::Arc;

use datazen_driver_api::*;

mod sqlite;
pub use sqlite::*;

struct SqliteFactory;
impl DatabaseDriverFactory for SqliteFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(SqliteDriver::new())
    }
    fn driver_id(&self) -> &'static str { "sqlite" }
    fn supports_explain(&self) -> bool { true }
}
datazen_driver_api::register_driver!(&SqliteFactory);
