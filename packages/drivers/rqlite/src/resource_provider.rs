//! Track O: rqlite's real [`ResourceProvider`].
//!
//! rqlite is reached through the contract's own migration bridge,
//! [`LegacyResourceAdapter`], rather than through a bespoke provider impl. The adapter
//! is what makes the handles real: it opens the connection itself, mints an opaque
//! [`ResourceHandle`] bound to that connection's own id, charges the caller's
//! [`BudgetPort`] *before* any wire work, and releases the charge exactly once on
//! close.
//!
//! Everything rqlite cannot honestly perform is refused by name instead of answered
//! with an empty success:
//!
//! * `request_cancel` — rqlite speaks the SQLite wire protocol and has no per-execution
//! cancel, so the adapter answers `CancelDisposition::Unsupported` and never calls the
//! legacy `cancel_query`. That method used to answer `Ok(())`, i.e. a reported
//! cancellation that never happened; Track O turns it into an explicit
//! `DriverError::Unsupported`.
//! * `observe_session` — every field unknown, never back-filled with the acquisition
//! target.
//! * `change_context` — refused, because rqlite's "switch" is a reconnect.
//! * `commit_transaction` / `rollback_transaction` — rqlite resolves transactions
//! inside its own commands, so the resource port refuses rather than guessing an
//! outcome.
//!
//! # Namespace
//!
//! rqlite has no schema level: `DatabaseDriver::has_schema_level` is the default
//! `false` and `get_databases` reports only `main`. The attached-database name rqlite
//! routes by arrives as the `database` argument and is quoted as a SQLite schema
//! qualifier (`RqliteDriver::effective_database`, blank falling back to `main`), so the
//! shape declares an **optional** `Database` level and nothing else. A caller that
//! supplies a `Schema` or a `Catalog` is refused with
//! `ResourceError::NonexistentNamespaceLevel` rather than having the value silently
//! dropped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::namespace::{NamespaceLevel, NamespaceLevelKind, NamespaceShape};
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;

use crate::RqliteDriver;

/// Generation stamped onto every handle this crate mints.
///
/// The host has no session generation to hand a driver yet
/// (`src-tauri/src/platform/adapter.rs` records `RuntimeEpoch` as 「❌ 无会话世代」), so the
/// provider mints its own instead of inventing a fixed constant.
/// `LegacyResourceAdapter` checks each handle against the epoch it was built with, so a
/// handle minted by one provider instance is rejected by any other. The counter
/// restarts at 1 on each process start, so a handle that outlived a restart is *not*
/// caught here — that needs a host-owned epoch, which a driver crate cannot supply.
static PROVIDER_GENERATION: AtomicU64 = AtomicU64::new(1);

/// The namespace rqlite actually addresses: an optional attached database.
pub(crate) fn namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![NamespaceLevel {
            kind: NamespaceLevelKind::Database,
            exists: true,
            required: false,
        }],
        ..NamespaceShape::default()
    }
}

/// The real provider for `rqlite`.
pub(crate) fn resource_provider() -> Arc<dyn ResourceProvider> {
    Arc::new(LegacyResourceAdapter::new(
        Arc::new(RqliteDriver::new()),
        "rqlite",
        env!("CARGO_PKG_VERSION"),
        PROVIDER_GENERATION.fetch_add(1, Ordering::Relaxed),
        namespace_shape(),
    ))
}

/// What this crate can honestly claim about its own resources.
///
/// `CapabilitySet::default()` is every field at its non-supporting value, and for
/// rqlite that is the *true* answer, not a shrug:
///
/// * no fixed session — the driver keeps a `RwLock<HashMap<..>>` of HTTP clients, so a
/// resource is a pooled lease (`SessionContinuity::Unknown`);
/// * no context read-back, so `ContextObservation::Unsupported`;
/// * no transaction observation, no verified reset-to-baseline, no savepoints and no
/// per-operation DDL atomicity evidence — every one of those stays unknown;
/// * **no precise cancel** — rqlite has no execution-handle protocol, so
/// `PreciseCancelSupport` never leaves its default.
///
/// The value is also required to be byte-identical to what the adapter's own
/// `CapabilityRegistry` reports, because `DatabaseDriverFactory::resource_capabilities`
/// and `ResourceProvider::capabilities` are two views of one claim; a driver that made
/// the first view rosier than the second would be the 「假的全支持」 this contract exists to
/// kill. `tests::factory_capabilities_match_the_provider` is the gate that keeps the
/// two from drifting.
pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet::default()
}

