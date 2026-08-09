//! DataZen path driver: redis

use std::sync::{Arc, OnceLock};

use datazen_driver_api::*;

mod ops;
mod redis_driver;
pub use redis_driver::*;

#[cfg(feature = "tauri-plugin")]
mod plugin;

#[cfg(feature = "tauri-plugin")]
pub use plugin::init;

static SHARED: OnceLock<Arc<RedisDriver>> = OnceLock::new();

/// Process-wide Redis driver instance (shared by host registry and plugin commands).
pub(crate) fn shared_driver() -> Arc<RedisDriver> {
    SHARED
        .get_or_init(|| Arc::new(RedisDriver::new()))
        .clone()
}

struct RedisFactory;
impl DatabaseDriverFactory for RedisFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        shared_driver()
    }
    fn create_kv(&self) -> Option<Arc<dyn KeyValueDriver>> {
        Some(shared_driver())
    }
    fn driver_id(&self) -> &'static str { "redis" }
}
datazen_driver_api::register_driver!(&RedisFactory);
