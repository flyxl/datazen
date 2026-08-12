//! DataZen path driver: mongodb

use std::sync::Arc;

use datazen_driver_api::*;

mod mongodb;
mod sync_adapter;
pub use mongodb::*;
pub use sync_adapter::MongodbSyncAdapter;

struct MongodbFactory;
impl DatabaseDriverFactory for MongodbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(MongodbDriver::new())
    }
    fn driver_id(&self) -> &'static str {
        "mongodb"
    }
}
datazen_driver_api::register_driver!(&MongodbFactory);
