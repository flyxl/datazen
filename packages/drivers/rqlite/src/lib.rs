//! DataZen path driver: rqlite

use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod resource_provider;
mod rqlite;
pub use rqlite::*;

struct RqliteFactory;
impl DatabaseDriverFactory for RqliteFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(RqliteDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "rqlite"
    }
    fn supports_explain(&self) -> bool {
        true
    }

    /// Track O: rqlite now reaches a real provider, so
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
datazen_driver_api::register_driver!(&RqliteFactory);
