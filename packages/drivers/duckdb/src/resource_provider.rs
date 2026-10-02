//! Track O: DuckDB's real [`ResourceProvider`].
//!
//! DuckDB is reached through the contract's own migration bridge,
//! [`LegacyResourceAdapter`], rather than through a bespoke provider impl. The adapter
//! is what makes the handles real: it opens the connection itself, mints an opaque
//! [`ResourceHandle`] bound to that connection's own id, charges the caller's
//! [`BudgetPort`] *before* any wire work, and releases the charge exactly once on
//! close.
//!
//! Everything DuckDB cannot honestly perform is refused by name instead of answered
//! with an empty success:
//!
//! * `request_cancel` — the bundled DuckDB handle exposes no interrupt from this
//! driver's call sites, so the adapter answers `CancelDisposition::Unsupported` and
//! never calls the legacy `cancel_query`. That method used to answer `Ok(())`, i.e. a
//! reported cancellation that never happened; Track O turns it into an explicit
//! `DriverError::Unsupported`.
//! * `observe_session` — every field unknown, never back-filled with the acquisition
//! target.
//! * `change_context` — refused, because attaching another database is a fresh open,
//! not a context switch.
//! * `commit_transaction` / `rollback_transaction` — DuckDB resolves transactions
//! inside the driver's own commands.
//!
//! # Namespace
//!
//! `DatabaseDriver::has_schema_level` is the default `false` and
//! `DuckDbDriver::qualify_sql_target` calls `sql_target::qualify_sql` with the database
//! only. A blank path is normalised to `:memory:` before connecting, which is still a
//! database name and is therefore declared. The shape declares an **optional**
//! `Database` level and nothing else, so a `Schema` or `Catalog` is refused by name
//! instead of being dropped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use datazen_driver_api::capabilities::CapabilitySet;
use datazen_driver_api::namespace::{NamespaceLevel, NamespaceLevelKind, NamespaceShape};
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;

use crate::DuckDbDriver;

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

/// The namespace DuckDB actually addresses.
///
/// DuckDB addresses one namespace level, the database.
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

/// The real provider for `duckdb`.
pub(crate) fn resource_provider() -> Arc<dyn ResourceProvider> {
    Arc::new(LegacyResourceAdapter::new(
        Arc::new(DuckDbDriver::new()),
        "duckdb",
        env!("CARGO_PKG_VERSION"),
        PROVIDER_GENERATION.fetch_add(1, Ordering::Relaxed),
        namespace_shape(),
    ))
}

/// What this crate can honestly claim, capability by capability.
///
/// The value is `CapabilitySet::default()` — every field at the non-supporting answer —
/// and that is the truthful statement, not an omission:
///
/// * `precise_cancel` stays non-supporting because DuckDB has no execution-handle
/// cancel protocol. `LegacyResourceAdapter` derives this field from
/// `supports_query_execution_cancel()`, which for DuckDB is `false`; declaring anything
/// stronger here would make the factory disagree with the provider that actually owns
/// the registry.
/// * `stateful_session` stays unknown because a DuckDB database handle is not a fixed,
/// resumable session
/// * `observe_session`, `change_context`, `reset_resource`, commit and rollback stay
/// non-supporting for the reasons in the module docs; nothing here is migrated to a
/// session contract yet, so claiming otherwise would be a capability the port cannot
/// honour.
///
/// A caller that needs one of these gets an explicit
/// `ResourceError::CapabilityNotDeclared` instead of a silent empty success. Keep this
/// byte-identical to what `resource_provider()` reports: a factory that looks rosier
/// than its own provider is exactly the drift the contract exists to prevent.
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

    use crate::{ConnectionHandle, DatabaseDriver, DriverError, DuckDbDriver, DuckDbFactory};

    fn factory() -> DuckDbFactory {
        DuckDbFactory
    }

    fn config() -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "cfg-1",
            "name": "duckdb",
            "databaseType": "duckdb",
            "database": "memory",
        }))
        .expect("a minimal config deserializes")
    }

    #[test]
    fn the_factory_reaches_a_real_provider_instead_of_a_missing_one() {
        let provider = require_resource_provider(&factory())
            .expect("DuckDB is migrated, so the provider must be reachable");
        assert_eq!(provider.provider_id(), "duckdb");
        assert_eq!(
            provider.namespace_shape().levels.len(),
            1,
            "DuckDB addresses one namespace level, the database"
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
            "DuckDB has no migrated session contract, so it must claim nothing"
        );
        assert!(
            !factory()
                .resource_capabilities()
                .precise_cancel
                .accepts_precise_cancel(),
            "DuckDB has no execution-handle cancel protocol"
        );
        assert!(registry.require_precise_cancel().is_err());
        assert_eq!(
            factory().resource_capabilities().stateful_session,
            datazen_driver_api::capabilities::Availability::Unknown,
            "a DuckDB database handle is not a fixed, resumable session"
        );
    }

    #[test]
    fn the_capability_snapshot_reports_identity_without_inventing_confirmations() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();

        assert_eq!(registry.snapshot.driver_id, "duckdb");
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
            "nothing was actually confirmed for DuckDB, and an empty record says so"
        );
    }

    #[tokio::test]
    async fn a_described_resource_is_never_mistaken_for_a_fixed_reusable_session() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let descriptor = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty().with_database("memory"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect("describe_resource must succeed without touching the network");

        assert!(!descriptor.is_fixed_session());
        assert_eq!(
            descriptor.session_continuity,
            SessionContinuity::Unknown,
            "DuckDB cannot observe a session, so continuity stays unknown"
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
            "DuckDB cannot prove a resource is back at its baseline after use"
        );
    }

    #[tokio::test]
    async fn a_namespace_level_duckdb_does_not_have_is_refused_by_name() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let err = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty()
                    .with_database("memory")
                    .with_schema("main"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect_err("the namespace shape must reject a level duckdb cannot address");

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
        let driver = DuckDbDriver::new();
        let err = driver
            .cancel_query(&ConnectionHandle {
                id: "connection-1".into(),
                pool_id: "duckdb-pool".into(),
            })
            .await
            .expect_err("DuckDB cannot cancel; Ok(()) was the defect Track O removes");

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
            "DuckDB has no execution-handle protocol to advertise"
        );
        assert!(!DuckDbDriver::new().supports_query_execution_cancel());
    }
}
