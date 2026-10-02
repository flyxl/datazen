//! DataZen path driver: vector

use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod resource;
mod sync_adapter;
mod vector;
pub use resource::VectorResourceProvider;
pub use sync_adapter::VectorSyncAdapter;
pub use vector::*;

/// The Qdrant factory.
///
/// The driver and its resource provider are memoized together on purpose: the
/// provider must be bound to *the same* [`VectorDriver`] the host gets from
/// [`DatabaseDriverFactory::create`]. A provider over a second driver instance
/// would hold resources the host cannot reach, and a handle minted by one would
/// be rejected by the other.
struct VectorFactory;

/// The one driver every caller shares.
static VECTOR_DRIVER: OnceLock<Arc<VectorDriver>> = OnceLock::new();
/// The one provider, bound to [`VECTOR_DRIVER`].
static VECTOR_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

impl VectorFactory {
    fn driver(&self) -> Arc<VectorDriver> {
        VECTOR_DRIVER
            .get_or_init(|| Arc::new(VectorDriver::new()))
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        VECTOR_PROVIDER
            .get_or_init(|| {
                Arc::new(VectorResourceProvider::new(self.driver())) as Arc<dyn ResourceProvider>
            })
            .clone()
    }
}

impl DatabaseDriverFactory for VectorFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "vector"
    }
    /// The vector driver really implements the resource contract: the returned
    /// provider runs this crate's own `reqwest` client pool. It is never `None`,
    /// so a caller that wants the resource contract always gets a real one — and
    /// the operations Qdrant cannot honour come back as an explicit
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

datazen_driver_api::register_driver!(&VectorFactory);
