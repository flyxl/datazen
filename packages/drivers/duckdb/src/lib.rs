//! DataZen path driver: duckdb

use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod duckdb;
mod resource_provider;
mod sql_target;
mod structure;
mod sync_adapter;
pub use duckdb::*;
pub use sync_adapter::DuckDbSyncAdapter;

struct DuckDbFactory;
impl DatabaseDriverFactory for DuckDbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(DuckDbDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "duckdb"
    }
    fn supports_explain(&self) -> bool {
        true
    }

    /// Track O: duckdb now reaches a real provider, so
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
datazen_driver_api::register_driver!(&DuckDbFactory);
