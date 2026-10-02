//! DataZen path driver: clickhouse

use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod clickhouse;
mod resource_provider;
mod sql_target;
mod structure;
mod sync_adapter;
pub use clickhouse::*;
pub use sync_adapter::ClickHouseSyncAdapter;

struct ClickHouseFactory;
impl DatabaseDriverFactory for ClickHouseFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ClickHouseDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "clickhouse"
    }
    fn supports_explain(&self) -> bool {
        true
    }

    /// Track O: clickhouse now reaches a real provider, so
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
datazen_driver_api::register_driver!(&ClickHouseFactory);
