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

/// The real provider for `rqlite`, built once and shared.
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
        "rqlite",
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

/// What this crate can honestly claim about its own resources, cell by cell.
///
/// `CapabilitySet` is fail-closed: every enum's default is its *non*-supporting value,
/// so a driver that declares nothing gates every dependent feature off. rqlite has
/// exactly **one** cell it can prove from its own code — `data` — and eleven it
/// deliberately leaves at the non-supporting answer. Every row below gives the
/// declared value and the evidence behind it; a blank is the claim that the evidence
/// is absent, not an oversight. Line numbers are `src/rqlite.rs` unless stated.
///
/// | cell | declared | evidence |
/// | --- | --- | --- |
/// | `data` | `DataSupport::BufferedReadWrite` | **The one filled cell.** Read: `query` (:277) returns a materialized `QueryResult` built by `result_from_json` (:91-136). Write: `execute` (:348-372) posts to `/db/execute?level=strong` and returns `rows_affected`. *Not* streaming: `query_json` (:30-51) reads the whole body with `resp.text()` and parses it into a `serde_json::Value` before returning; `query_stream` (:311-337) awaits that same full body at :322 and only then replays the already-materialized `Vec` through `stream_decoded_rows` (:325). No row is emitted before the whole result set is in memory, so `streamingResults` does not hold and `StreamingReadWrite` would be a false claim. |
/// | `backup` | `BackupSupport::Unknown` (blank) | This crate contains no backup or restore path — `backup` appears nowhere in `src/` but a test name. `ui/meta.ts:15` nevertheless sets `supportsBackup: true`, which only gates the frontend Backup window and is backed by no Rust here. `ArtifactOnly`/`ArtifactAndRestore` would repeat that unbacked claim in the contract; `Unsupported` would assert we probed a server capability this crate never touched. The contradiction is recorded here rather than papered over. |
/// | `stateful_session` | `Availability::Unsupported` | The handle carries no session identity: the driver's entire state is `clients: RwLock<HashMap<String, (reqwest::Client, String)>>` (:11-12) — an HTTP client and a base URL under a `rqlite_<uuid>` pool id minted at `connect` (:179). Every statement is an independently constructed `POST {base}/db/query?level=strong` (:36-40); no session-establishing call is ever issued and no server-assigned session id is ever stored, so the driver can neither address nor reuse a server-side session across statements. That is the same architecture as mysql, which the corpus already rules `Unsupported` on these grounds (`packages/drivers/mysql/src/resource_capabilities.rs:74` — a client is not a session), so the architecture is *measured*, not unmeasured: `Unknown` would discard evidence the repository already holds. `SessionContinuity::Leased` describes the handle lease, not a server session. Pinned by `tests::a_described_resource_is_never_mistaken_for_a_fixed_reusable_session`. |
/// | `namespace_switch` | `NamespaceSwitch::PerRequest` | The module docs above justify refusing `change_context` with "rqlite's 'switch' is a reconnect", but the code does not back that sentence: the database name is never stored on the resource. `database` is a parameter of the trait methods themselves (`get_tables` :199-209, `get_table_schema` :231-242), `effective_database` (:58-65) resolves blank to `main` and `quote_schema` (:69-71) renders it as a per-statement qualifier used by `list_tables_sql`/`table_info_sql` (:74-89), while the resource is the base URL fixed at `connect` (:178-183), whose only mutation is `disconnect`'s `remove` (:191). So the driver can neither switch a live session in place nor show that a switch needs a replacement resource — and because the supplied name is interpolated into the statement that is actually sent, the driver demonstrably *does* read whichever database it is handed, on every call, with no switch in between. That is `PerRequest`, not a blank: `switches_in_place` is still false for it (that is true only for `InPlace`), but unlike `Unsupported` it records a capability instead of a refusal. |
/// | `context_observation` | `ContextObservation::Unsupported` | Both the default and the measured answer: the crate contains no context read-back call, and the module docs record that `observe_session` reports every field unknown rather than back-filling the acquisition target. |
/// | `transaction_observation` | `TransactionObservation::Unsupported` | Default and truthful. This driver issues no `BEGIN`/`COMMIT`/`ROLLBACK` — none appears anywhere in the crate — and mints no transaction handle; the resource port refuses commit and rollback because rqlite resolves transactions inside its own commands. |
/// | `session_scoped_handles` | `SessionScopedHandleSupport::Unknown` (blank) | The only map the driver owns is `clients` (:12), whose values are `(reqwest::Client, String)`; there is no cursor, prepared-statement or transaction registry a session-scoped handle could key into. That is suggestive but not a measurement — nothing ever asked for such a handle — so unmeasured beats measured-absent. |
/// | `reset_for_reuse` | `ResetForReuse::Unsupported` | Default and truthful. There is no reset path: `disconnect` (:190-193) removes the map entry and nothing else in the crate mutates `clients`, so the adapter has no verified way to return a used resource to a known baseline. |
/// | `precise_cancel` | `PreciseCancelSupport::Unknown` | **Not the driver's cell to declare.** `LegacyResourceAdapter::new` overwrites `precise_cancel` from `DatabaseDriver::supports_query_execution_cancel()` (`packages/driver-api/src/resource_adapter.rs`), so any value written here is replaced before a caller can read it. rqlite inherits the trait default `false` and `cancel_query` answers `DriverError::Unsupported` (:393-399), so the adapter installs `Unknown` regardless. `Unknown` is written here because it is the truthful statement about a driver with no execution-handle protocol *and* the value the adapter will install, so the two views agree. |
/// | `snapshots` | `SnapshotSupport::Unsupported` | Default and truthful. `query_json` asks for `level=strong` (:36), which buys a per-statement linearizable read — but the driver opens nothing server-side to hold it open: one statement in, one fully parsed body out, `Value` dropped at the end of the call. A `SnapshotSupport` variant must describe a snapshot a caller can pin across statements, and no such object exists here; `PerDatabase` would smuggle in a Raft-quorum claim the contract never asked about. |
/// | `transactions` | `isolation_levels: []`, `savepoints: Unknown`, `max_open_transactions: None` (all blank) | An empty `isolation_levels` is a positive statement — "no level is confirmed" — the same way `DdlAtomicitySupport::atomicity_for` returns `Unknown` for an absent key. Nothing in the crate issues `BEGIN` or `SAVEPOINT`, so no level can be named, savepoint support has never been probed, and no bound on concurrently open transactions exists to state. |
/// | `ddl_atomicity` | `by_operation` left empty (blank) | `RqliteDriver` does not override `DatabaseDriver::ddl_atomicity`, so the trait default `DdlAtomicity::Unknown` is the answer for every operation and an empty `by_operation` reports exactly that. `sync_family() == "sqlite"` (:145-147) is lineage, not evidence: it says nothing about how rqlite's Raft-replicated DDL behaves, so the map stays empty instead of borrowing SQLite's answer. |
///
/// The value is also required to be byte-identical to what the adapter's own
/// `CapabilityRegistry` reports, because `DatabaseDriverFactory::resource_capabilities`
/// and `ResourceProvider::capabilities` are two views of one claim; a driver that made
/// the first view rosier than the second would be the 「假的全支持」 this contract exists to
/// kill. `tests::factory_capabilities_match_the_provider` is the gate that keeps the
/// two from drifting.
pub(crate) fn capabilities() -> CapabilitySet {
    CapabilitySet {
        // A handle is an HTTP client plus a base URL (:11-12) — it holds no
        // session identity, every statement independently builds its own
        // request (:36-40), and no server-assigned session id is ever stored.
        // The corpus already rules this exact architecture `Unsupported` for
        // mysql, so writing `Unknown` here would discard evidence the
        // repository already holds.
        stateful_session: Availability::Unsupported,

        // The one provable data cell. Reads return rows and writes report
        // `rows_affected`, but `query_json` parses the whole response body
        // before `result_from_json` builds a `Vec`, and `query_stream` awaits
        // that same body before replaying it — so the result set is fully
        // materialized before any of it is returned and `streamingResults`
        // does not hold. `StreamingReadWrite` would be a false claim here.
        data: DataSupport::BufferedReadWrite,

        // Blanks, each keeping the default that is also the honest answer.
        // They are spelled out rather than hidden behind `..Default::default()`
        // so that a later "fix" has to edit a value someone already argued for.
        namespace_switch: NamespaceSwitch::PerRequest, // no session to switch; `database` is a trait-method parameter interpolated into each catalog statement (:58-89)
        context_observation: ContextObservation::Unsupported, // no read-back call exists in this crate
        transaction_observation: TransactionObservation::Unsupported, // no BEGIN/COMMIT/ROLLBACK anywhere in the crate
        session_scoped_handles: SessionScopedHandleSupport::Unknown, // no cursor/statement registry, but never probed either
        reset_for_reuse: ResetForReuse::Unsupported, // disconnect only removes a map entry (:190-193)
        // Overwritten downstream by the adapter; see the table for why `Unknown`
        // is still the right thing to write here.
        precise_cancel: PreciseCancelSupport::Unknown,
        snapshots: SnapshotSupport::Unsupported, // `level=strong` (:36) is per-statement, not a pinnable snapshot
        transactions: TransactionSupport::default(), // empty isolation levels, unknown savepoints, no bound
        ddl_atomicity: DdlAtomicitySupport::default(), // no per-operation evidence; the trait default answers `Unknown`
        backup: BackupSupport::Unknown, // no backup code here at all, despite `ui/meta.ts:15`
    }
}

