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

use datazen_driver_api::capabilities::{
    Availability, CapabilitySet, ContextObservation, DdlAtomicitySupport, NamespaceSwitch,
    PreciseCancelSupport, ResetForReuse, SessionScopedHandleSupport, SnapshotSupport,
    TransactionObservation, TransactionSupport,
};
use datazen_driver_api::namespace::{NamespaceLevel, NamespaceLevelKind, NamespaceShape};
use datazen_driver_api::resource::ResourceProvider;
use datazen_driver_api::resource_adapter::LegacyResourceAdapter;
use datazen_driver_api::{BackupSupport, DataSupport, DatabaseDriver};

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
/// Every row below is a claim about code that already exists in this crate, with the
/// `file:line` that backs it. The blank-looking cells are the interesting ones, so
/// each one says *why it cannot be filled* rather than leaving it to inference. A
/// caller that needs one of them gets `ResourceError::CapabilityNotDeclared` from the
/// registry, never a silent empty success.
///
/// | capability | declared | evidence |
/// | --- | --- | --- |
/// | `stateful_session` | `Unsupported` | `connect` stores `(reqwest::Client, String)` and nothing else (`elasticsearch.rs:19`, inserted at `:174`); every read/write is a fresh `client.get/post(..).send()` on that tuple (`:52-89`, `:464-469`) and `disconnect` only removes the map entry (`:192`). There is no server-side object to bind to. Migration doc §2.2 counts elasticsearch among the drivers that store a `(client, base)` tuple and have 「完全无会话」. |
/// | `namespace_switch` | `Unsupported` | Nothing to switch *to*: `get_databases` answers `vec!["default"]` (`elasticsearch.rs:197-199`) because a cluster has one namespace, and `namespace_shape()` declares zero levels. `change_context` is refused by the adapter. Migration doc §2.1 records `unsupported`. |
/// | `context_observation` | `Unsupported` | There is no read-back path to observe. `resolve_database` is a pure function over its argument (`elasticsearch.rs:43-50`) — the module comments at `:40` record that `use_database` is gone, so no mutable current-context field exists — and `observe_session` returns `SessionObservation::unobservable()` for every field. |
/// | `transaction_observation` | `Unsupported` | The driver overrides none of `begin_transaction`/`commit_transaction`/`rollback_transaction`, so the trait defaults answer the refusal, and the adapter's commit/rollback refuse with `ResourceError::OperationNotSupported`. No transaction state is ever observable. |
/// | `session_scoped_handles` | `Unsupported` | Elasticsearch *does* hand out a server-side object — the `/_sql` cursor, closed by `close_cursor` (`elasticsearch.rs:83`) — but `query_stream` closes it before it returns (`elasticsearch.rs:364-453`, the cursor is closed on stop, on error and at end of stream). It is an implementation detail scoped to one call, never a handle handed to the caller, which is exactly §6.5's 「不表示句柄失效」 case. |
/// | `reset_for_reuse` | `Unsupported` | Only two variants exist and no baseline-restore exists to name. The adapter answers `ResetDisposition::Discard` for `reset_resource`, and the driver has no `discard_connection` override. |
/// | `precise_cancel` | `Unknown` | **Not mine to declare.** `LegacyResourceAdapter` overwrites this field from `supports_query_execution_cancel()` (`resource_adapter.rs:139-143`), which is `false` here, so the provider's registry holds `Unknown` no matter what this function says. Declaring `Unsupported` would make the factory disagree with the provider that owns the registry — the exact drift `factory_capabilities_match_the_provider` exists to catch. The underlying fact is a measured refusal (`cancel_query` returns `DriverError::Unsupported`, `elasticsearch.rs:514`) and is asserted by `the_legacy_cancel_refuses_instead_of_reporting_a_cancellation_that_never_happened`. |
/// | `snapshots` | `Unsupported` | No snapshot endpoint exists anywhere in the crate and `begin_read_snapshot` is not overridden, so the trait default refusal is the driver's own answer rather than a guess. |
/// | `transactions.isolation_levels` | empty | No transaction can be opened at all, so there is no level to honour. Empty is read as "cannot confirm any", which is stronger than listing one. |
/// | `transactions.savepoints` | `Unsupported` | Savepoints require an open transaction; the driver cannot open one (row above). |
/// | `transactions.max_open_transactions` | `None` | Consequence of the same fact: no transaction is openable, so there is no limit to state. |
/// | `ddl_atomicity.by_operation` | empty | `DatabaseDriver::ddl_atomicity` is not overridden, so the driver answers `DdlAtomicity::Unknown` for *any* operation. `atomicity_for()` returns `Unknown` for an unlisted operation, so an empty map reproduces the driver's own answer exactly. It is a measured "I do not know", not a hole. |
/// | `data` | `StreamingReadWrite` | The only affirmative claim, and it is load-bearing. Reads stream incrementally through the real `/_sql` cursor: `query_stream` posts `{"cursor": c}`, pushes each decoded batch through `QueryRowBatcher::new` (`elasticsearch.rs:412`) *before* the next request, and only then asks for more (`elasticsearch.rs:364-453`). Writes are real: `execute` posts the statement and returns the server's own row count (`elasticsearch.rs:464-469`, `:468`). |
/// | `backup` | `Unknown` | Deliberately left unmeasured. Nothing in this crate produces or consumes a backup artifact, but "this driver has no backup code" is not the same as having measured the backup workflow, so this stays `Unknown` rather than claiming a refusal nobody checked. |
///
/// Keep this byte-identical to what `provider()` reports: a factory that looks rosier
/// than its own provider is exactly the drift the contract exists to prevent.
///
/// The per-cell evidence lives here rather than in `CapabilitySnapshot::confirmed`,
/// because the adapter builds that snapshot and has no way to receive a confirmation
/// map — a driver's confirmation claim would have to be added to the adapter first.
pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet {
        stateful_session: Availability::Unsupported,
        namespace_switch: NamespaceSwitch::Unsupported,
        context_observation: ContextObservation::Unsupported,
        transaction_observation: TransactionObservation::Unsupported,
        session_scoped_handles: SessionScopedHandleSupport::Unsupported,
        reset_for_reuse: ResetForReuse::Unsupported,
        // See the table: the adapter owns this cell and pins it to `Unknown`.
        precise_cancel: PreciseCancelSupport::Unknown,
        snapshots: SnapshotSupport::Unsupported,
        transactions: TransactionSupport {
            // Empty on purpose, for the same reason postgres leaves it empty: a
            // level listed here would make `begin_transaction` accept an option
            // the driver cannot honour.
            isolation_levels: Vec::new(),
            savepoints: Availability::Unsupported,
            max_open_transactions: None,
        },
        // Empty on purpose: see the `ddl_atomicity` row.
        ddl_atomicity: DdlAtomicitySupport::default(),
        // Measured affirmative, backed by the cursor loop and the row count.
        data: DataSupport::StreamingReadWrite,
        // Never measured against the backup workflow. See the `backup` row.
        backup: BackupSupport::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use datazen_driver_api::capabilities::SessionContinuity;
    use datazen_driver_api::capabilities::{
        Availability, CapabilitySet, ContextObservation, NamespaceSwitch, PreciseCancelSupport,
        ResetForReuse, SessionScopedHandleSupport, SnapshotSupport, TransactionObservation,
    };
    use datazen_driver_api::namespace::NamespaceTarget;
    use datazen_driver_api::require_resource_provider;
    use datazen_driver_api::resource::{
        DescribeResourceRequest, IdentityScope, ResourceError, ResourceHandle, ResourcePurpose,
    };
    use datazen_driver_api::{BackupSupport, ConnectionConfig, DataSupport, DatabaseDriverFactory};

    use super::capabilities;
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
        assert_ne!(
            factory().resource_capabilities(),
            CapabilitySet::default(),
            "the inert default claims nothing; `capabilities()` documents a measured refusal \
             for every cell except `data`, so it must differ from the default"
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
            Availability::Unsupported,
            "an Elasticsearch cluster exposes no fixed session, and `connect` stores nothing \
             but a `(client, base)` tuple to prove it"
        );
    }

    /// Every cell of the declaration is pinned here, so a later edit cannot silently
    /// drift away from the evidence table above it.
    ///
    /// The empty containers are asserted too, not skipped: an empty
    /// `isolation_levels` and an empty `ddl_atomicity` map are deliberate
    /// "cannot confirm" answers, and a future edit that fills one of them should have
    /// to come back here.
    #[test]
    fn every_declared_cell_is_the_value_the_evidence_table_justifies() {
        let caps = capabilities();

        assert_ne!(
            caps,
            CapabilitySet::default(),
            "the declaration must differ from the inert default"
        );
        assert_eq!(caps.stateful_session, Availability::Unsupported);
        assert_eq!(caps.namespace_switch, NamespaceSwitch::Unsupported);
        assert_eq!(caps.context_observation, ContextObservation::Unsupported);
        assert_eq!(
            caps.transaction_observation,
            TransactionObservation::Unsupported
        );
        assert_eq!(
            caps.session_scoped_handles,
            SessionScopedHandleSupport::Unsupported
        );
        assert_eq!(caps.reset_for_reuse, ResetForReuse::Unsupported);
        assert_eq!(caps.precise_cancel, PreciseCancelSupport::Unknown);
        assert_eq!(caps.snapshots, SnapshotSupport::Unsupported);
        assert!(
            caps.transactions.isolation_levels.is_empty(),
            "no level is honoured: the driver cannot open a transaction at all"
        );
        assert_eq!(caps.transactions.savepoints, Availability::Unsupported);
        assert_eq!(caps.transactions.max_open_transactions, None);
        assert!(
            caps.ddl_atomicity.by_operation.is_empty(),
            "`DatabaseDriver::ddl_atomicity` is not overridden, so every operation is \
             `Unknown`; the map must stay empty to reproduce that"
        );
        assert_eq!(caps.data, DataSupport::StreamingReadWrite);
        assert_eq!(caps.backup, BackupSupport::Unknown);
    }

    /// The one affirmative cell has to open the three features it claims, and the
    /// negatives have to stay shut.
    ///
    /// `StreamingReadWrite` is the only variant that satisfies `enables_feature`, so
    /// this is where a wrong answer to the `data` row would show up as a product
    /// behaviour change rather than as a quiet mismatch.
    #[test]
    fn the_data_domain_opens_exactly_what_the_cursor_loop_can_deliver() {
        let caps = capabilities();

        assert!(caps.data.enables_row_read());
        assert!(
            caps.data.enables_row_write(),
            "`execute` posts the statement and returns the server's row count"
        );
        assert!(
            caps.data.enables_streaming_results(),
            "`query_stream` pushes each batch before requesting the next cursor page"
        );
        assert!(caps.data.enables_feature());
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
