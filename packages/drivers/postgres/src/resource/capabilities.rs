//! What the PostgreSQL provider declares, and the code each claim rests on.
//!
//! Every entry below is a claim about code that **already exists** in this
//! crate. A capability that is not proven is declared unsupported/unknown, and
//! the matching contract call rejects the request instead of returning a
//! plausible no-op — plan line 99: 「新增能力缺失不会 no-op 成功」.
//!
//! The evidence is also recorded in `CapabilitySnapshot::confirmed`, so a
//! reader can check the claim without re-deriving it.

use std::collections::BTreeMap;

use datazen_driver_api::capabilities::{
    Availability, CapabilityRegistry, CapabilitySet, CapabilitySnapshot, ContextObservation,
    DdlAtomicitySupport, NamespaceSwitch, PreciseCancelSupport, ResetForReuse,
    SessionScopedHandleSupport, SnapshotSupport, TransactionObservation, TransactionSupport,
};
// `capabilities` only re-imports the domain enums privately; they live here.
use datazen_driver_api::capability_domains::DataSupport;
use datazen_driver_api::namespace::{
    CanonicalIdRules, CaseFolding, CaseRules, NamespaceLevel, NamespaceLevelKind,
    NamespacePathSegment, NamespaceShape,
};
use datazen_driver_api::resource::{ConnectionCostPolicy, ResourcePurpose};
use datazen_driver_api::ConnectionConfig;
use datazen_driver_api::PROTOCOL_VERSION;

/// The provider id every [`ResourceHandle`] this crate issues is bound to.
///
/// Matches `PostgresFactory::driver_id`, so a handle minted by the resource
/// provider is accepted by the same identity the Command API uses.
///
/// [`ResourceHandle`]: datazen_driver_api::resource::ResourceHandle
pub(crate) const POSTGRES_PROVIDER_ID: &str = "postgresql";

/// The crate version the capability snapshot was taken at.
pub(crate) const POSTGRES_DRIVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// `connection.rs:424-436` opens a **second** pool with `max_connections = 1`
/// for the sole purpose of issuing `pg_cancel_backend`. A cancel therefore
/// costs one physical connection beyond the data pool, always.
pub(crate) const CONTROL_POOL_CONNECTIONS: u32 = 1;

/// Metadata commands are short-lived and execute one command at a time. Their
/// data pool is intentionally bounded to one connection; cancellation still
/// uses the separate control pool above.
pub(crate) const METADATA_POOL_CONNECTIONS: u32 = 1;

pub(crate) fn postgres_metadata_connection_cost() -> ConnectionCostPolicy {
    ConnectionCostPolicy::PoolBounded {
        max_physical_connections: METADATA_POOL_CONNECTIONS + CONTROL_POOL_CONNECTIONS,
    }
}

pub(crate) fn postgres_resource_connection_cost(
    config: &ConnectionConfig,
    purpose: ResourcePurpose,
) -> ConnectionCostPolicy {
    if purpose == ResourcePurpose::MetadataInspection {
        postgres_metadata_connection_cost()
    } else {
        postgres_connection_cost(config)
    }
}

/// The namespace shape PostgreSQL really has.
///
/// `Database` and `Schema` exist; a separate catalog level does not (PostgreSQL's
/// catalog *is* the database), so a target that names one is rejected by
/// `NamespaceShape::canonicalize` with `NonexistentNamespaceLevel`.
///
/// Neither level is `required`: `connection.rs::resolve_connect_database`
/// falls back to the server's default database and the driver's own schema
/// default (`search_path`, `connection.rs:43-46`) is applied per session, so
/// the provider can serve a target that names only one of them.
pub(crate) fn postgres_namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![
            NamespaceLevel::present(NamespaceLevelKind::Database, false),
            NamespaceLevel::absent(NamespaceLevelKind::Catalog),
            NamespaceLevel::present(NamespaceLevelKind::Schema, false),
        ],
        path_segments: vec![
            NamespacePathSegment {
                position: 0,
                kind: NamespaceLevelKind::Database,
                meaning: "database the session is attached to".to_string(),
            },
            NamespacePathSegment {
                position: 1,
                kind: NamespaceLevelKind::Schema,
                meaning: "first existing entry of the session's search_path".to_string(),
            },
        ],
        // Unquoted identifiers fold to lower case in PostgreSQL; a quoted one is
        // taken verbatim (`connection.rs:43-46` quotes the configured schema
        // for exactly this reason).
        case_rules: CaseRules {
            unquoted: CaseFolding::Lowercases,
            quoted: CaseFolding::Preserved,
        },
        canonical_id_rules: CanonicalIdRules {
            case_sensitive: false,
            delimiter: ".".to_string(),
        },
        aliases: BTreeMap::new(),
    }
}

