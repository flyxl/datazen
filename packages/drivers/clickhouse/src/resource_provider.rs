//! Track O: ClickHouse's real [`ResourceProvider`].
//!
//! ClickHouse is reached through the contract's own migration bridge,
//! [`LegacyResourceAdapter`], rather than through a bespoke provider impl. The adapter
//! is what makes the handles real: it opens the connection itself, mints an opaque
//! [`ResourceHandle`] bound to that connection's own id, charges the caller's
//! [`BudgetPort`] *before* any wire work, and releases the charge exactly once on
//! close.
//!
//! Everything ClickHouse cannot honestly perform is refused by name instead of answered
//! with an empty success:
//!
//! * `request_cancel` — this driver's HTTP interface exposes no cancel call at all, so
//! the adapter answers `CancelDisposition::Unsupported` and never calls the legacy
//! `cancel_query`. That method used to answer `Ok(())`, i.e. a reported cancellation
//! that never happened; Track O turns it into an explicit `DriverError::Unsupported`.
//! * `observe_session` — every field unknown, never back-filled with the acquisition
//! target.
//! * `change_context` — refused, because changing the ClickHouse user context is a
//! reconnect.
//! * `commit_transaction` / `rollback_transaction` — ClickHouse transactions are
//! per-statement, so the port refuses rather than guessing an outcome.
//!
//! # Namespace
//!
//! `DatabaseDriver::has_schema_level` is the default `false` and
//! `ClickHouseDriver::qualify_sql_target` calls `sql_target::qualify_sql` with the
//! database only; the many `schema` mentions in this crate are ClickHouse's own DDL
//! object names, not a namespace level. The shape declares an **optional** `Database`
//! level and nothing else, so a `Schema` or `Catalog` is refused by name instead of
//! being dropped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use datazen_driver_api::capabilities::{
    Availability, CapabilitySet, ContextObservation, DdlAtomicitySupport, NamespaceSwitch,
    PreciseCancelSupport, ResetForReuse, SessionScopedHandleSupport, SnapshotSupport,
    TransactionObservation as TransactionObservationCap, TransactionSupport,
};
use datazen_driver_api::capability_domains::{BackupSupport, DataSupport};
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

/// The namespace ClickHouse actually addresses.
///
/// ClickHouse addresses one namespace level, the database.
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

/// The real provider for `clickhouse`, built once and shared.
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
            Arc::new(adapter(driver, runtime_epoch))
        })
        .clone()
}

/// Build the adapter and attach this crate's evidence table.
///
/// Both construction sites go through here on purpose: the memoized provider and
/// the stray one have to describe the *same* declaration, otherwise a test that
/// compares them would be comparing two different capability claims.
fn adapter(driver: Arc<dyn DatabaseDriver>, runtime_epoch: u64) -> LegacyResourceAdapter {
    LegacyResourceAdapter::new(
        driver,
        "clickhouse",
        env!("CARGO_PKG_VERSION"),
        runtime_epoch,
        namespace_shape(),
        capabilities(),
    )
    .with_evidence(capability_evidence())
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
    Arc::new(adapter(driver, runtime_epoch))
}

