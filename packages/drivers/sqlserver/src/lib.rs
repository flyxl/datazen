//! DataZen path driver: sqlserver

use std::sync::Arc;

use datazen_driver_api::*;

mod admin_commands;
mod metadata;
mod migration;
mod parameters;
mod sql_target;
mod sqlserver;
mod structure;
mod sync_adapter;
mod type_normalizer;
pub use migration::{SqlServerMigrationCapabilities, SqlServerMigrationRenderer};
pub use sqlserver::*;
pub use sync_adapter::SqlServerSyncAdapter;
pub use type_normalizer::SqlServerTypeNormalizer;

struct SqlServerFactory;
impl DatabaseDriverFactory for SqlServerFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(SqlServerDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "sqlserver"
    }
    fn supports_explain(&self) -> bool {
        true
    }
}
datazen_driver_api::register_driver!(&SqlServerFactory);
