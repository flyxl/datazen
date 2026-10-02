//! DataZen path driver: turso

use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod resource_provider;
mod turso;
pub use turso::*;

struct TursoFactory;
impl DatabaseDriverFactory for TursoFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(TursoDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "turso"
    }
    fn supports_explain(&self) -> bool {
        true
    }

    /// Track O: turso now reaches a real provider, so
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
datazen_driver_api::register_driver!(&TursoFactory);
