//! DataZen path driver: sqlserver

use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod admin_commands;
mod metadata;
mod migration;
mod parameters;
mod resource_provider;
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

    /// Track O: sqlserver now reaches a real provider, so
    /// [`DatabaseDriverFactory::require_resource_provider`] returns a handle-
    /// issuing provider instead of `ResourceProviderMissing`.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(resource_provider::resource_provider())
    }

    /// What this crate can honestly claim. It is byte-identical to the
    /// provider's own registry on purpose — see `resource_provider::capabilities`.
    fn resource_capabilities(&self) -> CapabilitySet {
        resource_provider::capabilities()
    }
}
datazen_driver_api::register_driver!(&SqlServerFactory);