/// The physical cost of one PostgreSQL resource.
///
/// `connect_impl` opens `effective_max_pool_size()` data connections plus one
/// control connection, so the honest number is the configured pool size plus
/// one — not the configured pool size. Reporting the smaller number would let a
/// caller size a budget that the provider then exceeds.
pub(crate) fn postgres_connection_cost(config: &ConnectionConfig) -> ConnectionCostPolicy {
    ConnectionCostPolicy::PoolBounded {
        max_physical_connections: config
            .effective_max_pool_size()
            .saturating_add(CONTROL_POOL_CONNECTIONS),
    }
}

/// The capability set this provider can actually back up.
///
/// | Capability | Declared | Why |
/// |---|---|---|
/// | `stateful_session` | `Supported` | interactive/agent acquisition checks out and retains one physical backend until explicit close. |
/// | `namespace_switch` | `RequiresReplacement` | cross-database work selects another pool (`execution.rs:30-42`); nothing switches the attached database in place. |
/// | `context_observation` | `Partial` | a pinned transaction is read on its own `PoolConnection`; a leased read names one backend, and `search_path` can be changed by a statement on that same backend. |
/// | `transaction_observation` | `Full` | `begin_transaction_impl` pins a real `PoolConnection` (`execution.rs:915-947`) and the commit/rollback path reports the server's answer. |
/// | `session_scoped_handles` | `Supported` | the transaction handle is registered in `transactions` and dropped on commit/rollback/close. |
/// | `reset_for_reuse` | `Unsupported` | there is no verified baseline replay; `reset_resource` returns `Discard`. |
/// | `precise_cancel` | `Supported` | `pg_cancel_backend(<pid of that execution only>)` on a separate control pool (`execution.rs:369-437`). |
/// | `snapshots` | `Unsupported` | `begin_read_snapshot_impl` exists but a per-table/per-database/coordinated guarantee is unverified. |
/// | `transactions.isolation_levels` | empty | `begin_transaction_impl` sends a bare `BEGIN`, so it cannot honour a named level. |
/// | `data` | `StreamingReadWrite` | resource_stream.rs publishes bounded batches through an awaitable sink; execute preserves rowsAffected. |
/// | `backup` | empty | the provider contract carries no backup surface at all; the driver's real dump/restore live on the legacy `ConnectionHandle` path. Citations are on the field. |
pub(crate) fn postgres_capability_set() -> CapabilitySet {
    // Declared cell by cell rather than through one exhaustive struct
    // expression. Every one of the twelve fields is assigned below, so the
    // result is identical to the old literal by construction — the defaults
    // are fully overwritten, and no cell can silently drift to `Unknown`.
    //
    // This style is the only one that survives `#[non_exhaustive]` on
    // `CapabilitySet`. Note that it is the *only* one: functional update
    // (`CapabilitySet { x, ..Default::default() }`) is rejected by that
    // attribute just as an exhaustive literal is, so a future freeze of the
    // contract requires this shape, not a one-line `..Default::default()`.
    let mut capabilities = CapabilitySet::default();

    capabilities.stateful_session = Availability::Supported;
    capabilities.namespace_switch = NamespaceSwitch::RequiresReplacement;
    capabilities.context_observation = ContextObservation::Partial;
    capabilities.transaction_observation = TransactionObservation::Full;
    capabilities.session_scoped_handles = SessionScopedHandleSupport::Supported;
    capabilities.reset_for_reuse = ResetForReuse::Unsupported;
    capabilities.precise_cancel = PreciseCancelSupport::Supported;
    capabilities.snapshots = SnapshotSupport::Unsupported;

    // Empty on purpose: the contract reads this as "cannot confirm any
    // level", which is exactly what a bare `BEGIN` means. Listing a
    // level here would make `begin_transaction` accept an option the
    // driver silently ignores.
    capabilities.transactions = TransactionSupport {
        isolation_levels: Vec::new(),
        savepoints: Availability::Unsupported,
        // `transactions` is a map keyed by connection id holding a pinned
        // `PoolConnection`, so a session can hold exactly one.
        max_open_transactions: Some(1),
    };

    // Left empty on purpose. `DatabaseDriver::ddl_atomicity` answers for
    // the driver as a whole, not per operation, and
    // `DdlAtomicitySupport::atomicity_for` returns `Unknown` for an
    // absent key — the caller then asks, instead of assuming.
    capabilities.ddl_atomicity = DdlAtomicitySupport::default();

    // resource_stream.rs consumes SQLx rows on the physical checkout and
    // awaits each sink batch (first row immediately, then <=256 rows / 1 MiB).
    // Statement completions retain metadata only; rejection cancels and drains
    // the same connection before it may be observed or executed again.
    capabilities.data = DataSupport::StreamingReadWrite;

    // Left empty on purpose, and for a different reason than `data` above:
    // PostgreSQL can genuinely be dumped and restored, but not through
    // this provider.
    //
    // `ResourceProvider` has 14 methods (`provider_id` / `capabilities` /
    // `namespace_shape` plus 11 `async` ones) and not one of them is a
    // backup, restore or dump surface. Nor can one be smuggled through
    // `execute_on_resource`: `run_command` accepts whatever
    // `execute_command` returned, but `decode_command_result`
    // (`resource/payload.rs:41-83`) recognizes exactly two payload shapes
    // — a `MultiQueryResult` from `query`, and `{"rowsAffected": n}` from
    // `execute`. Everything else, including any dump artifact, becomes
    // `ResourceError::OperationNotSupported` at `payload.rs:73`, whose
    // reason names only the payload *shape* and never its contents.
    //
    // The capability really does exist on the driver: `postgres.rs:556`
    // `dump_database_with_progress` → `catalog.rs:352`
    // `dump_database_with_progress_impl` produces the artifact, and
    // restore rides the shared `sql_dump::restore_sql_statements_with_progress`
    // default (this crate overrides neither `restore_sql` nor
    // `recover_restore_statement`). Both run against a `ConnectionHandle`.
    //
    // So neither filling variant would be honest. `ArtifactAndRestore`
    // would advertise a surface the provider does not have, and a host
    // that trusted it would clear `require_backup_artifact()` and then hit
    // the dead end above. `Unsupported` would be the opposite lie: it
    // would tell the host PostgreSQL cannot be backed up, hiding a
    // feature that demonstrably works. "This contract cannot express the
    // backup capability" and "this database cannot be backed up" are two
    // different statements, and conflating them would bury a real feature
    // for whoever wires the backup window up later. So the caller asks,
    // and the question is answered honestly instead of wrongly.
    capabilities.backup = Default::default();

    capabilities
}