#[cfg(test)]
mod tests {
    use datazen_driver_api::capabilities::SessionContinuity;
    use datazen_driver_api::namespace::NamespaceTarget;
    use datazen_driver_api::require_resource_provider;
    use datazen_driver_api::resource::{DescribeResourceRequest, IdentityScope, ResourcePurpose};
    use datazen_driver_api::{ConnectionConfig, DatabaseDriverFactory};

    use crate::{ConnectionHandle, DatabaseDriver, DriverError, RqliteDriver, RqliteFactory};

    fn factory() -> RqliteFactory {
        RqliteFactory
    }

    fn config() -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "cfg-1",
            "name": "rqlite",
            "databaseType": "rqlite",
            "database": "main",
        }))
        .expect("a minimal config deserializes")
    }

    #[test]
    fn the_factory_reaches_a_real_provider_instead_of_a_missing_one() {
        let provider = require_resource_provider(&factory())
            .expect("rqlite is migrated, so the provider must be reachable");
        assert_eq!(provider.provider_id(), "rqlite");
        assert_eq!(
            provider.namespace_shape().levels.len(),
            1,
            "rqlite addresses one namespace level, the attached database"
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
            "rqlite has no migrated session contract, so it must claim nothing"
        );
        assert!(
            !factory()
                .resource_capabilities()
                .precise_cancel
                .accepts_precise_cancel(),
            "rqlite has no execution-handle cancel protocol"
        );
        assert!(registry.require_precise_cancel().is_err());
        assert_eq!(
            factory().resource_capabilities().stateful_session,
            datazen_driver_api::capabilities::Availability::Unknown,
            "a pool of HTTP clients is not a fixed session"
        );
    }

    #[test]
    fn the_capability_snapshot_reports_identity_without_inventing_confirmations() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();

        assert_eq!(registry.snapshot.driver_id, "rqlite");
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
            "nothing was actually confirmed for rqlite, and an empty record says so"
        );
    }

    #[tokio::test]
    async fn a_described_resource_is_never_mistaken_for_a_fixed_reusable_session() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let descriptor = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::default(),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect("describe succeeds");

        assert!(!descriptor.is_fixed_session());
        assert_eq!(descriptor.session_continuity, SessionContinuity::Unknown);
        assert_eq!(
            descriptor.reuse_policy,
            datazen_driver_api::resource::ReusePolicy::Unknown,
            "rqlite cannot prove a resource is back at its baseline after use"
        );
        assert!(
            matches!(
                descriptor.connection_cost_policy,
                datazen_driver_api::resource::ConnectionCostPolicy::DeclaredConservative { .. }
            ),
            "a driver that never measured its cost must declare a conservative one, not a free one"
        );
    }

    #[tokio::test]
    async fn a_namespace_level_rqlite_does_not_have_is_refused_by_name() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let err = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::default().with_schema("main"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect_err("rqlite has no schema level, so a schema must be refused");

        assert!(
            matches!(
                err,
                datazen_driver_api::resource::ResourceError::NonexistentNamespaceLevel { .. }
            ),
            "got {err}"
        );
    }

    #[tokio::test]
    async fn the_legacy_cancel_refuses_instead_of_reporting_a_cancellation_that_never_happened() {
        let driver = RqliteDriver::new();
        let err = driver
            .cancel_query(&ConnectionHandle {
                id: "connection-1".into(),
                pool_id: "rqlite-pool".into(),
            })
            .await
            .expect_err("rqlite cannot cancel; Ok(()) was the defect Track O removes");

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
            "rqlite has no execution-handle protocol to advertise"
        );
        assert!(!RqliteDriver::new().supports_query_execution_cancel());
    }
}
