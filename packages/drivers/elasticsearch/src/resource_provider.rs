//! Track O: Elasticsearch's real [`ResourceProvider`].
//!
//! Elasticsearch is reached through the contract's own migration bridge,
//! [`LegacyResourceAdapter`], rather than through a bespoke provider impl. The adapter
//! is what makes the handles real: it opens the connection itself, mints an opaque
//! [`ResourceHandle`] bound to that connection's own id, charges the caller's
//! [`BudgetPort`] *before* any wire work, and releases the charge exactly once on
//! close.
//!
//! Everything Elasticsearch cannot honestly perform is refused by name instead of
//! answered with an empty success:
//!
//! * `request_cancel` — the `_tasks` interrupt path is not wired into this driver, so
//! the adapter answers `CancelDisposition::Unsupported` and never calls the legacy
//! `cancel_query`. That method used to answer `Ok(())`, i.e. a reported cancellation
//! that never happened; Track O turns it into an explicit `DriverError::Unsupported`.
//! * `observe_session` — every field unknown, never back-filled with the acquisition
//! target.
//! * `change_context` — refused, because changing the authenticated principal is a
//! reconnect.
//! * `commit_transaction` / `rollback_transaction` — Elasticsearch has no cross-request
//! transactions, so the port refuses rather than inventing an outcome.
//!
//! # Namespace
//!
//! `ElasticsearchDriver` documents this directly at `elasticsearch.rs:13` — there is no
//! database/schema hierarchy, indices live in one cluster — and `has_schema_level` is
//! the default `false`. The shape therefore declares **no levels at all**, which is the
//! fail-closed reading: an index name travels in the resource `path`, not in the
//! namespace. A caller that supplies `Database`, `Schema` or `Catalog` is refused with
//! `ResourceError::NonexistentNamespaceLevel` rather than having the value silently
//! dropped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::namespace::{NamespaceLevel, NamespaceLevelKind, NamespaceShape};
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::DatabaseDriver;

/// Epoch counter handed to each provider this crate builds.
///
/// The host has no session generation to hand a driver yet
/// (`src-tauri/src/platform/adapter.rs` records `RuntimeEpoch` as 「❌ 无会话世代」), so
/// the provider mints its own instead of inventing a fixed constant.
/// `LegacyResourceAdapter` checks each handle against the epoch it was built with, so
/// a handle minted by one provider instance is rejected by any other.
///
/// The counter is only advanced inside [`PROVIDER`]'s initializer, so the one
/// provider that exists takes one epoch and keeps it: the host calls
/// `require_resource_provider` afresh on every lookup, and a provider rebuilt per
/// call would invalidate every handle the moment it was looked up again.
///
/// The counter restarts at 1 on each process start, so a handle that outlived a
/// restart is *not* caught here — that needs a host-owned epoch, which a driver crate
/// cannot supply.
static PROVIDER_GENERATION: AtomicU64 = AtomicU64::new(1);

/// The one provider this process has. Memoized, so its epoch never changes and the
/// handles it issues stay valid across however many times the host looks it up.
static PROVIDER: OnceLock<Arc<dyn ResourceProvider>> = OnceLock::new();

/// The epoch [`PROVIDER`] was actually built with.
///
/// The counter cannot answer that on its own — it advances for every provider built,
/// memoized or not — so the value the live provider took is recorded next to it.
static PROVIDER_EPOCH: OnceLock<u64> = OnceLock::new();

/// The namespace Elasticsearch actually addresses.
///
/// Elasticsearch declares no namespace levels at all.
pub(crate) fn namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![],
        ..NamespaceShape::default()
    }
}

/// The real provider for `elasticsearch`, built once and shared.
///
/// `driver` is the instance the host also gets from `DatabaseDriverFactory::create`,
/// not a throwaway: the provider and the host must be looking at the same driver, or
/// a handle it issues would address a different one.
pub(crate) fn provider(driver: Arc<dyn DatabaseDriver>) -> Arc<dyn ResourceProvider> {
    PROVIDER
        .get_or_init(|| {
            let runtime_epoch = PROVIDER_GENERATION.fetch_add(1, Ordering::Relaxed);
            // Recorded inside the initializer, so it always describes the provider
            // that won the race and never a discarded attempt.
            let _ = PROVIDER_EPOCH.set(runtime_epoch);
            Arc::new(LegacyResourceAdapter::new(
                driver,
                "elasticsearch",
                env!("CARGO_PKG_VERSION"),
                runtime_epoch,
                namespace_shape(),
                capabilities(),
            ))
        })
        .clone()
}

