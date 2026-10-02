//! What this provider declares, and the evidence behind each cell.
//!
//! Every cell of [`hbase_capability_set`] is written explicitly, and every one
//! of them is justified by a line in [`hbase_capability_evidence`]. A cell with
//! no reason has not been examined, and a cell whose reason says "unknown" is
//! left at a value that claims nothing — because `CapabilitySet::default()` is
//! the fail-closed declaration: a caller reading an empty set has to ask,
//! whereas a cell guessed in either direction lets a caller proceed on an
//! assumption this driver cannot support.

use datazen_driver_api::capabilities::{
    Availability, CapabilityRegistry, CapabilitySet, CapabilitySnapshot, ContextObservation,
    DdlAtomicitySupport, NamespaceSwitch, PreciseCancelSupport, ResetForReuse, SessionContinuity,
    SessionScopedHandleSupport, SnapshotSupport, TransactionObservation, TransactionSupport,
};
use datazen_driver_api::capability_domains::{BackupSupport, DataSupport};
use datazen_driver_api::namespace::{CanonicalIdRules, NamespaceShape};
use datazen_driver_api::resource::ConnectionCostPolicy;
use datazen_driver_api::{ConnectionConfig, PROTOCOL_VERSION};

/// Stable provider identity. Also the owner string every
/// [`ResourceHandle`](datazen_driver_api::resource::ResourceHandle) is checked
/// against.
pub const HBASE_PROVIDER_ID: &str = "hbase";

const HBASE_DRIVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bumped whenever [`hbase_capability_set`] changes, so a stale snapshot is
/// detectable rather than silently reused.
const HBASE_CAPABILITY_REVISION: u64 = 1;

/// One acquired HBase resource is exactly one `reqwest::Client`.
///
/// This is the *declared* bound, and the budget charges it as one physical
/// connection per acquire. `HBaseDriver::connect` (`src/hbase.rs:259`) builds
/// exactly one client and inserts one pool entry, so the charge matches what the
/// driver creates. It does **not** claim anything about TCP sockets: a
/// `reqwest::Client` pools those internally and the driver exposes no
/// per-socket bound, so [`ConnectionCostPolicy::PoolBounded`] is used for the
/// client-level resource and the socket level is stated as undeclared in
/// [`hbase_capability_evidence`].
pub const MAX_PHYSICAL_CONNECTIONS: u32 = 1;

/// The only namespace shape a Stargate endpoint can have.
///
/// `levels` is **empty on purpose**. The driver's namespace is the base URL: a
/// Stargate cluster exposes no database, catalog or schema level, and
/// `get_tables` reads a `schema` argument only to reject it
/// (`src/hbase.rs:286`). `NamespaceShape::canonicalize` (`namespace.rs:180`)
/// treats a level the shape does not declare exactly like one declared with
/// `exists: false` — a value supplied for it is *rejected* rather than dropped.
/// So this shape does not merely ignore a `NamespaceTarget`'s fields, it
/// refuses them, which is what stops a driver that forgot to declare its shape
/// from appearing to honour an arbitrary target.
pub fn hbase_namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: Vec::new(),
        // No level means no segment in a request path: the namespace is the base
        // URL itself, which is not composed per statement.
        path_segments: Vec::new(),
        // Stargate table names are case-sensitive, but no rule here was
        // confirmed against a live cluster, so `Unknown` fails closed rather
        // than asserting either behaviour.
        case_rules: Default::default(),
        canonical_id_rules: CanonicalIdRules {
            // Table names are addressed by their exact string through the URL
            // path, and this driver never lower-cases them before sending. The
            // delimiter stays `"."` because there is no canonical path syntax to
            // separate on.
            case_sensitive: true,
            delimiter: ".".to_string(),
        },
        // A Stargate endpoint is addressed by one base URL; there is no alias.
        aliases: Default::default(),
    }
}

/// What one acquired resource costs.
///
/// Independent of the config because the driver's pool size is not the thing
/// being charged: one acquire creates exactly one HTTP client.
pub fn hbase_connection_cost(_config: &ConnectionConfig) -> ConnectionCostPolicy {
    ConnectionCostPolicy::PoolBounded {
        max_physical_connections: MAX_PHYSICAL_CONNECTIONS,
    }
}

/// The declaration a caller reads before deciding whether to try.
///
/// Built field by field on the default so a future `#[non_exhaustive]` field
/// cannot silently take this driver's declaration with it.
///
/// Two cells are deliberately left at their default even though the driver
/// demonstrably *can* read rows:
///
/// * `data` stays `Unknown`. `DataSupport` has no "buffered read-only" member.
///   `StreamingReadOnly` would over-claim, because a scan is paged and read to
///   exhaustion before the first chunk reaches the sink;
///   `BufferedReadWrite` would over-claim the write axis that
///   `HBaseDriver::execute` refuses outright (`src/hbase.rs:453`); and
///   `Unsupported` is a *measured absence*, which would deny the row reads the
///   driver performs every time a scan succeeds. `Unknown` is the only value
///   that claims nothing, and it fails closed: `enables_feature()` is false, so
///   a caller has to ask instead of assuming.
/// * `backup` stays `Unknown` for the same reason — no artifact is produced or
///   consumed anywhere in this driver or in the 14 contract methods.
pub fn hbase_capability_set() -> CapabilitySet {
    let mut capabilities = CapabilitySet::default();

    capabilities.stateful_session = Availability::Unsupported;
    capabilities.namespace_switch = NamespaceSwitch::Unsupported;
    capabilities.context_observation = ContextObservation::Unsupported;
    capabilities.transaction_observation = TransactionObservation::Unsupported;
    capabilities.session_scoped_handles = SessionScopedHandleSupport::Unsupported;
    capabilities.reset_for_reuse = ResetForReuse::Unsupported;
    capabilities.precise_cancel = PreciseCancelSupport::Unsupported;
    capabilities.snapshots = SnapshotSupport::Unsupported;
    capabilities.transactions = TransactionSupport {
        isolation_levels: Vec::new(),
        savepoints: Availability::Unsupported,
        max_open_transactions: None,
    };
    capabilities.ddl_atomicity = DdlAtomicitySupport::default();
    capabilities.data = DataSupport::default();
    capabilities.backup = BackupSupport::default();

    capabilities
}

