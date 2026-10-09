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

/// The real provider for `duckdb`, built once and shared.
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
        "duckdb",
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
/// Every row below is a claim about code that **already exists**. DuckDB reaches the
/// host through [`LegacyResourceAdapter`], so a cell is only filled when the path that
/// would have to implement it is visible in `src/duckdb.rs`, in this file, or in the
/// adapter itself. What cannot be proven stays `Unsupported`/`Unknown`, and the row says
/// which of the two it is and why — an unmeasured cell and a measured refusal are
/// different diagnoses even though both fail closed.
///
/// | Capability | Declared | Why |
/// |---|---|---|
/// | `stateful_session` | `Availability::Unknown` (blank) | Unlike the HTTP drivers, this is *not* the "a client is not a session" shape, so the mysql precedent does not carry over: `connect` opens a real in-process `duckdb::Connection` and parks it in the pool under the handle's pool id (`duckdb.rs:161-174`, open at `:165`), so genuine session state exists and `Unsupported` would be a false claim. `Supported` would instead require a session the host can pin state onto and resume, and this crate implements no such thing — the adapter refuses `change_context` outright (`resource_adapter.rs:407-414`) and exposes no session handle to carry one. Neither direction is backed, so `Unknown` keeps the gate shut; §6.1 is explicit that a pool of size one is not a fixed session. |
/// | `namespace_switch` | `Unsupported` | DuckDB does not override `has_multi_database`, so the trait default `false` applies (`traits.rs:133-135`), and `get_databases` is a hardcoded `["main", "temp"]` (`duckdb.rs:181-183`). Any other database is reachable only as an `ATTACH`ed catalog on the very connection `connect` opened (`duckdb.rs:88-101`). The adapter refuses `change_context` outright (`resource_adapter.rs:407-414`), so an acquired resource provably cannot move. |
/// | `context_observation` | `Unsupported` | `LegacyResourceAdapter::observe_session` returns `SessionObservation::unobservable()` on every call (`resource_adapter.rs:397-402`), and `execute_on_resource` pins `context_before`/`context_after` to `unobserved()` (`resource_adapter.rs:377-378`). No code path reads DuckDB's session state back. |
/// | `transaction_observation` | `Unsupported` | `commit_transaction`/`rollback_transaction` error on every call (`resource_adapter.rs:429-449`), and `execute_on_resource` hardcodes `TransactionState::Unknown` / `transaction_id: None` / `effect: None` (`resource_adapter.rs:380-382`). Legacy DuckDB resolves transactions inside its own statements and the outcome cannot be read back afterwards. |
/// | `session_scoped_handles` | `Unsupported` | `execute_on_resource` returns an empty `session_handles` because this path registers none (`resource_adapter.rs:387`), and this crate exposes no prepared-statement, temporary-table or cursor handle for a caller to hold. |
/// | `reset_for_reuse` | `Unsupported` | `reset_resource` always answers `ResetDisposition::Discard` (`resource_adapter.rs:499-506`); there is no verified baseline replay, so `Verified` cannot be claimed. |
/// | `precise_cancel` | `Unknown` | Not this function's cell to set — see the note below. |
/// | `snapshots` | `Unsupported` | neither DuckDB nor the adapter implements `begin_read_snapshot`, so the trait default answers `DriverError::Unsupported` (`traits.rs:734-741`). With no point-in-time read at all, there is no scope to declare. |
/// | `transactions.isolation_levels` | empty | `DatabaseDriver::begin_transaction` is the default and errors (`traits.rs:602-609`), so `LegacyResourceAdapter::begin_transaction` (`resource_adapter.rs:416-425`) can never open one. Empty means "no level can be honoured", not "nobody wrote anything". |
/// | `transactions.savepoints` | `Unsupported` | the same unreachable path; this crate issues no `SAVEPOINT`. |
/// | `transactions.max_open_transactions` | `None` | there is no transaction registry at all, so there is no number to report — which is not the same as "unbounded". |
/// | `ddl_atomicity.by_operation` | empty | `DatabaseDriver::ddl_atomicity` is not overridden here, so it answers `DdlAtomicity::Unknown` (`traits.rs:155-158`). `DdlAtomicitySupport::atomicity_for` fails closed to `Unknown` for any absent key (`capabilities.rs:240-245`), which is the correct answer for every operation: the caller wraps nothing and asks instead of assuming. |
/// | `data` | `StreamingReadWrite` | rows are read (`query`, `duckdb.rs:367-408`) and written (`execute`, `duckdb.rs:549-557`), and `query_stream` pulls them one at a time out of a live `Rows` cursor into the batcher (`duckdb.rs:479-538`); the test `query_stream_emits_multiple_row_batches_when_unlimited` (`duckdb.rs:804`) proves several `Rows` batches are emitted from one statement. That is incremental over the wire, which is exactly what `resp.text()`-style buffering denies ClickHouse. |
/// | `backup` | `Unsupported` | `command_definitions` (`duckdb.rs:580-589`) is a closed list of query / execute / query_stream plus the schema-catalog and schema-object commands; nothing produces or consumes an artifact. |
///
/// # `precise_cancel` is declared here, but the adapter owns it
///
/// `LegacyResourceAdapter::new` overwrites this one cell from the driver's own
/// `supports_query_execution_cancel()` (`resource_adapter.rs:148-152`), which for
/// DuckDB is the trait default `false` (`traits.rs:817-819`); `cancel_query` agrees by
/// answering `DriverError::Unsupported` instead of reporting a cancellation that never
/// happened (`duckdb.rs:573-578`). The effective value is `Unknown` whatever is written
/// here, and `Unknown` is the honest answer for the set too — `Supported` would be a
/// claim no code in this crate makes, and one the adapter would erase anyway.
///
/// A caller that needs one of the unclaimed cells gets an explicit
/// `ResourceError::CapabilityNotDeclared` instead of a silent empty success. Keep this
/// byte-identical to what `provider()` reports: a factory that looks rosier than its own
/// provider is exactly the drift the contract exists to prevent.
pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet {
        // Not `Unsupported`: `connect` really does open one `Connection` per handle
        // (duckdb.rs:161-174). Not `Supported` either: nothing confirms the host may
        // pin state onto it and resume it, so this stays the measured-unknown answer
        // rather than a false claim of absence.
        stateful_session: Availability::Unknown,
        // Measured absent. `has_multi_database` is the trait default `false`
        // (traits.rs:133-135), `get_databases` is hardcoded (duckdb.rs:181-183), and any
        // other database is only an ATTACHed catalog on this connection
        // (duckdb.rs:88-101). `change_context` is refused outright
        // (resource_adapter.rs:407-414), so no acquired resource can move.
        namespace_switch: NamespaceSwitch::Unsupported,
        // The adapter answers `unobservable()` on every call
        // (resource_adapter.rs:397-402) and no path reads DuckDB session state back.
        context_observation: ContextObservation::Unsupported,
        // Commit and rollback are refused by name (resource_adapter.rs:429-449): legacy
        // DuckDB resolves transactions inside its own statements, so the outcome cannot
        // be read back.
        transaction_observation: TransactionObservationCap::Unsupported,
        // The execution path registers no session-scoped handle
        // (resource_adapter.rs:387).
        session_scoped_handles: SessionScopedHandleSupport::Unsupported,
        // `reset_resource` only ever answers `Discard` (resource_adapter.rs:499-506);
        // there is no verified path back to an initialization baseline.
        reset_for_reuse: ResetForReuse::Unsupported,
        // Overwritten by the adapter from `supports_query_execution_cancel()`, which is
        // `false` here (traits.rs:817-819, corroborated by duckdb.rs:573-578).
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
        // operation would assert a multi-statement DDL atomicity nobody measured — the
        // three drivers that do file one measured it: postgres.rs:80, sqlite.rs:221 and
        // mysql.rs:756. This crate is not among them.
        ddl_atomicity: DdlAtomicitySupport::default(),
        // The one variant that satisfies `enables_feature()`: rows read, rows written,
        // and `query_stream` genuinely pulls them incrementally out of a live cursor
        // (duckdb.rs:479-538), which the multiple-batch test at duckdb.rs:804 confirms.
        data: DataSupport::StreamingReadWrite,
        // `command_definitions` (duckdb.rs:580-589) is a closed list with no artifact
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
/// Every one of the twelve cells is listed, including the three that stay blank.
/// A blank cell is not an absence of a finding: `statefulSession`, `transactions`
/// and `ddlAtomicity` are blank because the paths that would fill them are
/// *measured* unreachable or unbacked, and that measurement is exactly what the
/// evidence table exists to preserve. Entries marked `declined:` record a positive
/// decision not to claim, which is the opposite of a silently empty map.
pub(crate) fn capability_evidence() -> Vec<(&'static str, String)> {
    vec![
        (
            "statefulSession",
            "Unknown, and deliberately not `Unsupported`. `connect` really does open one \
             in-process `duckdb::Connection` per handle and parks it in the pool under that \
             handle's pool id (duckdb.rs:161-174, the open at duckdb.rs:165), so a real \
             session object exists and a measured 'absent' claim would be false. \
             `Supported` is equally unbacked: nothing here lets the host pin state onto that \
             connection and resume it, the adapter refuses `change_context` outright \
             (resource_adapter.rs:407-414), and no session handle is ever minted \
             (resource_adapter.rs:387). Both directions lack evidence, so the gate stays shut."
                .to_string(),
        ),
        (
            "namespaceSwitch",
            "Unsupported: DuckDB does not override `has_multi_database`, so the trait default \
             `false` applies (traits.rs:133-135), and `get_databases` is a hardcoded \
             `[\"main\", \"temp\"]` (duckdb.rs:181-183). Any other database is reachable only \
             as an `ATTACH`ed catalog selected by name on the very connection `connect` \
             opened (duckdb.rs:88-101), and the adapter refuses `change_context` outright \
             (resource_adapter.rs:407-414), so an acquired resource provably cannot move."
                .to_string(),
        ),
        (
            "contextObservation",
            "Unsupported: `observe_session` answers `SessionObservation::unobservable()` on \
             every call (resource_adapter.rs:397-402) and `execute_on_resource` pins \
             `context_before`/`context_after` to `unobserved()` (resource_adapter.rs:377-378). \
             No code path in this crate reads DuckDB's session state back, so there is nothing \
             that could replace either default."
                .to_string(),
        ),
        (
            "transactionObservation",
            "Unsupported: `commit_transaction` (resource_adapter.rs:429-438) and \
             `rollback_transaction` (resource_adapter.rs:440-449) both refuse by name because \
             the outcome cannot be read back, and `execute_on_resource` hardcodes \
             `TransactionState::Unknown` with `transaction_id: None` and `effect: None` \
             (resource_adapter.rs:380-382). Legacy DuckDB resolves transactions inside its \
             own statements, so there is no outcome left to observe."
                .to_string(),
        ),
        (
            "sessionScopedHandles",
            "Unsupported: `execute_on_resource` returns an empty `session_handles` because this \
             path registers none (resource_adapter.rs:387), and this crate exposes no \
             prepared-statement, temporary-table or cursor handle a caller could hold."
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
             this crate does not override, so the trait default `false` applies \
             (traits.rs:817-819); `cancel_query` agrees by refusing (duckdb.rs:573-578) \
             instead of reporting a cancellation that never happened. The value written here \
             is the value the adapter installs, so the two views agree rather than merely not \
             contradicting."
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
             multi-statement DDL atomicity that nobody has measured — unlike the three drivers \
             that do file one."
                .to_string(),
        ),
        (
            "data",
            "StreamingReadWrite, the one affirmative cell: rows are read (`query`, \
             duckdb.rs:367-408) and written (`execute`, duckdb.rs:549-557), and `query_stream` \
             pulls them one at a time out of a live `Rows` cursor, pushing each decoded row \
             through the batcher (`while let Some(row) = rows.next()` then `batcher.push(vals)`, \
             duckdb.rs:515-526) before asking for more. That is incremental over the wire, \
             which is exactly what ClickHouse's `resp.text()`-style buffering denies; the test \
             `query_stream_emits_multiple_row_batches_when_unlimited` (duckdb.rs:804) proves \
             several batches leave one statement."
                .to_string(),
        ),
        (
            "backup",
            "Unsupported: `command_definitions` (duckdb.rs:580-589) is a closed list of query \
             / execute / query_stream plus the schema-catalog and schema-object commands, with \
             no artifact producer or consumer in it. The absence is measurable rather than \
             unknown."
                .to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use datazen_driver_api::capabilities::{
        Availability, CapabilitySet, NamespaceSwitch, SessionContinuity, SnapshotSupport,
    };
    use datazen_driver_api::capability_domains::{BackupSupport, DataSupport};
    use datazen_driver_api::namespace::NamespaceTarget;
    use datazen_driver_api::require_resource_provider;
    use datazen_driver_api::resource::{
        DescribeResourceRequest, IdentityScope, ResourceError, ResourceHandle, ResourcePurpose,
    };
    use datazen_driver_api::resource_adapter::ADAPTER_EVIDENCE_REVISION;
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
        // DuckDB now declares the cells it can back with code. The `assert_eq!`
        // above already pins the factory to the provider; this records that the
        // declaration is non-empty, so a silent regression back to
        // `CapabilitySet::default()` fails here instead of in production.
        assert!(
            factory().resource_capabilities().declares_anything(),
            "DuckDB declares what it can prove: a genuinely incremental read/write path. \
             An empty set would be under-claiming, not caution."
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
        assert_eq!(declared.data, DataSupport::StreamingReadWrite);
        assert!(
            declared.data.enables_row_read(),
            "query builds a buffered result set"
        );
        assert!(
            declared.data.enables_row_write(),
            "execute reports affected rows"
        );
        assert!(
            declared.data.enables_streaming_results(),
            "query_stream pulls rows one at a time out of a live cursor \
             (duckdb.rs:479-538) and the test at duckdb.rs:804 shows several batches \
             from one statement, so this is genuinely incremental"
        );
        assert!(
            declared.data.enables_feature(),
            "StreamingReadWrite is the only variant that satisfies enables_feature"
        );
        assert_eq!(declared.namespace_switch, NamespaceSwitch::Unsupported);
        assert!(
            !declared.namespace_switch.switches_in_place(),
            "an ATTACHed catalog is the only way to reach another database \
             (duckdb.rs:88-101) and change_context is refused by the adapter"
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
        // The deliberate asymmetry with ClickHouse: DuckDB *has* a session
        // (duckdb.rs:161-174) so `Unsupported` would be false, but nothing confirms the
        // host may pin onto it, so `Unknown` stands. ClickHouse, which provably opens
        // none, gets `Unsupported`. Different evidence, different answer.
        assert_eq!(
            declared.stateful_session,
            Availability::Unknown,
            "connect opens a real in-process Connection (duckdb.rs:165), so Unsupported would \
             be false; and no host-resumable session exists, so Supported is unbacked either"
        );
        assert!(!registry.require_stateful_session().is_ok());
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
        // Not an empty confirmation set: every one of the twelve cells carries an
        // evidence record, so the snapshot reports a non-zero capability revision
        // and no gaps. This is a stronger claim than "nothing was invented".
        assert!(
            registry.snapshot.capability_revision == ADAPTER_EVIDENCE_REVISION,
            "a populated evidence table must be stamped with the adapter evidence revision; \
             got {}",
            registry.snapshot.capability_revision
        );
        assert!(
            registry.snapshot.capability_revision != 0,
            "the revision must not be left at its zero default"
        );
        assert_eq!(
            registry.evidence_gaps(),
            Vec::<&'static str>::new(),
            "every confirmed cell must carry an evidence record; a gap means a value was \
             claimed without a reason"
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
            .expect("duckdb declares a real provider");
        let second = factory()
            .resource_provider()
            .expect("duckdb declares a real provider");
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
