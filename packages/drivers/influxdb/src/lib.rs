//! DataZen path driver: influxdb

use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod influxdb;
mod resource_provider;
mod sync_adapter;
pub use influxdb::*;
pub use sync_adapter::InfluxDbSyncAdapter;

struct InfluxDbFactory;
impl DatabaseDriverFactory for InfluxDbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(InfluxDbDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "influxdb"
    }

    /// Track O: influxdb now reaches a real provider, so
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
datazen_driver_api::register_driver!(&InfluxDbFactory);
