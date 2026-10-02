//! DataZen path driver: mongodb

use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod mongodb;
mod resource;
mod resource_capabilities;
mod sync_adapter;
pub use mongodb::*;
use resource::MongodbResourceProvider;
pub use sync_adapter::MongodbSyncAdapter;

struct MongodbFactory;
impl DatabaseDriverFactory for MongodbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(MongodbDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "mongodb"
    }
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(Arc::new(MongodbResourceProvider::new(
            self.create(),
            self.driver_id(),
        )))
    }
    fn resource_capabilities(&self) -> CapabilitySet {
        resource_capabilities::mongodb_capability_set()
    }
}
datazen_driver_api::register_driver!(&MongodbFactory);
