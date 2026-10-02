//! DataZen path driver: redis

use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod commands;
mod connect;
mod decode;
mod driver;
mod ops;
mod resource;
mod types;
mod value;
pub use connect::{build_connection_plan, ConnectionPlan, RedisLiveConn, TlsPlan, Topology};
pub use ops::{set_settings_allow_flush, settings_allow_flush};
pub use resource::RedisResourceProvider;

/// Plugin settings key used by the host to locate this driver's settings block.
/// Avoids the host hard-coding `"redis"` — the driver owns its key identity.
pub const SETTINGS_KEY: &str = "redis";
pub use driver::*;

#[cfg(feature = "tauri-plugin")]
mod plugin;

#[cfg(feature = "tauri-plugin")]
pub use plugin::init;

static SHARED: OnceLock<Arc<RedisDriver>> = OnceLock::new();

/// Process-wide Redis driver instance (shared by host registry and plugin commands).
pub(crate) fn shared_driver() -> Arc<RedisDriver> {
    SHARED.get_or_init(|| Arc::new(RedisDriver::new())).clone()
}

static RESOURCE_PROVIDER: OnceLock<Arc<RedisResourceProvider>> = OnceLock::new();

/// The one [`ResourceProvider`] this process will ever hand out.
///
/// Memoized for two reasons, and the second is the load-bearing one:
///
/// 1. Every provider instance takes the next runtime epoch
///    (`resource::provider`), so a handle issued by an older instance would
///    fail [`ResourceHandle::check`] on the next call. Re-creating the provider
///    per lookup would invalidate every live Redis session.
/// 2. The provider holds the *same* `Arc<RedisDriver>` the factory hands out,
///    because `RedisDriver::connect` writes into `RedisDriver::connections` and
///    every later command resolves it by `handle.pool_id`. A second
///    `RedisDriver` would be a driver with no connections at all.
fn resource_provider() -> Arc<RedisResourceProvider> {
    RESOURCE_PROVIDER
        .get_or_init(|| Arc::new(RedisResourceProvider::new(shared_driver())))
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
    fn driver_id(&self) -> &'static str {
        "redis"
    }

    /// Redis reaches a real provider, so
    /// [`DatabaseDriverFactory::require_resource_provider`] returns a
    /// handle-issuing provider instead of `ResourceProviderMissing`.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(resource_provider())
    }

    /// What this crate can honestly claim. It comes from the provider's own
    /// registry rather than being restated, so the factory's view and the
    /// provider's view cannot drift apart.
    fn resource_capabilities(&self) -> CapabilitySet {
        resource_provider().capabilities().capabilities.clone()
    }
}
datazen_driver_api::register_driver!(&RedisFactory);
