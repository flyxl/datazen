//! DataZen path driver: hbase

use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod hbase;
mod resource;
mod sync_adapter;
pub use hbase::*;
pub use resource::HBaseResourceProvider;
pub use sync_adapter::HBaseSyncAdapter;

/// The Stargate factory.
///
/// The driver and its resource provider are memoized together on purpose: the
/// provider must be bound to *the same* [`HBaseDriver`] the host gets from
/// [`DatabaseDriverFactory::create`]. A provider over a second driver instance
/// would hold resources the host cannot reach, and a handle minted by one would
/// be rejected by the other.
struct HBaseFactory;

/// The one driver every caller shares.
static HBASE_DRIVER: OnceLock<Arc<HBaseDriver>> = OnceLock::new();
/// The one provider, bound to [`HBASE_DRIVER`].
static HBASE_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

impl HBaseFactory {
    fn driver(&self) -> Arc<HBaseDriver> {
        HBASE_DRIVER
            .get_or_init(|| Arc::new(HBaseDriver::new()))
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        HBASE_PROVIDER
            .get_or_init(|| {
                Arc::new(HBaseResourceProvider::new(self.driver())) as Arc<dyn ResourceProvider>
            })
            .clone()
    }
}

impl DatabaseDriverFactory for HBaseFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "hbase"
    }
    /// The hbase driver really implements the resource contract: the returned
    /// provider runs this crate's own `reqwest` client pool. It is never `None`,
    /// so a caller that wants the resource contract always gets a real one — and
    /// the operations Stargate cannot honour come back as an explicit
    /// `Unsupported`, not as a missing provider.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }
    /// Declared by [`resource::capabilities`], whose table maps every cell to the
    /// code that backs it. The cells that cannot be proven are left `Unknown`
    /// rather than guessed.
    fn resource_capabilities(&self) -> CapabilitySet {
        self.provider().capabilities().capabilities.clone()
    }
}
datazen_driver_api::register_driver!(&HBaseFactory);
