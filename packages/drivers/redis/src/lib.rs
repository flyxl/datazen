//! DataZen path driver: redis

use std::sync::Arc;

use datazen_driver_api::*;

mod redis_driver;
pub use redis_driver::*;

struct RedisFactory;
impl DatabaseDriverFactory for RedisFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        Arc::new(RedisDriver::new()) as Arc<dyn DatabaseDriver>
    }
    fn create_kv(&self) -> Option<Arc<dyn KeyValueDriver>> {
        Some(Arc::new(RedisDriver::new()) as Arc<dyn KeyValueDriver>)
    }
    fn driver_id(&self) -> &'static str { "redis" }
}
datazen_driver_api::register_driver!(&RedisFactory);
