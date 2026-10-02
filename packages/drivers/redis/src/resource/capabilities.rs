//! What the Redis provider declares, and the code each claim rests on.
//!
//! Every entry below is a claim about code that **already exists** in this
//! crate. A capability that is not proven is declared unsupported/unknown, and
//! the matching contract call rejects the request instead of returning a
//! plausible no-op.
//!
//! The evidence is also recorded in `CapabilitySnapshot::confirmed`, so a
//! reader can check the claim without re-deriving it.
//!
//! ## Redis is the mirror image of PostgreSQL here
//!
//! PostgreSQL is declared `stateful_session: Unsupported` because a `PgPool`
//! hands out a different backend per statement. Redis is the opposite: one
//! [`RedisConn`] holds exactly **one** [`RedisLiveConn`], and every operation
//! in the crate reaches that same connection through a single keyed lookup
//! ([`RedisDriver::get_conn`]). A session is not merely stateful, it is a
//! single wire, which is exactly what makes an in-place `SELECT` provable
//! here and only a replacement in PostgreSQL.
//!
//! [`RedisConn`]: crate::driver::RedisConn
//! [`RedisLiveConn`]: crate::connect::RedisLiveConn
//! [`RedisDriver::get_conn`]: crate::driver::RedisDriver::get_conn

use std::collections::BTreeMap;

use datazen_driver_api::capabilities::{
    Availability, CapabilityRegistry, CapabilitySet, CapabilitySnapshot, ContextObservation,
    NamespaceSwitch,
};
// `capabilities` only re-imports the domain enums privately; they live here.
use datazen_driver_api::capability_domains::{BackupSupport, DataSupport};
use datazen_driver_api::namespace::{
    CanonicalIdRules, CaseFolding, CaseRules, NamespaceLevel, NamespaceLevelKind,
    NamespacePathSegment, NamespaceShape,
};
use datazen_driver_api::resource::ConnectionCostPolicy;
use datazen_driver_api::PROTOCOL_VERSION;

/// The provider id every [`ResourceHandle`] this crate issues is bound to.
///
/// Matches `RedisFactory::driver_id`, so a handle minted by the resource
/// provider is accepted by the same identity the Command API uses.
///
/// [`ResourceHandle`]: datazen_driver_api::resource::ResourceHandle
pub(crate) const REDIS_PROVIDER_ID: &str = "redis";

/// The crate version the capability snapshot was taken at.
pub(crate) const REDIS_DRIVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// The namespace shape Redis really has.
///
/// `Database` is the only level that exists: Redis has 16 logical databases
/// and nothing below them. There is no catalog and no schema, which is not an
/// opinion — `DatabaseDriver::has_schema_level` is left at the trait default
/// of `false` by this crate and `database.rs:378` asserts it. A target naming
/// either level is therefore rejected by name in `NamespaceShape::canonicalize`
/// rather than being silently folded into the database level.
///
/// The level is not `required`: a target that names no database at all is
/// legal, and the provider attaches to `SELECT 0`.
pub(crate) fn redis_namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![
            NamespaceLevel::present(NamespaceLevelKind::Database, false),
            NamespaceLevel::absent(NamespaceLevelKind::Catalog),
            NamespaceLevel::absent(NamespaceLevelKind::Schema),
        ],
        path_segments: vec![NamespacePathSegment {
            position: 0,
            kind: NamespaceLevelKind::Database,
            meaning: "logical database index the session is attached to via SELECT".to_string(),
        }],
        // A logical database is named by a bare index. Redis folds no case
        // anywhere in the namespace path: `db3` and `DB3` are not two
        // databases, and `RedisDriver::parse_db_name` (`driver/mod.rs:164-178`)
        // accepts either spelling of the same number while keys — which are
        // case-sensitive, not namespace — are a different matter entirely.
        case_rules: CaseRules {
            unquoted: CaseFolding::Preserved,
            quoted: CaseFolding::Preserved,
        },
        canonical_id_rules: CanonicalIdRules {
            case_sensitive: true,
            delimiter: String::new(),
        },
        aliases: BTreeMap::new(),
    }
}

/// The physical cost of one Redis resource.
///
/// `database.rs:67-80 connect` calls `open_live_conn` exactly once and stores
/// the result under a single `pool_id`; there is no second pool, no control
/// connection, and nothing that opens lazily later. One resource is one socket
/// — so this is a fixed cost, not a bound, and reporting a `PoolBounded` range
/// would imply a ceiling the driver never had.
pub(crate) fn redis_connection_cost() -> ConnectionCostPolicy {
    ConnectionCostPolicy::PoolBounded {
        max_physical_connections: 1,
    }
}