/// What this crate can honestly claim, capability by capability.
///
/// Every row below is a claim about code that **already exists**. ClickHouse reaches the
/// host through [`LegacyResourceAdapter`], so a cell is only filled when the path that
/// would have to implement it is visible in `src/clickhouse.rs`, in this file, or in the
/// adapter itself. What cannot be proven stays `Unsupported`/`Unknown`, and the row says
/// which of the two it is and why — an unmeasured cell and a measured refusal are
/// different diagnoses even though both fail closed.
///
/// | Capability | Declared | Why |
/// |---|---|---|
/// | `stateful_session` | `Unsupported` | `connect` opens no physical session: it files a `reqwest::Client`, the base URL and the database under a fresh UUID and hands that id back (`clickhouse.rs:249-257`). Every statement is an independent `POST` that re-sends `database=` (`clickhouse.rs:83-87`). Measured absent, not merely unconfirmed. |
/// | `namespace_switch` | `RequiresReplacement` | `has_multi_database` is `true` (`clickhouse.rs:189-191`), so another database *is* reachable — but it is bound once at `connect` (`clickhouse.rs:255`) and `effective_database` only ever reads it (`clickhouse.rs:49-61`). The mutating `use_database` path was removed, so a live resource cannot move; only a freshly acquired one can. |
/// | `context_observation` | `Unsupported` | `LegacyResourceAdapter::observe_session` returns `SessionObservation::unobservable()` on every call (`resource_adapter.rs:397-402`), and `execute_on_resource` pins `context_before`/`context_after` to `unobserved()` (`resource_adapter.rs:377-378`). ClickHouse has no read-back API that could replace it. |
/// | `transaction_observation` | `Unsupported` | `commit_transaction`/`rollback_transaction` error on every call (`resource_adapter.rs:429-449`), and `execute_on_resource` hardcodes `TransactionState::Unknown` / `transaction_id: None` / `effect: None` (`resource_adapter.rs:380-382`). A ClickHouse transaction is per-statement, so there is no outcome to read back. |
/// | `session_scoped_handles` | `Unsupported` | `execute_on_resource` returns an empty `session_handles` because this path registers none (`resource_adapter.rs:387`), and ClickHouse exposes no prepared-statement, temporary-table or user-variable handle for a caller to hold. |
/// | `reset_for_reuse` | `Unsupported` | `reset_resource` always answers `ResetDisposition::Discard` (`resource_adapter.rs:499-506`); there is no verified baseline replay, so `Verified` cannot be claimed. |
/// | `precise_cancel` | `Unknown` | Not this function's cell to set — see the note below. |
/// | `snapshots` | `Unsupported` | neither ClickHouse nor the adapter implements `begin_read_snapshot`, so the trait default answers `DriverError::Unsupported` (`traits.rs:734-741`). With no point-in-time read at all, there is no scope to declare. |
/// | `transactions.isolation_levels` | empty | `DatabaseDriver::begin_transaction` is the default and errors (`traits.rs:602-609`), so `LegacyResourceAdapter::begin_transaction` (`resource_adapter.rs:416-425`) can never open one. Empty means "no level can be honoured", not "nobody wrote anything". |
/// | `transactions.savepoints` | `Unsupported` | the same unreachable path; this crate issues no `SAVEPOINT`. |
/// | `transactions.max_open_transactions` | `None` | there is no transaction registry at all, so there is no number to report — which is not the same as "unbounded". |
/// | `ddl_atomicity.by_operation` | empty | `DatabaseDriver::ddl_atomicity` is not overridden here, so it answers `DdlAtomicity::Unknown` (`traits.rs:155-158`). `DdlAtomicitySupport::atomicity_for` fails closed to `Unknown` for any absent key (`capabilities.rs:240-245`), which is the correct answer for every operation: the caller wraps nothing and asks instead of assuming. |
/// | `data` | `BufferedReadWrite` | rows are read (`query` then `result_from_json`, `clickhouse.rs:467-480` and `:106-145`) and written (`execute`, `clickhouse.rs:535-541`). But `http_query` awaits `resp.text()` (`clickhouse.rs:95-98`) and `stream_json` (`clickhouse.rs:147-184`) only feeds an already-materialized `serde_json::Value` into the batcher, so nothing is pulled incrementally over the wire and `streamingResults` does not hold. |
/// | `backup` | `Unsupported` | `command_definitions` (`clickhouse.rs:562-570`) is a closed list of query / execute / query_stream plus the schema-catalog commands; nothing produces or consumes an artifact. |
///
/// # `precise_cancel` is declared here, but the adapter owns it
///
/// `LegacyResourceAdapter::new` overwrites this one cell from the driver's own
/// `supports_query_execution_cancel()` (`resource_adapter.rs:148-152`), which for
/// ClickHouse is the trait default `false` (`traits.rs:817-819`); `cancel_query` agrees
/// by answering `DriverError::Unsupported` instead of reporting a cancellation that never
/// happened (`clickhouse.rs:555-560`). The effective value is `Unknown` whatever is
/// written here, and `Unknown` is the honest answer for the set too — `Supported` would
/// be a claim no code in this crate makes, and one the adapter would erase anyway.
///
/// A caller that needs one of the unclaimed cells gets an explicit
/// `ResourceError::CapabilityNotDeclared` instead of a silent empty success. Keep this
/// byte-identical to what `provider()` reports: a factory that looks rosier than its own
/// provider is exactly the drift the contract exists to prevent.
pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet {
        // Measured absent: `connect` never opens a server-side session
        // (clickhouse.rs:249-257) and every statement re-sends `database=` as a request
        // parameter (clickhouse.rs:83-87), so there is nothing to pin state onto.
        stateful_session: Availability::Unsupported,
        // Another database is reachable, but only through a replacement resource: the
        // database is bound once at `connect` (clickhouse.rs:255) and nothing mutates it
        // afterwards (clickhouse.rs:49-61).
        namespace_switch: NamespaceSwitch::RequiresReplacement,
        // The adapter answers `unobservable()` on every call
        // (resource_adapter.rs:397-402) and ClickHouse has no read-back API to replace it.
        context_observation: ContextObservation::Unsupported,
        // Commit and rollback are refused by name (resource_adapter.rs:429-449) because
        // a ClickHouse transaction is per-statement and its outcome cannot be read back.
        transaction_observation: TransactionObservationCap::Unsupported,
        // The execution path registers no session-scoped handle
        // (resource_adapter.rs:387).
        session_scoped_handles: SessionScopedHandleSupport::Unsupported,
        // `reset_resource` only ever answers `Discard` (resource_adapter.rs:499-506);
        // there is no verified path back to an initialization baseline.
        reset_for_reuse: ResetForReuse::Unsupported,
        // Overwritten by the adapter from `supports_query_execution_cancel()`, which is
        // `false` here (traits.rs:817-819, corroborated by clickhouse.rs:555-560).
        // `Unknown` keeps the set honest on its own terms instead of only by accident of
        // the adapter's correction.
        precise_cancel: PreciseCancelSupport::Unknown,
        // Neither this crate nor the adapter implements `begin_read_snapshot`; the trait
        // default errors with `DriverError::Unsupported` (traits.rs:734-741).
        snapshots: SnapshotSupport::Unsupported,
        transactions: TransactionSupport {
            // Empty on purpose. `begin_transaction` is the trait default and errors
            // (traits.rs:602-609), so `LegacyResourceAdapter::begin_transaction`
            // (resource_adapter.rs:416-425) can never open one. The contract reads an
            // empty list as "cannot confirm any level", which is exactly right: no
            // `SET TRANSACTION` is ever issued, so naming a level would advertise an
            // option the driver silently ignores.
            isolation_levels: Vec::new(),
            // No `SAVEPOINT` is ever issued by this crate.
            savepoints: Availability::Unsupported,
            // No number at all: there is no transaction registry, so this reads as
            // "unmeasured", not "unbounded".
            max_open_transactions: None,
        },
        // Left empty on purpose. `DatabaseDriver::ddl_atomicity` is not overridden, so it
        // answers `Unknown` (traits.rs:155-158), and `atomicity_for` fails closed to
        // `Unknown` for any absent key (capabilities.rs:240-245). Filing a value per
        // operation would assert a multi-statement DDL atomicity nobody measured.
        ddl_atomicity: DdlAtomicitySupport::default(),
        // `BufferedReadWrite`, not `StreamingReadWrite`: `http_query` awaits
        // `resp.text()` (clickhouse.rs:95-98), so the whole result set is one string before
        // `stream_json` (clickhouse.rs:147-184) ever sees it. Read and write are both
        // real — this only denies `streamingResults`.
        data: DataSupport::BufferedReadWrite,
        // `command_definitions` (clickhouse.rs:562-570) is a closed list with no artifact
        // producer or consumer in it, so the absence is measurable rather than unknown.
        backup: BackupSupport::Unsupported,
    }
}

