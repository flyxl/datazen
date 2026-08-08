//! DataZen path driver: mongodb

use std::sync::Arc;

use datazen_driver_api::*;

mod mongodb;
pub use mongodb::*;

struct MongodbFactory;
impl DatabaseDriverFactory for MongodbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(MongodbDriver::new())
    }
    fn driver_id(&self) -> &'static str { "mongodb" }
}
datazen_driver_api::register_driver!(&MongodbFactory);
