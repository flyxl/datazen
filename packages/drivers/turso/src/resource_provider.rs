//! Track O: Turso's real [`ResourceProvider`].
//!
//! Turso is reached through the contract's own migration bridge,
//! [`LegacyResourceAdapter`], rather than through a bespoke provider impl. The adapter
//! is what makes the handles real: it opens the connection itself, mints an opaque
//! [`ResourceHandle`] bound to that connection's own id, charges the caller's
//! [`BudgetPort`] *before* any wire work, and releases the charge exactly once on
//! close.
//!
//! Everything Turso cannot honestly perform is refused by name instead of answered with
//! an empty success:
//!
//! * `request_cancel` — the libSQL/HTTP path has no cancel call on the connection, so
//! the adapter answers `CancelDisposition::Unsupported` and never calls the legacy
//! `cancel_query`. That method used to answer `Ok(())`, i.e. a reported cancellation
//! that never happened; Track O turns it into an explicit `DriverError::Unsupported`.
//! * `observe_session` — every field unknown, never back-filled with the acquisition
//! target.
//! * `change_context` — refused, because switching the Turso branch context is a
//! reconnect.
//! * `commit_transaction` / `rollback_transaction` — transactions are resolved inside
//! the driver's own commands.
//!
//! # Namespace
//!
//! Turso is the same shape as rqlite: `has_schema_level` is the default `false`,
//! `TursoDriver::effective_database` falls back to `main` when the configured name is
//! blank, and the database name is quoted as a SQLite schema qualifier. The shape
//! declares an **optional** `Database` level and nothing else, so a `Schema` or
//! `Catalog` is refused by name.

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

/// The namespace Turso actually addresses.
///
/// Turso addresses one namespace level, the attached database.
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

/// The real provider for `turso`, built once and shared.
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
                "turso",
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
        "turso",
        env!("CARGO_PKG_VERSION"),
        runtime_epoch,
        namespace_shape(),
        capabilities(),
    ))
}

