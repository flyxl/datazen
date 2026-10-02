//! DataZen path driver: mongodb
//!
//! The factory memoizes its driver and its resource provider together. The
//! reason is identity, not performance: a `ResourceHandle` is only meaningful
//! against the provider that issued it, so the provider must stay the same
//! instance for the life of the process, and it must be bound to the very
//! driver [`DatabaseDriverFactory::create`] hands out. Building a fresh pair
//! per `resource_provider()` call satisfies neither — the host holds one driver
//! while the provider drives another, and a handle saved across two lookups is
//! rejected by the second.
//!
//! For mongodb that split is concrete rather than theoretical: the driver holds
//! the `Client` map in [`MongodbDriver`], so the instance the host holds and
//! the instance the provider connects through would otherwise be two separate
//! client caches.
//!
//! One `create()`, once, is therefore part of the contract. Calling `create()`
//! again — or letting the provider call it — is not.

use std::sync::{Arc, OnceLock};

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

/// The one driver every caller shares, and the one [`MONGODB_PROVIDER`] is bound to.
static MONGODB_DRIVER: OnceLock<Arc<dyn DatabaseDriver>> = OnceLock::new();
/// The one provider, bound to [`MONGODB_DRIVER`].
///
/// Held as the concrete type rather than the `Arc<dyn ResourceProvider>` the
/// other memoized factories use, for exactly one reason: it is what lets
/// `create_and_resource_provider_share_one_driver` reach the driver's `Arc` and
/// prove the binding instead of reading it out of the source. The coercion
/// happens at the boundary, so callers still receive the same
/// `Arc<dyn ResourceProvider>` as before.
static MONGODB_PROVIDER: OnceLock<Arc<MongodbResourceProvider>> = OnceLock::new();

impl MongodbFactory {
    fn driver(&self) -> Arc<dyn DatabaseDriver> {
        MONGODB_DRIVER
            .get_or_init(|| Arc::new(MongodbDriver::new()))
            .clone()
    }

    /// The memoized provider, still concrete — see [`MONGODB_PROVIDER`].
    fn provider_bound(&self) -> Arc<MongodbResourceProvider> {
        MONGODB_PROVIDER
            .get_or_init(|| {
                Arc::new(MongodbResourceProvider::new(
                    self.driver(),
                    self.driver_id(),
                ))
            })
            .clone()
    }

    fn provider(&self) -> Arc<dyn ResourceProvider> {
        self.provider_bound() as Arc<dyn ResourceProvider>
    }
}

impl DatabaseDriverFactory for MongodbFactory {
    fn create(&self) -> Arc<dyn DatabaseDriver> {
        self.driver()
    }
    fn driver_id(&self) -> &'static str {
        "mongodb"
    }
    /// Memoized, and bound to [`MongodbFactory::driver`] — the same instance
    /// `create()` returns. Rebuilding it per call would leave the host holding
    /// one driver while the provider connects through another, and would reject
    /// every handle saved across two lookups.
    fn resource_provider(&self) -> Option<Arc<dyn ResourceProvider>> {
        Some(self.provider())
    }
    fn resource_capabilities(&self) -> CapabilitySet {
        resource_capabilities::mongodb_capability_set()
    }
}
datazen_driver_api::register_driver!(&MongodbFactory);

#[cfg(test)]
mod tests {
    use super::*;

    /// Criterion one, taken literally: the driver `create()` hands out and the
    /// driver the provider holds must be the same allocation. Asserting only
    /// that two `create()` calls agree would pass even if the provider bound
    /// itself to a private second instance, which is the defect being fixed.
    #[test]
    fn create_and_resource_provider_share_one_driver() {
        let factory = MongodbFactory;

        let from_create = factory.create();
        let provider = factory.provider_bound();

        assert!(
            Arc::ptr_eq(&from_create, provider.driver()),
            "the provider must be bound to the very driver create() hands out; \
             two instances means the host's client cache and the provider's are separate"
        );
        assert_eq!(provider.provider_id(), factory.driver_id());
    }

    /// Criterion two: one provider per factory for the life of the process.
    #[test]
    fn repeated_resource_provider_calls_return_the_same_instance() {
        let factory = MongodbFactory;

        let first = factory
            .resource_provider()
            .expect("mongodb implements the resource provider contract");
        let second = factory
            .resource_provider()
            .expect("mongodb implements the resource provider contract");

        assert!(
            Arc::ptr_eq(&first, &second),
            "resource_provider() must be memoized: a rebuilt provider rejects every \
             handle issued by the previous one"
        );
        assert_eq!(first.provider_id(), factory.driver_id());
    }

    /// The two criteria joined the way the host actually uses them: take a
    /// driver, look the provider up twice, and expect the second lookup to
    /// still be the provider the first one handed out.
    #[test]
    fn a_lookup_after_create_reuses_both() {
        let factory = MongodbFactory;

        let driver = factory.create();
        let first = factory
            .resource_provider()
            .expect("mongodb implements the resource provider contract");
        let second = factory
            .resource_provider()
            .expect("mongodb implements the resource provider contract");

        assert!(
            Arc::ptr_eq(&first, &second),
            "provider lookup must be stable"
        );
        assert!(
            Arc::ptr_eq(&driver, factory.provider_bound().driver()),
            "create() must keep returning the driver the memoized provider is bound to"
        );
    }
}