/// The capability set this provider can actually back up.
///
/// | Capability | Declared | Why |
/// |---|---|---|
/// | `stateful_session` | `Supported` | one `RedisConn` per handle holds one `RedisLiveConn` (`driver/mod.rs:21-24`); every operation reaches it through the single keyed lookup `RedisDriver::get_conn` (`driver/mod.rs:127-134`). The session's current database is therefore real, stable state, not a per-statement artifact. |
/// | `namespace_switch` | `InPlace` | `select_db_on` (`driver/session.rs:9-18`) issues `SELECT <index>` **on that same live connection**. No new socket, no replacement. |
/// | `context_observation` | `Partial` | `observe_session` does a real `INFO server` round trip, so liveness, health and protocol drain are observed facts. The current database index is *not* read back from the server, so the reported context carries an empty namespace and `Partial` confidence instead of a namespace the server never confirmed. |
/// | `transaction_observation` | `Unsupported` | Redis has no SQL transaction. A `MULTI` block is a queued-command buffer, not an all-or-nothing unit. |
/// | `session_scoped_handles` | `Unknown` | the contract's session-scoped handle is a transaction handle; this driver issues none. |
/// | `reset_for_reuse` | `Unsupported` | there is no verified baseline replay. `FLUSHDB` is the closest command and it is *destructive* — replaying a baseline with it would delete the user's data. `reset_resource` returns `Discard`. |
/// | `precise_cancel` | `Unknown` | Redis has no execution-addressed cancel. `CLIENT KILL` tears down a whole connection, not one running command, and the legacy `cancel_query` (`driver/database.rs`) refuses with `DriverError::Unsupported` rather than reporting a cancellation that never happened. |
/// | `snapshots` | `Unsupported` | no snapshot or point-in-time-read protocol exists on this path. |
/// | `transactions.isolation_levels` | empty | there is no transaction to give a level. |
/// | `data` | empty | Redis is a key-value store. `ResultChunk.rows` is a *tabular* shape and nothing in this crate produces one — see the field. |
/// | `backup` | empty | the provider contract carries no backup surface at all. See the field. |
pub(crate) fn redis_capability_set() -> CapabilitySet {
    // Built field by field from the default rather than through a struct
    // literal, so a capability added to `CapabilitySet` later fails to compile
    // here instead of silently inheriting a value nobody chose.
    let mut set = CapabilitySet::default();

    set.stateful_session = Availability::Supported;
    set.namespace_switch = NamespaceSwitch::InPlace;
    set.context_observation = ContextObservation::Partial;

    // `transaction_observation`, `session_scoped_handles`, `reset_for_reuse`,
    // `precise_cancel`, `snapshots`, `transactions` and `ddl_atomicity` stay
    // at their fail-closed defaults, and each of those defaults is the true
    // answer for this driver rather than a shrug:
    //
    //   * `precise_cancel` defaults to `Unknown`, and `Unknown` is treated as
    //     "not in the cancel set" — never as "the cancel worked".
    //   * `reset_for_reuse` defaults to `Unsupported`, which is exactly the
    //     disposition `reset_resource` returns.
    //   * `snapshots` defaults to `Unsupported`, so the snapshot gate is closed.
    //   * `transactions` defaults to an empty isolation-level list, an unknown
    //     savepoint answer, and no concurrency cap.
    //
    // Nothing is flipped without a file:line that shows the behaviour.

    // `data` — left at the default (`DataSupport::Unknown`).
    //
    // The row surface genuinely does not exist, and this is measured rather
    // than assumed: `query` (`database.rs:216-243`) turns a Redis reply into
    // one `Value` per command, `query_multi` (`database.rs:245-291`) wraps
    // those in a `StatementResult` whose `columns` is always empty because
    // Redis has no result-set metadata, and the Command API returns free-form
    // JSON per command. `decode_command_result` in `payload.rs` refuses all
    // of those rather than flattening them into empty rows.
    //
    // `Unsupported` is deliberately not used: that variant means "measured and
    // permanently absent" *for the driver*, and Redis plainly has a data
    // plane — it simply has no tabular one, and this axis is defined in terms
    // of a row surface. The axis stays undeclared; the refusal is still total,
    // because `enables_row_read` is false for every value here.
    set.data = DataSupport::default();

    // `backup` — left at the default (`BackupSupport::Unknown`).
    //
    // 契约无备份面: the 14 methods of `ResourceProvider` expose no backup,
    // restore or dump at all, so this contract can neither prove nor deny the
    // capability.
    //
    // The capability really does exist on the driver — `dump_database_with_progress`
    // (`database.rs:340`) and `restore_sql_with_progress` (`database.rs:352`)
    // both work — but they run against a legacy `ConnectionHandle`, and
    // `decode_command_result` (`payload.rs:73`) turns their payload into an
    // explicit `OperationNotSupported`, because a dump artifact's reason names
    // only the payload *shape* and never its contents.
    //
    // So neither filling variant would be honest. `ArtifactAndRestore` would
    // advertise a surface this contract does not have, and a host that trusted
    // it would clear `require_backup_artifact()` and then hit the dead end.
    // `Unsupported` would be the opposite lie: it would tell the host Redis
    // cannot be backed up, hiding a feature that demonstrably works. "This
    // contract cannot express the backup capability" and "this database cannot
    // be backed up" are two different statements, and conflating them would
    // bury a real feature for whoever wires the backup window up later. So the
    // caller asks, and the question is answered honestly instead of wrongly.
    set.backup = BackupSupport::default();

    set
}

