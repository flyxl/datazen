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
/// Held as the concrete type, with the erasure deferred to
/// [`MongodbFactory::provider`], because erasing it here would discard it
/// permanently.
///
/// `ResourceProvider` is `Send + Sync` and nothing more — no `Any`, and no
/// `downcast` anywhere in `driver-api`. So an `Arc<dyn ResourceProvider>` is a
/// one-way door: once this static is written that way, nothing in the crate can
/// again reach the driver the provider is bound to, and "the provider drives the
/// same driver `create()` handed out" stops being a property that can be
/// checked and becomes something only the source text can be read for. Holding
/// the concrete type costs one coercion at the boundary — callers still receive
/// `Arc<dyn ResourceProvider>` — and keeps the binding inspectable, without
/// widening the trait surface to get it back later.
///
/// Do not "unify" this toward the majority `Arc<dyn ResourceProvider>`:
/// `packages/drivers/redis/src/lib.rs` holds its provider the same way, so the
/// concrete shape is the established one for a provider whose binding to a
/// particular driver has to remain checkable. The two shapes differ by what
/// they throw away, not by oversight.
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
