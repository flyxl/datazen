//! DataZen path driver: elasticsearch

use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod elasticsearch;
mod resource_provider;
mod sync_adapter;
pub use elasticsearch::*;
pub use sync_adapter::ElasticsearchSyncAdapter;

struct ElasticsearchFactory;
impl DatabaseDriverFactory for ElasticsearchFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(ElasticsearchDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "elasticsearch"
    }

    /// Track O: elasticsearch now reaches a real provider, so
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
datazen_driver_api::register_driver!(&ElasticsearchFactory);