/// The registry `ResourceProvider::capabilities` hands out, carrying the
/// evidence below so a caller can see *why* a cell says what it says.
pub fn hbase_capability_registry() -> CapabilityRegistry {
    let mut snapshot = CapabilitySnapshot::new(
        HBASE_PROVIDER_ID,
        HBASE_DRIVER_VERSION,
        PROTOCOL_VERSION,
        HBASE_CAPABILITY_REVISION,
    );
    for (claim, evidence) in hbase_capability_evidence() {
        snapshot.confirmed.insert(claim.to_string(), evidence);
    }
    let mut registry = CapabilityRegistry::new(HBASE_PROVIDER_ID, snapshot);
    registry.capabilities = hbase_capability_set();
    registry
}

/// Each declared cell, with the source that justifies it.
///
/// A refusal is evidence too, and is recorded as `declined: …` rather than
/// omitted — a capability with no line here has not been examined.
pub fn hbase_capability_evidence() -> Vec<(&'static str, String)> {
    vec![
        (
            "stateful_session",
            "declined: a resource is one reqwest::Client in HBaseDriver::clients \
             (src/hbase.rs:14); the Stargate REST API carries no session id and no session state \
             between requests, and GET / reports cluster status rather than a session"
                .to_string(),
        ),
        (
            "namespace_switch",
            "declined: the namespace is the base URL built from the connection config \
             (base_url, src/hbase.rs:259); a Stargate endpoint exposes no second namespace to \
             switch to, and get_tables rejects a schema argument outright (src/hbase.rs:286)"
                .to_string(),
        ),
        (
            "context_observation",
            "declined: there is no server-side session context to read back, so observe_session \
             reports SessionContext::unobserved() with ObservationConfidence::Unknown"
                .to_string(),
        ),
        (
            "transaction_observation",
            "declined: no transaction exists on this resource — see the transactions cell"
                .to_string(),
        ),
        (
            "session_scoped_handles",
            "declined: a scan opens a scanner, reads it to exhaustion and deletes it inside one \
             command (src/hbase.rs:55, HBaseDriver::scan), so the driver issues no cursor, scanner \
             or prepared-statement handle that outlives the call; execute_on_resource reports an \
             empty list rather than inventing one"
                .to_string(),
        ),
        (
            "reset_for_reuse",
            "declined: initialization_requirements is empty, so no baseline exists to replay; \
             reset_resource returns ResetDisposition::Discard and never claims Verified"
                .to_string(),
        ),
        (
            "precise_cancel",
            "declined: the driver keeps no execution registry, so no execution id can be addressed; \
             DatabaseDriver::cancel_query is a no-op Ok(()) (src/hbase.rs:498) and the contract \
             forbids using it as a fallback"
                .to_string(),
        ),
        (
            "snapshots",
            "declined: no snapshot endpoint is called anywhere in src/hbase.rs; Stargate offers \
             none, and HBase snapshots are a master-server concept this REST surface does not \
             expose"
                .to_string(),
        ),
        (
            "transactions",
            "declined: the Stargate REST API has no transaction endpoint and \
             HBaseDriver::execute refuses writes (src/hbase.rs:453), so isolation_levels is empty \
             rather than declaring a level nothing honours"
                .to_string(),
        ),
        (
            "ddl_atomicity",
            "declined: no DDL path exists on this provider — execute is refused before the wire — \
             so the map is empty and an absent key resolves to Unknown"
                .to_string(),
        ),
        (
            "data",
            "unknown: DataSupport has no 'buffered read-only' member. StreamingReadOnly would \
             over-claim incremental delivery, BufferedReadWrite would over-claim writes the driver \
             refuses, and Unsupported is a measured absence that would deny real row reads. Unknown \
             claims nothing and fails closed"
                .to_string(),
        ),
        (
            "backup",
            "unknown: this driver produces and consumes no artifact, and none of the 14 contract \
             methods carries a backup surface"
                .to_string(),
        ),
        (
            "namespace_shape",
            "confirmed: levels is empty because Stargate has no database, catalog or schema level; \
             NamespaceShape::canonicalize therefore *rejects* any value supplied for an undeclared \
             level instead of dropping it"
                .to_string(),
        ),
        (
            "connection_cost",
            "confirmed: one acquire creates exactly one reqwest::Client (src/hbase.rs:259, connect), \
             so PoolBounded{1} matches what the driver builds. The TCP socket level is *not* \
             declared: reqwest pools sockets internally and this driver exposes no bound over them"
                .to_string(),
        ),
        (
            "session_continuity",
            "confirmed: describe_resource declares SessionContinuity::Leased. One client is not a \
             fixed session — a pool of size one would pass for one, which is exactly the confusion \
             SessionContinuity::is_fixed() exists to prevent"
                .to_string(),
        ),
    ]
}

/// The continuity this provider declares, named once so the capability evidence
/// and `describe_resource` cannot drift apart.
pub const HBASE_SESSION_CONTINUITY: SessionContinuity = SessionContinuity::Leased;