/// Bumped whenever [`redis_capability_set`] changes meaning, so a cached
/// snapshot can be told apart from the current declaration. `1` is the first
/// set that was actually checked against the code cited above.
pub(crate) const REDIS_CAPABILITY_REVISION: u64 = 1;

/// The full registry: identity, version, protocol and the declared set.
pub(crate) fn redis_capability_registry() -> CapabilityRegistry {
    let mut snapshot = CapabilitySnapshot::new(
        REDIS_PROVIDER_ID,
        REDIS_DRIVER_VERSION,
        PROTOCOL_VERSION,
        REDIS_CAPABILITY_REVISION,
    );
    for (capability, evidence) in redis_capability_evidence() {
        snapshot.confirmed.insert(capability.to_string(), evidence);
    }
    let mut registry = CapabilityRegistry::new(REDIS_PROVIDER_ID, snapshot);
    registry.capabilities = redis_capability_set();
    registry
}

/// The evidence recorded next to the declaration, so every claim above can be
/// checked against the file it names.
fn redis_capability_evidence() -> Vec<(&'static str, String)> {
    vec![
        (
            "statefulSession",
            "driver/mod.rs:21-24 RedisConn holds exactly one RedisLiveConn, and \
             driver/mod.rs:127-134 RedisDriver::get_conn is the single keyed \
             access point every operation in the crate uses"
                .to_string(),
        ),
        (
            "namespaceSwitch",
            "driver/session.rs:9-18 select_db_on issues SELECT <index> in place on \
             the same live connection; driver/mod.rs:136-138 dispatching the same \
             call across Standalone/Cluster/Sentinel"
                .to_string(),
        ),
        (
            "contextObservation",
            "src/resource/observation.rs issues INFO server (driver/session.rs:20-29) \
             on the live connection, so liveness, health and protocol drain are \
             observed; the current database index is not read back from the server, \
             so the context reports an empty namespace at Partial confidence"
                .to_string(),
        ),
        (
            "resetForReuse",
            "declined: there is no verified baseline replay protocol. FLUSHDB is the \
             nearest command and it is destructive, so replaying a baseline with it \
             would delete the caller's data; src/resource/provider/contract.rs \
             reset_resource therefore returns Discard and leaves the handle valid"
                .to_string(),
        ),
        (
            "preciseCancel",
            "declined: Redis has no execution-addressed cancel. CLIENT KILL tears down \
             a whole connection rather than one running command, and \
             src/driver/database.rs cancel_query now refuses with DriverError::Unsupported \
             instead of reporting a cancellation that never happened"
                .to_string(),
        ),
        (
            "transactionObservation",
            "declined: Redis has no SQL transaction. MULTI queues commands and EXEC \
             applies them, but a queued buffer is not an all-or-nothing unit, so \
             declaring Full or Partial would promise a guarantee the server does not \
             give"
                .to_string(),
        ),
        (
            "data",
            "declined: ResultChunk.rows is a tabular shape and this driver never \
             produces one. src/driver/database.rs:216-243 query yields one Value per \
             command, database.rs:245-291 query_multi wraps those with empty columns \
             because Redis has no result-set metadata, and the Command API returns \
             free-form JSON. src/resource/payload.rs refuses all of them instead of \
             flattening them into empty rows"
                .to_string(),
        ),
        (
            "backup",
            "declined: ResourceProvider's 14 methods expose no backup/restore/dump, \
             and src/resource/payload.rs turns anything but MultiQueryResult or \
             {rowsAffected} into OperationNotSupported — while the real dump \
             (database.rs:340) and restore (database.rs:352) both run on a legacy \
             ConnectionHandle, so this contract can neither prove nor deny the \
             capability"
                .to_string(),
        ),
    ]
}
