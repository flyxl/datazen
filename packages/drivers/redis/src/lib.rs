//! DataZen path driver: redis

use std::sync::{Arc, OnceLock};

use datazen_driver_api::*;

mod commands;
mod connect;
mod ops;
mod ops_cluster;
mod ops_exec;
mod ops_io;
mod ops_json;
mod ops_observe;
mod ops_pubsub;
mod ops_stream;
mod redis_driver;
pub use connect::{build_connection_plan, ConnectionPlan, RedisLiveConn, Topology, TlsPlan};
pub use ops::{set_settings_allow_flush, settings_allow_flush};
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
