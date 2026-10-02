//! DataZen path driver: sqlite

use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod adb;
mod migration;
mod resource;
mod resource_capabilities;
mod sql_target;
mod sqlite;
mod structure;
mod sync_adapter;
mod type_normalizer;
pub use migration::{SqliteMigrationCapabilities, SqliteMigrationRenderer};
use resource::SqliteResourceProvider;
pub use sqlite::*;
pub use sync_adapter::SqliteSyncAdapter;
pub use type_normalizer::SqliteTypeNormalizer;

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
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(Arc::new(SqliteResourceProvider::new(
            self.create(),
            self.driver_id(),
        )))
    }
    fn resource_capabilities(&self) -> CapabilitySet {
        resource_capabilities::sqlite_capability_set()
    }
}
datazen_driver_api::register_driver!(&SqliteFactory);