/// The evidence behind every cell [`capabilities`] fills, as the snapshot records it.
///
/// This is the machine-readable twin of the table above: the table is what a human
/// reads, this is what `CapabilitySnapshot::evidence_gaps` and any future consumer
/// read. Both are derived from the same code citations, and both must agree — a cell
/// that is filled here without a citation above would be an unfalsifiable claim.
///
/// Every one of the twelve cells is listed, including the two that stay blank.
/// A blank cell is not an absence of a finding: `transactions` and `ddlAtomicity`
/// are blank because the paths that would fill them are *measured* unreachable,
/// and that measurement is exactly what the evidence table exists to preserve.
/// Entries marked `declined:` record a positive decision not to claim, which is
/// the opposite of a silently empty map.
pub(crate) fn capability_evidence() -> Vec<(&'static str, String)> {
    vec![
        (
            "statefulSession",
            "Unsupported: `connect` opens no physical session. It files a reqwest::Client, \
             the base URL and the database under a fresh `clickhouse_<uuid>` key \
             (clickhouse.rs:249-257) and every statement is an independent POST that \
             re-sends `database=` as a request parameter (clickhouse.rs:83-87). There is \
             no server-side session for a caller to pin state onto. Measured absent."
                .to_string(),
        ),
        (
            "namespaceSwitch",
            "RequiresReplacement: `has_multi_database` is `true` (clickhouse.rs:189-191), so \
             another database is genuinely reachable, but it is bound once inside `connect` \
             (clickhouse.rs:255) and `effective_database` only ever reads it \
             (clickhouse.rs:49-61). The mutating `use_database` path was removed, so a live \
             resource cannot move — only a freshly acquired one can."
                .to_string(),
        ),
        (
            "contextObservation",
            "Unsupported: `observe_session` answers `SessionObservation::unobservable()` on \
             every call (resource_adapter.rs:397-402) and `execute_on_resource` pins \
             `context_before`/`context_after` to `unobserved()` (resource_adapter.rs:377-378). \
             ClickHouse exposes no session read-back API that could replace either."
                .to_string(),
        ),
        (
            "transactionObservation",
            "Unsupported: `commit_transaction` (resource_adapter.rs:429-438) and \
             `rollback_transaction` (resource_adapter.rs:440-449) both refuse by name because \
             the outcome cannot be read back, and `execute_on_resource` hardcodes \
             `TransactionState::Unknown` with `transaction_id: None` and `effect: None` \
             (resource_adapter.rs:380-382). A ClickHouse transaction is per-statement, so \
             there is no outcome to observe."
                .to_string(),
        ),
        (
            "sessionScopedHandles",
            "Unsupported: `execute_on_resource` returns an empty `session_handles` because this \
             path registers none (resource_adapter.rs:387), and the crate exposes no \
             prepared-statement, temporary-table or user-variable handle a caller could hold."
                .to_string(),
        ),
        (
            "resetForReuse",
            "Unsupported: `reset_resource` always answers `ResetDisposition::Discard` \
             (resource_adapter.rs:499-506). There is no verified baseline replay, so `Verified` \
             would hand back a resource whose state nobody checked."
                .to_string(),
        ),
        (
            "preciseCancel",
            "Unknown, and the adapter owns this cell. `LegacyResourceAdapter::new` overwrites \
             it from `supports_query_execution_cancel()` (resource_adapter.rs:148-152), which \
             for ClickHouse is the trait default `false` (traits.rs:817-819); `cancel_query` \
             agrees by refusing (clickhouse.rs:555-560) instead of reporting a cancellation \
             that never happened. The value written here is the value the adapter installs, so \
             the two views agree rather than merely not contradicting."
                .to_string(),
        ),
        (
            "snapshots",
            "Unsupported: neither this crate nor the adapter implements `begin_read_snapshot`, \
             so the trait default answers `DriverError::Unsupported` (traits.rs:734-741). With \
             no point-in-time read at all there is no snapshot scope to declare."
                .to_string(),
        ),
        (
            "transactions",
            "declined: the declaration is deliberately blank and each blank is a measurement, not \
             an omission. `DatabaseDriver::begin_transaction` is the trait default and errors \
             (traits.rs:602-609), so the adapter's `begin_transaction` \
             (resource_adapter.rs:416-425) can never open one. `isolation_levels: []` therefore \
             reads as 'no level can be honoured' — naming a level would advertise an option the \
             driver silently ignores, since no `SET TRANSACTION` is ever issued. \
             `savepoints: Unsupported` follows from the same unreachable path: this crate issues \
             no `SAVEPOINT`. `max_open_transactions: None` is 'unmeasured', not 'unbounded' — \
             there is no transaction registry in this crate to count."
                .to_string(),
        ),
        (
            "ddlAtomicity",
            "declined: the map is empty on purpose. `DatabaseDriver::ddl_atomicity` is not \
             overridden in this crate, so it answers `Unknown` (traits.rs:156-158), and \
             `DdlAtomicitySupport::atomicity_for` fails closed to `Unknown` for any absent key \
             (capabilities.rs:240-245). Filing a value per operation would assert a \
             multi-statement DDL atomicity that nobody has measured."
                .to_string(),
        ),
        (
            "data",
            "BufferedReadWrite, not StreamingReadWrite: rows are read (`query` then \
             `result_from_json`, clickhouse.rs:467-480 and :106-145) and written (`execute`, \
             clickhouse.rs:535-541), so both directions are real — only `streamingResults` is \
             denied. `http_query` awaits `resp.text()` (clickhouse.rs:95-98), so the whole \
             result set is one string before `stream_json` (clickhouse.rs:147-184) ever sees \
             it; nothing is pulled incrementally over the wire."
                .to_string(),
        ),
        (
            "backup",
            "Unsupported: `command_definitions` (clickhouse.rs:562-570) is a closed list of \
             query / execute / query_stream plus the schema-catalog commands, with no artifact \
             producer or consumer in it. The absence is measurable rather than unknown."
                .to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use datazen_driver_api::capabilities::{
        CapabilitySet, NamespaceSwitch, SessionContinuity, SnapshotSupport,
    };
    use datazen_driver_api::capability_domains::{BackupSupport, DataSupport};
    use datazen_driver_api::namespace::NamespaceTarget;
    use datazen_driver_api::require_resource_provider;
    use datazen_driver_api::resource::{
        DescribeResourceRequest, IdentityScope, ResourceError, ResourceHandle, ResourcePurpose,
    };
    use datazen_driver_api::resource_adapter::ADAPTER_EVIDENCE_REVISION;
    use datazen_driver_api::{ConnectionConfig, DatabaseDriverFactory};

    use crate::{
        ClickHouseDriver, ClickHouseFactory, ConnectionHandle, DatabaseDriver, DriverError,
    };

    fn factory() -> ClickHouseFactory {
        ClickHouseFactory
    }

    fn config() -> ConnectionConfig {
        serde_json::from_value(serde_json::json!({
            "id": "cfg-1",
            "name": "clickhouse",
            "databaseType": "clickhouse",
            "database": "default",
        }))
        .expect("a minimal config deserializes")
    }

    #[test]
    fn the_factory_reaches_a_real_provider_instead_of_a_missing_one() {
        let provider = require_resource_provider(&factory())
            .expect("ClickHouse is migrated, so the provider must be reachable");
        assert_eq!(provider.provider_id(), "clickhouse");
        assert_eq!(
            provider.namespace_shape().levels.len(),
            1,
            "ClickHouse addresses one namespace level, the database"
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
        // ClickHouse now declares the cells it can back with code. The `assert_eq!`
        // above already pins the factory to the provider; this records that the
        // declaration is non-empty, so a silent regression back to
        // `CapabilitySet::default()` fails here instead of in production.
        assert!(
            factory().resource_capabilities().declares_anything(),
            "ClickHouse declares what it can prove: a buffered read/write path and a \
             replacement-only namespace switch. An empty set would be under-claiming, \
             not caution."
        );
        assert!(
            !factory()
                .resource_capabilities()
                .precise_cancel
                .accepts_precise_cancel(),
            "ClickHouse has no execution-handle cancel protocol"
        );
        assert!(registry.require_precise_cancel().is_err());
        assert_eq!(
            factory().resource_capabilities().stateful_session,
            datazen_driver_api::capabilities::Availability::Unsupported,
            "a ClickHouse HTTP session is stateless per request, so `connect` provably \
             opens no session for a caller to pin state onto"
        );
    }

    /// Pins every cell the doc table above justifies. If a refactor quietly returned the
    /// function to `CapabilitySet::default()`, all twelve claims would start lying at
    /// once and no other test in this module would notice.
    #[test]
    fn every_declared_cell_matches_the_code_the_doc_comment_cites() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let declared = crate::resource_provider::capabilities();
        let registry = provider.capabilities();

        assert_ne!(
            declared,
            CapabilitySet::default(),
            "the declaration is non-empty on purpose; an all-default set is the silent \
             regression this test exists to make loud"
        );
        assert_eq!(declared.data, DataSupport::BufferedReadWrite);
        assert!(
            declared.data.enables_row_read(),
            "query -> result_from_json"
        );
        assert!(
            declared.data.enables_row_write(),
            "execute reports affected rows"
        );
        assert!(
            !declared.data.enables_streaming_results(),
            "http_query awaits the whole body (clickhouse.rs:95-98) and stream_json only \
             batcher-feeds an already-materialized value, so a long-lived stream would \
             have nothing to pull"
        );
        assert_eq!(
            declared.namespace_switch,
            NamespaceSwitch::RequiresReplacement
        );
        assert!(
            registry.require_in_place_namespace_switch().is_err(),
            "RequiresReplacement opens no in-place gate: require_in_place_namespace_switch \
             (capabilities.rs:447) passes only for InPlace, so declaring it costs no \
             behaviour and claims less than Unsupported would hide"
        );
        assert_eq!(declared.snapshots, SnapshotSupport::Unsupported);
        assert_eq!(declared.backup, BackupSupport::Unsupported);
        assert!(
            declared.transactions.isolation_levels.is_empty(),
            "an empty level list is the measured answer, not an omission: \
             begin_transaction is the trait default and errors (traits.rs:602-609)"
        );
        assert!(
            declared.transactions.max_open_transactions.is_none(),
            "no transaction registry exists, so there is no number to report — which is \
             not the same as unbounded"
        );
        assert!(
            declared.ddl_atomicity.by_operation.is_empty(),
            "atomicity_for fails closed to Unknown for any absent key \
             (capabilities.rs:240-245); a driver that never overrode \
             DatabaseDriver::ddl_atomicity must not file invented per-operation values"
        );
    }

    #[test]
    fn the_capability_snapshot_reports_identity_without_inventing_confirmations() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();

        assert_eq!(registry.snapshot.driver_id, "clickhouse");
        assert_eq!(
            registry.snapshot.driver_version,
            env!("CARGO_PKG_VERSION"),
            "the driver version must be the real crate version, not an invented string"
        );
        assert_eq!(
            registry.snapshot.protocol_version,
            datazen_driver_api::PROTOCOL_VERSION
        );
        assert_eq!(
            registry.snapshot.capability_revision, ADAPTER_EVIDENCE_REVISION,
            "an adapter that carries an evidence table must stamp ADAPTER_EVIDENCE_REVISION; \
             leaving the revision at 0 would claim the snapshot predates any capability module"
        );
        assert_ne!(
            registry.snapshot.capability_revision, 0,
            "revision 0 means 'no capability module existed yet' and must not carry evidence"
        );
        assert_eq!(
            registry.evidence_gaps(),
            Vec::<&'static str>::new(),
            "all twelve capability cells must carry a record — either a real citation, or an \
             explicit 'declined:' record of what was measured and why nothing may be claimed"
        );
    }

    /// The anti-fabrication guard.
    ///
    /// The point of recording evidence is that a cell is no longer a bare assertion in a
    /// struct literal: someone must be able to walk from the claim to the line of code
    /// that justifies it. A record that cites nothing is indistinguishable from a
    /// record somebody made up, so this test requires the citation shape rather than
    /// trusting the surrounding prose.
    #[test]
    fn every_capability_record_cites_the_source_line_it_claims_to_describe() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let registry = provider.capabilities();
        let confirmed = &registry.snapshot.confirmed;

        // Restated here rather than imported, so driver-api changing its own cell list
        // cannot quietly make this test vacuous.
        let cells = [
            "statefulSession",
            "namespaceSwitch",
            "contextObservation",
            "transactionObservation",
            "sessionScopedHandles",
            "resetForReuse",
            "preciseCancel",
            "snapshots",
            "transactions",
            "ddlAtomicity",
            "data",
            "backup",
        ];
        assert_eq!(
            confirmed.len(),
            cells.len(),
            "the evidence table must hold exactly one record per capability cell, no more \
             and no fewer — extra keys would let a cell go unexamined"
        );

        for cell in cells {
            let record = confirmed
                .get(cell)
                .unwrap_or_else(|| panic!("capability cell `{cell}` has no evidence record"));
            assert!(
                cites_a_source_line(record),
                "evidence for `{cell}` cites no `file.rs:NNN`, so it cannot be checked \
                 against the code: {record}"
            );
        }
    }

    /// True when the text points at a concrete source line, i.e. it contains
    /// `something.rs:` immediately followed by a digit.
    fn cites_a_source_line(text: &str) -> bool {
        text.match_indices(".rs:").any(|(at, _)| {
            text[at + 4..]
                .chars()
                .next()
                .is_some_and(|c: char| c.is_ascii_digit())
        })
    }

    #[tokio::test]
    async fn a_described_resource_is_never_mistaken_for_a_fixed_reusable_session() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let descriptor = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty().with_database("default"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect("describe_resource must succeed without touching the network");

        assert!(!descriptor.is_fixed_session());
        assert_eq!(
            descriptor.session_continuity,
            SessionContinuity::Unknown,
            "ClickHouse cannot observe a session, so continuity stays unknown"
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
            "ClickHouse cannot prove a resource is back at its baseline after use"
        );
    }

    #[tokio::test]
    async fn a_namespace_level_clickhouse_does_not_have_is_refused_by_name() {
        let provider = require_resource_provider(&factory()).expect("provider is reachable");
        let err = provider
            .describe_resource(&DescribeResourceRequest {
                connection_config: config(),
                identity_scope: IdentityScope::default(),
                target: NamespaceTarget::empty()
                    .with_database("default")
                    .with_schema("dbo"),
                purpose: ResourcePurpose::InteractiveQuery,
            })
            .await
            .expect_err("the namespace shape must reject a level clickhouse cannot address");

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
        let driver = ClickHouseDriver::new();
        let err = driver
            .cancel_query(&ConnectionHandle {
                id: "connection-1".into(),
                pool_id: "clickhouse-pool".into(),
            })
            .await
            .expect_err("ClickHouse cannot cancel; Ok(()) was the defect Track O removes");

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
            "ClickHouse has no execution-handle protocol to advertise"
        );
        assert!(!ClickHouseDriver::new().supports_query_execution_cancel());
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
            .expect("clickhouse declares a real provider");
        let second = factory()
            .resource_provider()
            .expect("clickhouse declares a real provider");
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