/// Bumped whenever [`postgres_capability_set`] changes meaning, so a cached
/// snapshot can be told apart from the current declaration. `1` is the first
/// set that was actually checked against the code below; a snapshot taken
/// before this module existed carried `0`. `2` added buffered row read/write;
/// `3` adds fixed interactive/agent sessions and bounded streaming results.
pub(crate) const POSTGRES_CAPABILITY_REVISION: u64 = 3;

/// The full registry: identity, version, protocol and the declared set.
pub(crate) fn postgres_capability_registry() -> CapabilityRegistry {
    let mut snapshot = CapabilitySnapshot::new(
        POSTGRES_PROVIDER_ID,
        POSTGRES_DRIVER_VERSION,
        PROTOCOL_VERSION,
        POSTGRES_CAPABILITY_REVISION,
    );
    for (capability, evidence) in postgres_capability_evidence() {
        snapshot.confirmed.insert(capability.to_string(), evidence);
    }
    let mut registry = CapabilityRegistry::new(POSTGRES_PROVIDER_ID, snapshot);
    registry.capabilities = postgres_capability_set();
    registry
}

/// The evidence recorded next to the declaration, so every claim above can be
/// checked against the file it names.
fn postgres_capability_evidence() -> Vec<(&'static str, String)> {
    vec![
        (
            "preciseCancel",
            "src/execution.rs cancel_query_with_execution_impl: SELECT pg_cancel_backend($1) \
             with that execution's backend pid, issued on the dedicated control pool \
             (src/connection.rs opens it with max_connections = 1)"
                .to_string(),
        ),
        (
            "transactionObservation",
            "src/execution.rs begin_transaction_impl pins the PoolConnection in the \
             transactions map; commit_impl/rollback_impl report the server's answer"
                .to_string(),
        ),
        (
            "sessionScopedHandles",
            "the transaction handle is registered in transactions keyed by connection id \
             and removed on commit, rollback and disconnect"
                .to_string(),
        ),
        (
            "contextObservation",
            "src/resource/observation.rs reads current_database/current_user/search_path on \
             the pinned connection (Confirmed) or on one pooled backend (Partial)"
                .to_string(),
        ),
        (
            "namespaceSwitch",
            "src/execution.rs resolve_statement_pool selects a different pool for \
             cross-database work instead of switching the attached database"
                .to_string(),
        ),
        (
            "statefulSession",
            "src/fixed_session.rs open_fixed_session retains a physical checkout until disconnect; transaction finish never returns a fixed checkout"
                .to_string(),
        ),
        (
            "snapshots",
            "declined: begin_read_snapshot_impl exists but a per-table/per-database/\
             coordinated guarantee is unverified"
                .to_string(),
        ),
        (
            "connectionCostPolicy",
            concat!(
                "InteractiveQuery and AgentSession retain one data connection plus one control connection; ",
                "MetadataInspection uses one data connection plus one control connection; ",
                "other pooled resources use effective_max_pool_size plus one control connection",
            )
            .to_string(),
        ),
        (
            "data",
            "rowRead and streamingResults: resource_stream.rs emits bounded batches directly from the physical connection into ResultSink; rowWrite: execute_at retains bound parameters and reports affected rows"
                .to_string(),
        ),
        (
            "backup",
            "declined: ResourceProvider's 14 methods expose no backup/restore/dump, and \
             payload.rs:73 turns anything but MultiQueryResult or {rowsAffected} into \
             OperationNotSupported — while the real dump (catalog.rs:352 \
             dump_database_with_progress_impl) and the shared restore default both run \
             on a legacy ConnectionHandle, so this contract can neither prove nor deny \
             the capability"
                .to_string(),
        ),
    ]
}