/// What this crate can honestly claim, capability by capability.
///
/// `CapabilitySet` is fail-closed: every enum's default is its *non*-supporting value,
/// so a driver that declares nothing gates every dependent feature off. Turso has
/// exactly **one** cell it can prove from its own code — `data` — and eleven it
/// deliberately leaves at the non-supporting answer. Every row below gives the declared
/// value and the evidence behind it; a blank is the claim that the evidence is absent,
/// not an oversight. Line numbers are `src/turso.rs` unless stated.
///
/// | cell | declared | evidence |
/// | --- | --- | --- |
/// | `data` | `DataSupport::BufferedReadWrite` | **The one filled cell.** Read: `query` (:317-329) returns a materialized `QueryResult` built by `result_from_json` (:93-…). Write: `execute` (:388-…) posts a statement through `pipeline` and returns `rows_affected`. *Not* streaming: `pipeline` (:30-53) reads the whole body with `resp.text()` and parses it into a `serde_json::Value` before returning; `query_stream` (:351-377) awaits that same full body at :362 and only then replays the already-materialized `Vec` through `stream_decoded_rows` (:365-374). No row is emitted before the whole result set is in memory, so `streamingResults` does not hold and `StreamingReadWrite` would be a false claim. |
/// | `backup` | `BackupSupport::Unknown` (blank) | This crate contains no backup or restore path — `backup` appears nowhere in `src/` but a test name. `ui/meta.ts` nevertheless sets `supportsBackup: true`, which only gates the frontend Backup window and is backed by no Rust here. `ArtifactOnly`/`ArtifactAndRestore` would repeat that unbacked claim in the contract; `Unsupported` would assert we probed a server capability this crate never touched. The contradiction is recorded here rather than papered over. |
/// | `stateful_session` | `Availability::Unknown` (blank) | The driver's entire state is `clients: RwLock<HashMap<String, (reqwest::Client, String)>>` (:11-12) — an HTTP client and a base URL under a `turso_<uuid>` pool id minted at `connect` (:250). Every statement is an independently constructed `POST {base}/v2/pipeline` (:35-39). The driver never issues a session-establishing call nor stores a server-assigned session id, so whether Turso binds a resource to a server-side session is **unmeasured**, not measured-absent. Pinned by `tests::a_described_resource_is_never_mistaken_for_a_fixed_reusable_session`. |
/// | `namespace_switch` | `NamespaceSwitch::Unknown` (blank) | The module docs above justify refusing `change_context` because "switching the Turso branch context is a reconnect", but the code does not back that sentence: the database name is never stored on the resource. `effective_database` (:60-67) resolves blank to `main` and `quote_schema` (:71-73) renders it as a per-statement qualifier used by `list_tables_sql`/`table_info_sql` (:76-91), while the resource is the base URL fixed at `connect` (:249-257), whose only mutation is `disconnect`'s `remove` (:262). So the driver can neither switch a live session in place nor show that a switch needs a replacement resource — the name rides along on each statement. `Unknown` keeps the gate shut; so do all three non-`InPlace` variants, and `switches_in_place` is true only for `InPlace`. |
/// | `context_observation` | `ContextObservation::Unsupported` | Both the default and the measured answer: the crate contains no context read-back call, and the module docs record that `observe_session` reports every field unknown rather than back-filling the acquisition target. |
/// | `transaction_observation` | `TransactionObservation::Unsupported` | Default and truthful. This driver issues no `BEGIN`/`COMMIT`/`ROLLBACK` — none appears anywhere in the crate — and mints no transaction handle; the resource port refuses commit and rollback because transactions are resolved inside the driver's own commands. |
/// | `session_scoped_handles` | `SessionScopedHandleSupport::Unknown` (blank) | The only map the driver owns is `clients` (:12), whose values are `(reqwest::Client, String)`; there is no cursor, prepared-statement or transaction registry a session-scoped handle could key into. That is suggestive but not a measurement — nothing ever asked for such a handle — so unmeasured beats measured-absent. |
/// | `reset_for_reuse` | `ResetForReuse::Unsupported` | Default and truthful. There is no reset path: `disconnect` (:261-264) removes the map entry and nothing else in the crate mutates `clients`, so the adapter has no verified way to return a used resource to a known baseline. |
/// | `precise_cancel` | `PreciseCancelSupport::Unknown` | **Not the driver's cell to declare.** `LegacyResourceAdapter::new` overwrites `precise_cancel` from `DatabaseDriver::supports_query_execution_cancel()` (`packages/driver-api/src/resource_adapter.rs`), so any value written here is replaced before a caller can read it. Turso inherits the trait default `false` and `cancel_query` answers `DriverError::Unsupported` (:419-427), so the adapter installs `Unknown` regardless. `Unknown` is written here because it is the truthful statement about a driver with no execution-handle protocol *and* the value the adapter will install, so the two views agree. |
/// | `snapshots` | `SnapshotSupport::Unsupported` | Default and truthful. The driver opens nothing server-side to hold a read open: one statement in, one fully parsed body out, `Value` dropped at the end of the call. A `SnapshotSupport` variant must describe a snapshot a caller can pin across statements, and no such object exists here; `PerDatabase` would be a branch-pinning claim the contract never asked about. |
/// | `transactions` | `isolation_levels: []`, `savepoints: Unknown`, `max_open_transactions: None` (all blank) | An empty `isolation_levels` is a positive statement — "no level is confirmed" — the same way `DdlAtomicitySupport::atomicity_for` returns `Unknown` for an absent key. Nothing in the crate issues `BEGIN` or `SAVEPOINT`, so no level can be named, savepoint support has never been probed, and no bound on concurrently open transactions exists to state. |
/// | `ddl_atomicity` | `by_operation` left empty (blank) | `TursoDriver` does not override `DatabaseDriver::ddl_atomicity`, so the trait default `DdlAtomicity::Unknown` is the answer for every operation and an empty `by_operation` reports exactly that. `sync_family() == "sqlite"` (:225-227) is lineage, not evidence: it says nothing about how a Turso branch's replicated DDL behaves, so the map stays empty instead of borrowing SQLite's answer. |
///
/// A caller that needs one of the blank cells gets an explicit
/// `ResourceError::CapabilityNotDeclared` instead of a silent empty success. Keep this
/// byte-identical to what `provider()` reports: a factory that looks rosier
/// than its own provider is exactly the drift the contract exists to prevent.
pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet {
        // The one provable cell. Reads return rows and writes report
        // `rows_affected`, but `pipeline` parses the whole response body before
        // `result_from_json` builds a `Vec`, and `query_stream` awaits that same
        // body before replaying it — so the result set is fully materialized
        // before any of it is returned and `streamingResults` does not hold.
        // `StreamingReadWrite` would be a false claim here.
        data: DataSupport::BufferedReadWrite,

        // Blanks, each keeping the default that is also the honest answer.
        // They are spelled out rather than hidden behind `..Default::default()`
        // so that a later "fix" has to edit a value someone already argued for.
        stateful_session: Availability::Unknown, // unmeasured: no session is ever established or stored (:11-12, :250)
        namespace_switch: NamespaceSwitch::Unknown, // the database is a per-statement qualifier, not resource state (:60-91)
        context_observation: ContextObservation::Unsupported, // no read-back call exists in this crate
        transaction_observation: TransactionObservation::Unsupported, // no BEGIN/COMMIT/ROLLBACK anywhere in the crate
        session_scoped_handles: SessionScopedHandleSupport::Unknown, // no cursor/statement registry, but never probed either
        reset_for_reuse: ResetForReuse::Unsupported, // disconnect only removes a map entry (:261-264)
        // Overwritten downstream by the adapter; see the table for why `Unknown`
        // is still the right thing to write here.
        precise_cancel: PreciseCancelSupport::Unknown,
        snapshots: SnapshotSupport::Unsupported, // one statement in, one parsed body out; nothing pinned server-side
        transactions: TransactionSupport::default(), // empty isolation levels, unknown savepoints, no bound
        ddl_atomicity: DdlAtomicitySupport::default(), // no per-operation evidence; the trait default answers `Unknown`
        backup: BackupSupport::Unknown, // no backup code here at all, despite `ui/meta.ts`
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use datazen_driver_api::capabilities::{
        CapabilitySet, PreciseCancelSupport, SessionContinuity,
    };
    use datazen_driver_api::namespace::NamespaceTarget;
    use datazen_driver_api::require_resource_provider;
    use datazen_driver_api::resource::{
        DescribeResourceRequest, IdentityScope, ResourceError, ResourceHandle, ResourcePurpose,
    };
    use datazen_driver_api::{BackupSupport, ConnectionConfig, DataSupport, DatabaseDriverFactory};

    use crate::resource_provider::capabilities;
    use crate::{ConnectionHandle, DatabaseDriver, DriverError, TursoDriver, TursoFactory};

    fn factory() -> TursoFactory {
        TursoFactory
    }

    fn config() -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "cfg-1",
            "name": "turso",
            "databaseType": "turso",
            "database": "main",
        }))
        .expect("a minimal config deserializes")
    }

    #[test]
    fn the_factory_reaches_a_real_provider_instead_of_a_missing_one() {
        let provider = require_resource_provider(&factory())
            .expect("Turso is migrated, so the provider must be reachable");
        assert_eq!(provider.provider_id(), "turso");
        assert_eq!(
            provider.namespace_shape().levels.len(),
            1,
            "Turso addresses one namespace level, the attached database"
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
        // This assertion used to read `!declares_anything()`: the placeholder state
        // where the whole set was `CapabilitySet::default()`. `declares_anything()`
        // is literally `*self != Self::default()`, so keeping the old direction
        // would forbid the one cell Turso can prove. It is inverted, not
        // removed — the same gate, now pointed at the real state, so the set can
        // never silently collapse back to all-defaults without a red test.
        assert!(
            factory().resource_capabilities().declares_anything(),
            "Turso declares the one cell it can prove (buffered read/write); \
             an all-default set would mean the declaration was dropped"
        );
        assert!(
            !factory()
                .resource_capabilities()
                .precise_cancel
                .accepts_precise_cancel(),
            "Turso has no execution-handle cancel protocol"
        );
        assert!(registry.require_precise_cancel().is_err());
        assert_eq!(
            factory().resource_capabilities().stateful_session,
            datazen_driver_api::capabilities::Availability::Unknown,
            "a Turso HTTP request is stateless"
        );
    }

    #[test]
    fn the_declaration_claims_the_one_thing_turso_can_prove() {
        let declared = capabilities();

        assert_ne!(
            declared,
            CapabilitySet::default(),
            "the declaration must not collapse back to every cell at its default"
        );
        assert_eq!(
            declared.data,
            DataSupport::BufferedReadWrite,
            "reads and writes are real, but pipeline parses the whole body before any \
             row is emitted, so streamingResults does not hold"
        );
        assert!(
            !declared.data.enables_streaming_results(),
            "declaring BufferedReadWrite while streaming results were enabled would be \
             the one claim this crate cannot back"
        );
        assert_eq!(
            declared.backup,
            BackupSupport::Unknown,
            "this crate has no backup code at all, so it must not claim an artifact \
             or a restore just because ui/meta.ts sets supportsBackup"
        );
        assert!(
            !declared.namespace_switch.switches_in_place(),
            "the database is a per-statement SQL qualifier, not state on the resource"
        );
        assert_eq!(
            declared.precise_cancel,
            PreciseCancelSupport::Unknown,
            "no execution-handle protocol exists; the adapter overwrites this cell with \
             the same value, which is what keeps the two views equal"
        );
    }

    #[test]
    fn the_capability_snapshot_reports_identity_without_inventing_confirmations() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();

        assert_eq!(registry.snapshot.driver_id, "turso");
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
            "nothing was actually confirmed for Turso, and an empty record says so"
        );
    }

    #[tokio::test]
    async fn a_described_resource_is_never_mistaken_for_a_fixed_reusable_session() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let descriptor = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty().with_database("main"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect("describe_resource must succeed without touching the network");

        assert!(!descriptor.is_fixed_session());
        assert_eq!(
            descriptor.session_continuity,
            SessionContinuity::Unknown,
            "Turso cannot observe a session, so continuity stays unknown"
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
            "Turso cannot prove a resource is back at its baseline after use"
        );
    }

    #[tokio::test]
    async fn a_namespace_level_turso_does_not_have_is_refused_by_name() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let err = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty()
                    .with_database("main")
                    .with_schema("dbo"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect_err("the namespace shape must reject a level turso cannot address");

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
        let driver = TursoDriver::new();
        let err = driver
            .cancel_query(&ConnectionHandle {
                id: "connection-1".into(),
                pool_id: "turso-pool".into(),
            })
            .await
            .expect_err("Turso cannot cancel; Ok(()) was the defect Track O removes");

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
            "Turso has no execution-handle protocol to advertise"
        );
        assert!(!TursoDriver::new().supports_query_execution_cancel());
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
            .expect("turso declares a real provider");
        let second = factory()
            .resource_provider()
            .expect("turso declares a real provider");
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