/// The epoch the live provider stamps into every handle it mints.
///
/// Test-only: production code hands a handle back to the provider that issued it and
/// never has to compare epochs itself.
#[cfg(test)]
pub(crate) fn runtime_epoch() -> u64 {
    *PROVIDER_EPOCH
        .get()
        .expect("the memoized provider is built before any handle is minted")
}

/// Build a provider that is *not* [`PROVIDER`], and the epoch it took.
///
/// Test-only. It reproduces the failure per-call construction caused, so the
/// memoization test can show the epoch check really does refuse a foreign handle.
#[cfg(test)]
pub(crate) fn stray_provider(driver: Arc<dyn DatabaseDriver>) -> Arc<dyn ResourceProvider> {
    let runtime_epoch = PROVIDER_GENERATION.fetch_add(1, Ordering::Relaxed);
    Arc::new(LegacyResourceAdapter::new(
        driver,
        "elasticsearch",
        env!("CARGO_PKG_VERSION"),
        runtime_epoch,
        namespace_shape(),
        capabilities(),
    ))
}

/// What this crate can honestly claim, capability by capability.
///
/// The value is `CapabilitySet::default()` — every field at the non-supporting answer —
/// and that is the truthful statement, not an omission:
///
/// * `precise_cancel` stays non-supporting because Elasticsearch has no
/// execution-handle cancel protocol. `LegacyResourceAdapter` derives this field from
/// `supports_query_execution_cancel()`, which for Elasticsearch is `false`; declaring
/// anything stronger here would make the factory disagree with the provider that
/// actually owns the registry.
/// * `stateful_session` stays unknown because an Elasticsearch cluster exposes no fixed
/// session
/// * `observe_session`, `change_context`, `reset_resource`, commit and rollback stay
/// non-supporting for the reasons in the module docs; nothing here is migrated to a
/// session contract yet, so claiming otherwise would be a capability the port cannot
/// honour.
///
/// A caller that needs one of these gets an explicit
/// `ResourceError::CapabilityNotDeclared` instead of a silent empty success. Keep this
/// byte-identical to what `provider()` reports: a factory that looks rosier
/// than its own provider is exactly the drift the contract exists to prevent.
pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet::default()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use datazen_driver_api::capabilities::SessionContinuity;
    use datazen_driver_api::namespace::NamespaceTarget;
    use datazen_driver_api::require_resource_provider;
    use datazen_driver_api::resource::{
        DescribeResourceRequest, IdentityScope, ResourceError, ResourceHandle, ResourcePurpose,
    };
    use datazen_driver_api::{ConnectionConfig, DatabaseDriverFactory};

    use crate::{
        ConnectionHandle, DatabaseDriver, DriverError, ElasticsearchDriver, ElasticsearchFactory,
    };

    fn factory() -> ElasticsearchFactory {
        ElasticsearchFactory
    }

    fn config() -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "cfg-1",
            "name": "elasticsearch",
            "databaseType": "elasticsearch",
            "database": "",
        }))
        .expect("a minimal config deserializes")
    }

    #[test]
    fn the_factory_reaches_a_real_provider_instead_of_a_missing_one() {
        let provider = require_resource_provider(&factory())
            .expect("Elasticsearch is migrated, so the provider must be reachable");
        assert_eq!(provider.provider_id(), "elasticsearch");
        assert_eq!(
            provider.namespace_shape().levels.len(),
            0,
            "Elasticsearch declares no namespace levels at all"
        );
    }

    #[test]
    fn factory_capabilities_match_the_provider() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();

        assert_eq!(
            factory().resource_capabilities(),
            registry.capabilities,
            "the factory's capability view must not be rosier than the provider's own registry"
        );
        assert!(
            !factory().resource_capabilities().declares_anything(),
            "Elasticsearch has no migrated session contract, so it must claim nothing"
        );
        assert!(
            !factory()
                .resource_capabilities()
                .precise_cancel
                .accepts_precise_cancel(),
            "Elasticsearch has no execution-handle cancel protocol"
        );
        assert!(registry.require_precise_cancel().is_err());
        assert_eq!(
            factory().resource_capabilities().stateful_session,
            datazen_driver_api::capabilities::Availability::Unknown,
            "an Elasticsearch cluster exposes no fixed session"
        );
    }

    #[test]
    fn the_capability_snapshot_reports_identity_without_inventing_confirmations() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();

        assert_eq!(registry.snapshot.driver_id, "elasticsearch");
        assert_eq!(
            registry.snapshot.driver_version,
            env!("CARGO_PKG_VERSION"),
            "the driver version must be the real crate version, not an invented string"
        );
        assert_eq!(
            registry.snapshot.protocol_version,
            datazen_driver_api::PROTOCOL_VERSION
        );
        assert!(
            registry.snapshot.confirmed.is_empty(),
            "nothing was actually confirmed for Elasticsearch, and an empty record says so"
        );
    }

    #[tokio::test]
    async fn a_described_resource_is_never_mistaken_for_a_fixed_reusable_session() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let descriptor = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty(),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect("describe_resource must succeed without touching the network");

        assert!(!descriptor.is_fixed_session());
        assert_eq!(
            descriptor.session_continuity,
            SessionContinuity::Unknown,
            "Elasticsearch cannot observe a session, so continuity stays unknown"
        );
        assert!(
            matches!(
                descriptor.connection_cost_policy,
                datazen_driver_api::resource::ConnectionCostPolicy::DeclaredConservative { .. }
            ),
            "a driver that never measured its cost must declare a conservative one, not a free one"
        );
        assert_eq!(
            descriptor.reuse_policy,
            datazen_driver_api::resource::ReusePolicy::Unknown,
            "Elasticsearch cannot prove a resource is back at its baseline after use"
        );
    }

    #[tokio::test]
    async fn a_namespace_level_elasticsearch_does_not_have_is_refused_by_name() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let err = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty().with_database("logs-2024"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect_err("the namespace shape must reject a level elasticsearch cannot address");

        assert!(
            matches!(
                err,
                datazen_driver_api::resource::ResourceError::NonexistentNamespaceLevel { .. }
            ),
            "a caller must be told the level is not part of the shape, not have it dropped: {err}"
        );
    }

    #[tokio::test]
    async fn the_legacy_cancel_refuses_instead_of_reporting_a_cancellation_that_never_happened() {
        let driver = ElasticsearchDriver::new();
        let err = driver
            .cancel_query(&ConnectionHandle {
                id: "connection-1".into(),
                pool_id: "elasticsearch-pool".into(),
            })
            .await
            .expect_err("Elasticsearch cannot cancel; Ok(()) was the defect Track O removes");

        assert!(matches!(err, DriverError::Unsupported(_)), "got {err:?}");
    }

    #[test]
    fn no_cancel_predicate_is_claimed_on_either_dialect_of_the_contract() {
        let factory = factory();
        assert!(
            !factory.supports_cancel_query(),
            "the legacy session-wide cancel is refused, so nothing may advertise it"
        );
        assert!(
            !factory.supports_query_execution_cancel(),
            "Elasticsearch has no execution-handle protocol to advertise"
        );
        assert!(!ElasticsearchDriver::new().supports_query_execution_cancel());
    }

    /// The host must be able to find the same provider twice.
    ///
    /// `require_resource_provider` calls `DatabaseDriverFactory::resource_provider`
    /// afresh on every lookup, and `LegacyResourceAdapter` refuses a handle whose
    /// epoch is not the one it was built with. A provider rebuilt per call therefore
    /// invalidated every handle the instant the host looked it up again: the resource
    /// layer existed, and nothing could use it.
    #[tokio::test]
    async fn the_provider_is_memoized_so_its_own_handles_survive_the_next_lookup() {
        let first = factory()
            .resource_provider()
            .expect("elasticsearch declares a real provider");
        let second = factory()
            .resource_provider()
            .expect("elasticsearch declares a real provider");
        assert!(
            Arc::ptr_eq(&first, &second),
            "two lookups handed back different provider instances, so a handle minted \
             by the first is rejected by the second"
        );

        let handle =
            ResourceHandle::issue(first.provider_id(), "connection-1", super::runtime_epoch());
        // The epoch being checked is the one *this* provider was built with, not one
        // the test hands over: `close_resource` is the cheapest provider call that
        // runs the ownership + epoch check, and it answers `Ok` for a resource it
        // simply is not holding, so a pass is attributable to the check alone.
        second.close_resource(&handle).await.expect(
            "a handle the provider minted must still validate against the provider \
                 the host looks up next time",
        );

        // And the check is not vacuous: a provider built outside the memo took
        // another epoch and still refuses the handle. That is precisely the failure
        // per-call construction produced.
        let stray = super::stray_provider(factory().create());
        assert!(
            !Arc::ptr_eq(&first, &stray),
            "the stray provider must not be the memoized one"
        );
        let rejection = stray
            .close_resource(&handle)
            .await
            .expect_err("a provider built outside the memo must not accept the handle");
        assert!(
            matches!(rejection, ResourceError::StaleRuntimeEpoch { .. }),
            "got {rejection:?}"
        );
    }
}
