//! DataZen path driver: rqlite

use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::*;

mod resource_provider;
mod rqlite;
pub use rqlite::*;

/// The one driver every caller shares, and the one provider bound to it.
///
/// Memoized for the same reason [`DatabaseDriverFactory::create`] must be: the host
/// calls `require_resource_provider` afresh on every lookup, and
/// [`LegacyResourceAdapter`] rejects any handle whose epoch is not the one its own
/// provider was built with. A provider rebuilt per call would invalidate every
/// handle immediately, which is the failure this shape exists to prevent.
static RQLITE_DRIVER: OnceLock<Arc<RqliteDriver>> = OnceLock::new();
/// The one provider, bound to [`RQLITE_DRIVER`] rather than to a throwaway driver
/// the host never holds.
static RQLITE_PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

struct RqliteFactory;

impl RqliteFactory {
    /// The driver the host and the provider both use. Shared, not rebuilt, so a
    /// handle the provider issues addresses the object the host is talking to.
    fn driver(&self) -> Arc<RqliteDriver> {
        RQLITE_DRIVER
            .get_or_init(|| Arc::new(RqliteDriver::new()))
            .clone()
    }

    /// The one provider, built over [`Self::driver`].
    fn provider(&self) -> Arc<dyn ResourceProvider> {
        RQLITE_PROVIDER
            .get_or_init(|| resource_provider::provider(self.driver()))
            .clone()
    }
}

impl DatabaseDriverFactory for RqliteFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "rqlite"
    }
    fn supports_explain(&self) -> bool {
        true
    }

    /// Track O: rqlite now reaches a real provider, so
    /// [`DatabaseDriverFactory::require_resource_provider`] returns a handle-
    /// issuing provider instead of `ResourceProviderMissing`.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }

    /// What this crate can honestly claim. It is byte-identical to the
    /// provider's own registry on purpose — see `resource_provider::capabilities`.
    fn resource_capabilities(&self) -> CapabilitySet {
        resource_provider::capabilities()
    }
}
datazen_driver_api::register_driver!(&RqliteFactory);