/// This crate's evidence table, in the machine-readable form the registry carries.
///
/// The doc table above is a claim about the same thing; this is the claim the host
/// can read back, one record per capability cell, and each record names the line of
/// code that justifies it. A cell that is not filled is recorded too, with a
/// `declined:` prefix and the measurement that produced the blank — a refusal that
/// was measured and a claim nobody could support are different diagnoses, and only
/// the first one may be written down as a fact.
///
/// Entries marked `declined:` record a positive decision not to claim, which is
/// the opposite of a silently empty map.
pub(crate) fn capability_evidence() -> Vec<(&'static str, String)> {
    vec![
        (
            "statefulSession",
            "Unsupported: `connect` opens no physical session. The only state it files is a \
             pooled `reqwest::Client` plus a base URL under a fresh `rqlite_<uuid>` key \
             (rqlite.rs:12, inserted at rqlite.rs:180-183), and `disconnect` only removes that \
             map entry (rqlite.rs:190-193). Every statement is an independent POST to \
             `/db/query?level=strong` carrying nothing but its own SQL \
             (rqlite.rs:35-38), and no server-assigned session id is ever stored. The corpus \
             already rules this exact architecture Unsupported for mysql \
             (mysql/src/resource_capabilities.rs:74), so `Unknown` here would discard evidence \
             the repository already holds. Measured absent."
                .to_string(),
        ),
        (
            "namespaceSwitch",
            "PerRequest: there is no session to switch, and the attached database is \
             addressed on every call instead of being held as resource state. \
             `database` is a parameter of the trait methods themselves — \
             `get_tables(&self, handle, database: &str, schema)` (rqlite.rs:199-209) and \
             `get_table_schema(..., database: &str, ...)` (rqlite.rs:231-242) — and it \
             is interpolated into the outgoing catalog SQL as a quoted schema qualifier: \
             `SELECT name FROM \"<db>\".sqlite_master ...` and `PRAGMA \"<db>\".table_info(...)` \
             (rqlite.rs:74-89, via `effective_database` rqlite.rs:58-65 and `quote_schema` \
             rqlite.rs:69-71). So supplying a different `database` demonstrably changes \
             which namespace is read; the resource itself is only the base URL fixed at \
             `connect`. `effective_database` is a pure function of its argument — no \
             mutable pool or handle state is consulted or written. That is precisely \
             `PerRequest`, and it is not `Unsupported`: the hard-coded \
             `vec![\"main\"]` from `get_databases` (rqlite.rs:195-197) describes the \
             default node's ATTACH set, not a limit on what the catalog SQL can address."
                .to_string(),
        ),
        (
            "contextObservation",
            "Unsupported: `observe_session` answers `SessionObservation::unobservable()` on \
             every call (resource_adapter.rs:397-402) and `execute_on_resource` pins \
             `context_before`/`context_after` to `unobserved()` (resource_adapter.rs:377-378). \
             This crate issues no read-back call that could replace either — `query_json` only \
             parses the body of the statement it just sent (rqlite.rs:30-51). Measured absent."
                .to_string(),
        ),
        (
            "transactionObservation",
            "Unsupported: `commit_transaction` (resource_adapter.rs:429-438) and \
             `rollback_transaction` (resource_adapter.rs:440-449) both refuse by name because \
             the outcome cannot be read back, and `execute_on_resource` hardcodes \
             `TransactionState::Unknown` with `transaction_id: None` and `effect: None` \
             (resource_adapter.rs:380-382). The crate side agrees: \
             `DatabaseDriver::begin_transaction` is the trait default and errors \
             (traits.rs:602-609), and `execute` posts a bare `{\"q\": sql}` body with no \
             `BEGIN`/`COMMIT` anywhere around it (rqlite.rs:348-372). Measured absent."
                .to_string(),
        ),
        (
            "sessionScopedHandles",
            "declined: `Unknown`, not `Unsupported`, and the distinction is deliberate. \
             `execute_on_resource` returns an empty `session_handles` because this path \
             registers none (resource_adapter.rs:387), and this crate keeps no cursor or \
             statement registry — `query_multi` and `query_with_params` are one-line \
             delegations that re-issue the whole statement (rqlite.rs:291-309, rqlite.rs:339-346) \
             — so there is no handle to offer. But nothing here ever probed a handle-holding \
             session either, so the record says 'not investigated' rather than claiming a \
             measured refusal."
                .to_string(),
        ),
        (
            "resetForReuse",
            "Unsupported: `reset_resource` always answers `ResetDisposition::Discard` \
             (resource_adapter.rs:499-505), and the crate has no baseline to replay — \
             `disconnect` only removes a map entry (rqlite.rs:190-193). There is no verified \
             baseline return, so `Verified` would hand back a resource whose state nobody \
             checked."
                .to_string(),
        ),
        (
            "preciseCancel",
            "Unknown, and the adapter owns this cell. `LegacyResourceAdapter::new` overwrites \
             it from `supports_query_execution_cancel()` (resource_adapter.rs:148-152), which \
             for rqlite is the trait default `false` (traits.rs:817-819); `cancel_query` agrees \
             by refusing (rqlite.rs:393-398) instead of reporting a cancellation that never \
             happened. The value recorded here is the value the adapter installs, so the two \
             views agree rather than merely not contradicting."
                .to_string(),
        ),
        (
            "snapshots",
            "Unsupported: neither this crate nor the adapter implements `begin_read_snapshot`, \
             so the trait default answers `DriverError::Unsupported` (traits.rs:734-741). \
             `level=strong` on the query endpoint (rqlite.rs:36) is a per-statement consistency \
             level, not a pinnable snapshot scope, so it cannot stand in for one."
                .to_string(),
        ),
        (
            "transactions",
            "declined: the declaration is deliberately blank and each blank is a measurement, not \
             an omission. `DatabaseDriver::begin_transaction` is the trait default and errors \
             (traits.rs:602-609), so the adapter's `begin_transaction` \
             (resource_adapter.rs:416-424) can never open one. `isolation_levels: []` therefore \
             reads as 'no level can be honoured' — naming a level would advertise an option the \
             driver silently ignores, since `execute` posts a bare `{\"q\": sql}` body \
             (rqlite.rs:348-372) and no `SET TRANSACTION` is ever issued. `savepoints` is left at \
             its `Unknown` default rather than forced to `Unsupported` \
             (capabilities.rs:216-224) because the savepoint question is only reachable through \
             a transaction this driver cannot open — the blank is 'never probed', which is the \
             weaker and truthful claim. `max_open_transactions: None` is 'unmeasured', not \
             'unbounded' — there is no transaction registry here to count."
                .to_string(),
        ),
        (
            "ddlAtomicity",
            "declined: the map is empty on purpose. `DatabaseDriver::ddl_atomicity` is not \
             overridden in this crate, so it answers `Unknown` (traits.rs:156-158), and \
             `DdlAtomicitySupport::atomicity_for` fails closed to `Unknown` for any absent key \
             (capabilities.rs:240-245). rqlite would in fact execute a DDL statement through \
             `execute` (rqlite.rs:348-372), but a single-request statement is not the \
             multi-statement migration atomicity this field asks about, and filing a value per \
             operation would assert something nobody has measured."
                .to_string(),
        ),
        (
            "data",
            "BufferedReadWrite, not StreamingReadWrite: rows are read (`query_json` then \
             `result_from_json`, rqlite.rs:30-51 and rqlite.rs:91-136) and written (`execute` \
             reads `rows_affected` off the reply, rqlite.rs:348-372), so both directions are \
             real — only `streamingResults` is denied. `query_json` awaits `resp.text()` and \
             parses it with `from_str` (rqlite.rs:42-50), and `query_stream` awaits that same \
             body before replaying it through one `stream_decoded_rows` call \
             (rqlite.rs:311-337); nothing is pulled incrementally over the wire. `rows_affected` \
             is `None` for every result rqlite returns (rqlite.rs:133), so the write claim is \
             about reaching the server, not about a row count."
                .to_string(),
        ),
        (
            "backup",
            "declined: `Unknown`, not `Unsupported`, because this crate has no backup command at \
             all to measure the absence of. `command_definitions` is a closed list of query / \
             execute / query_stream plus the schema-catalog and schema-object commands \
             (rqlite.rs:400-409), with no artifact producer or consumer in it, and \
             `get_databases` returns a hard-coded `vec![\"main\"]` (rqlite.rs:195-197) rather \
             than exporting one. `ui/meta.ts:15` setting `supportsBackup: true` is host metadata, \
             not code in this crate, so it cannot support a capability claim here. The blank says \
             'not investigated', which is the weaker and truthful claim."
                .to_string(),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use datazen_driver_api::capabilities::{
        CapabilitySet, NamespaceSwitch, PreciseCancelSupport, SessionContinuity,
    };
    use datazen_driver_api::namespace::NamespaceTarget;
    use datazen_driver_api::require_resource_provider;
    use datazen_driver_api::resource::{
        DescribeResourceRequest, IdentityScope, ResourceError, ResourceHandle, ResourcePurpose,
    };
    use datazen_driver_api::resource_adapter::ADAPTER_EVIDENCE_REVISION;
    use datazen_driver_api::{BackupSupport, ConnectionConfig, DataSupport, DatabaseDriverFactory};

    use crate::resource_provider::capabilities;
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
        // This assertion used to read `!declares_anything()`: the placeholder state
        // where the whole set was `CapabilitySet::default()`. `declares_anything()`
        // is literally `*self != Self::default()`, so keeping the old direction
        // would forbid the one cell rqlite can prove. It is inverted, not
        // removed — the same gate, now pointed at the real state, so the set can
        // never silently collapse back to all-defaults without a red test.
        assert!(
            factory().resource_capabilities().declares_anything(),
            "rqlite declares the one cell it can prove (buffered read/write); \
             an all-default set would mean the declaration was dropped"
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
            datazen_driver_api::capabilities::Availability::Unsupported,
            "a handle is an HTTP client plus a base URL, never a server-side session"
        );
    }

    #[test]
    fn the_declaration_claims_only_what_the_crate_backs() {
        let declared = capabilities();

        assert_ne!(
            declared,
            CapabilitySet::default(),
            "the declaration must not collapse back to every cell at its default"
        );
        assert_eq!(
            declared.stateful_session,
            datazen_driver_api::capabilities::Availability::Unsupported,
            "the handle is an HTTP client plus a base URL, so no server-side session can be \
             addressed across statements — the same architecture the corpus rules Unsupported \
             for mysql"
        );
        assert_eq!(
            declared.data,
            DataSupport::BufferedReadWrite,
            "reads and writes are real, but query_json parses the whole body before any \
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
            declared.namespace_switch,
            NamespaceSwitch::PerRequest,
            "the supplied database is interpolated into the catalog SQL that is actually \
             sent (rqlite.rs:74-89) and the resource is only the base URL, so this driver \
             demonstrably reads whichever namespace it is handed, per call"
        );
        assert!(
            declared.namespace_switch.addresses_per_request(),
            "PerRequest must stay distinguishable from Unsupported, which would claim the \
             driver cannot address another namespace at all — that is false here"
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
        assert_eq!(
            registry.snapshot.capability_revision, ADAPTER_EVIDENCE_REVISION,
            "the adapter merged this crate's evidence table, so the revision must be the \
             one that merge stamps"
        );
        assert_ne!(
            registry.snapshot.capability_revision, 0,
            "a revision of 0 means no evidence was ever merged and the snapshot would be \
             indistinguishable from a driver that confirmed nothing"
        );
        assert_eq!(
            registry.evidence_gaps(),
            Vec::<&'static str>::new(),
            "every capability cell must carry a record: a blank with a `declined:` prefix \
             is a written-down measurement, and an absent record is not"
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
            .expect("rqlite declares a real provider");
        let second = factory()
            .resource_provider()
            .expect("rqlite declares a real provider");
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
