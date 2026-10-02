//! What the vector provider can honestly be asked to do.
//!
//! Two artefacts live here and they are deliberately separate:
//!
//! * the **shape and cost** ([`vector_namespace_shape`],
//!   [`vector_connection_cost`]) — arithmetic over the request, used by
//!   `describe_resource` and `acquire_resource`;
//! * the **declaration** ([`vector_capability_set`]) — the 12 cells a caller
//!   reads before it decides whether to try.
//!
//! The declaration is built with `CapabilitySet::default()` plus one
//! assignment per field, never a struct literal: the set is `#[non_exhaustive]`,
//! so a literal would not compile once another driver lands a field. Every cell
//! below states *what was observed in this driver's source* — a claim a reviewer
//! can check — or is left `Unknown` with the reason it could not be proven.
//!
//! | Capability | Declared | Why |
//! |---|---|---|
//! | `stateful_session` | `Unsupported` | A resource is one `reqwest::Client` in `VectorDriver::clients` (`vector.rs:21`). The Qdrant REST API has no session id, no session affinity and no session state: every request is independent. Nothing server-side survives between two statements on one resource. |
//! | `namespace_switch` | `Unsupported` | An instance has exactly one namespace and it is pinned by the base URL built from the connection config (`vector.rs:base_url`). There is no second namespace to switch to, in place or otherwise. |
//! | `context_observation` | `Unsupported` | There is no server-side context to read back. `observe_session` reports `SessionContext::unobserved()` with `ObservationConfidence::Unknown`, which never matches an acquisition target (`session.rs:194`). |
//! | `transaction_observation` | `Unsupported` | No transaction exists to observe — see the `transactions` row. |
//! | `session_scoped_handles` | `Unsupported` | The driver issues no cursor, prepared statement or scanner handle. `points_to_result` (`vector.rs:76`) materialises the whole result set; `execute_on_resource` therefore reports an empty handle list rather than inventing one. |
//! | `reset_for_reuse` | `Unsupported` | Nothing is replayed and nothing can be proven clean, so `reset_resource` returns `ResetDisposition::Discard`. A `Verified` claim would require a baseline this provider never issues (`initialization_requirements` is empty). |
//! | `precise_cancel` | `Unsupported` | The driver keeps no execution registry, so no id can be addressed. `cancel_query` is `Ok(())` (`vector.rs`), a no-op that the contract explicitly forbids using as a fallback. |
//! | `snapshots` | `Unsupported` | No snapshot endpoint is called anywhere in the driver. |
//! | `transactions` | no levels, no savepoints, no maximum | The Qdrant REST API this driver speaks has no transaction endpoint, and `VectorDriver::execute` already refuses writes outright. A declared level would be one nothing honours. |
//! | `ddl_atomicity` | left empty | No DDL path exists on this provider: `execute` is refused before it reaches the wire, so there is no statement whose atomicity could be observed. The empty map is the honest answer, and `atomicity_for` resolves an absent key to `Unknown`. |
//! | `data` | `Unknown` (default) | See [`vector_capability_set`]'s own comment — the domain has no honest value for "buffered read-only", and both alternatives lie. |
//! | `backup` | `Unknown` (default) | Nothing here produces or consumes an artifact, and the 14 contract methods contain no backup surface at all. |

use datazen_driver_api::capabilities::{
    Availability, CapabilityRegistry, CapabilitySet, CapabilitySnapshot, ContextObservation,
    DdlAtomicitySupport, NamespaceSwitch, PreciseCancelSupport, ResetForReuse, SessionContinuity,
    SessionScopedHandleSupport, SnapshotSupport, TransactionObservation, TransactionSupport,
};
use datazen_driver_api::capability_domains::{BackupSupport, DataSupport};
use datazen_driver_api::namespace::{CanonicalIdRules, NamespaceShape};
use datazen_driver_api::resource::ConnectionCostPolicy;
use datazen_driver_api::{ConnectionConfig, PROTOCOL_VERSION};

/// Stable provider identity. Also the owner string every [`ResourceHandle`](datazen_driver_api::resource::ResourceHandle)
/// is checked against.
pub const VECTOR_PROVIDER_ID: &str = "vector";

const VECTOR_DRIVER_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Bumped whenever [`vector_capability_set`] changes, so a stale snapshot is
/// detectable rather than silently reused.
const VECTOR_CAPABILITY_REVISION: u64 = 1;

/// One acquired vector resource is exactly one `reqwest::Client`.
///
/// This is the *declared* bound, and the budget charges it as one physical
/// connection per acquire. `VectorDriver::connect` (`vector.rs`) builds exactly
/// one client and inserts one pool entry, so the charge matches what the driver
/// creates. It does **not** claim anything about TCP sockets: a `reqwest::Client`
/// pools those internally and the driver exposes no per-socket bound, so
/// [`ConnectionCostPolicy::PoolBounded`] is used for the client-level resource
/// and the socket level is stated as undeclared in
/// [`vector_capability_evidence`].
pub const MAX_PHYSICAL_CONNECTIONS: u32 = 1;

/// The only namespace shape a Qdrant instance can have.
///
/// `levels` is **empty on purpose**. Qdrant has no database, catalog or schema
/// level: collections live directly in the one namespace the base URL addresses.
/// `NamespaceShape::canonicalize` (`namespace.rs:180`) treats a level the shape
/// does not declare exactly like one declared with `exists: false` — a value
/// supplied for it is *rejected* rather than dropped. So this shape does not
/// merely ignore a `NamespaceTarget`'s fields, it refuses them, which is what
/// stops a driver that forgot to declare its shape from appearing to honour an
/// arbitrary target.
pub fn vector_namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: Vec::new(),
        // No level means no segment in a request path: the namespace is the
        // base URL itself, which is not composed per statement.
        path_segments: Vec::new(),
        // Qdrant never folds case for collection names, but no rule here was
        // confirmed against a server, so `Unknown` fails closed rather than
        // asserting either behaviour.
        case_rules: Default::default(),
        canonical_id_rules: CanonicalIdRules {
            // Collection names are compared verbatim by Qdrant, and this driver
            // addresses them by their exact string. The delimiter stays `"."`
            // because there is no canonical path syntax to separate on.
            case_sensitive: true,
            delimiter: ".".to_string(),
        },
        // A Qdrant instance is addressed by one base URL; there is no alias.
        aliases: Default::default(),
    }
}

/// What one acquired resource costs.
///
/// Independent of the config because the driver's pool size is not the thing
/// being charged: one acquire creates exactly one HTTP client.
pub fn vector_connection_cost(_config: &ConnectionConfig) -> ConnectionCostPolicy {
    ConnectionCostPolicy::PoolBounded {
        max_physical_connections: MAX_PHYSICAL_CONNECTIONS,
    }
}

/// The declaration a caller reads before deciding whether to try.
///
/// Built field by field on the default so a future
/// `#[non_exhaustive]` field cannot silently take this driver's declaration with
/// it.
///
/// Two cells are deliberately left at their default even though the driver
/// demonstrably *can* read rows:
///
/// * `data` stays `Unknown`. `DataSupport` has no "buffered read-only" member.
///   `StreamingReadOnly` would over-claim, because this provider's whole result
///   set is materialised before the first chunk reaches the sink;
///   `BufferedReadWrite` would over-claim the write axis that
///   `VectorDriver::execute` refuses outright; and `Unsupported` is a
///   *measured absence*, which would deny the row reads the driver performs
///   every time `query` succeeds. `Unknown` is the only value that claims
///   nothing, and it fails closed: `enables_feature()` is false, so a caller
///   has to ask instead of assuming.
/// * `backup` stays `Unknown` for the same reason — no artifact is produced or
///   consumed anywhere in this driver or in the 14 contract methods.
pub fn vector_capability_set() -> CapabilitySet {
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
pub fn vector_capability_registry() -> CapabilityRegistry {
    let mut snapshot = CapabilitySnapshot::new(
        VECTOR_PROVIDER_ID,
        VECTOR_DRIVER_VERSION,
        PROTOCOL_VERSION,
        VECTOR_CAPABILITY_REVISION,
    );
    for (claim, evidence) in vector_capability_evidence() {
        snapshot.confirmed.insert(claim.to_string(), evidence);
    }
    let mut registry = CapabilityRegistry::new(VECTOR_PROVIDER_ID, snapshot);
    registry.capabilities = vector_capability_set();
    registry
}

/// Each declared cell, with the source that justifies it.
///
/// A refusal is evidence too, and is recorded as `declined: …` rather than
/// omitted — a capability with no line here has not been examined.
pub fn vector_capability_evidence() -> Vec<(&'static str, String)> {
    vec![
        (
            "stateful_session",
            "declined: a resource is one reqwest::Client in VectorDriver::clients (src/vector.rs:21); \
             the Qdrant REST API carries no session id and no session state between requests"
                .to_string(),
        ),
        (
            "namespace_switch",
            "declined: the namespace is the base URL built from the connection config; an instance \
             exposes no second namespace to switch to (src/vector.rs, base_url)"
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
            "declined: points_to_result materialises the whole result set (src/vector.rs:76), so the \
             driver issues no cursor or prepared-statement handle; execute_on_resource reports an \
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
             DatabaseDriver::cancel_query is a no-op Ok(()) and the contract forbids using it as a \
             fallback"
                .to_string(),
        ),
        (
            "snapshots",
            "declined: no snapshot endpoint is called anywhere in src/vector.rs"
                .to_string(),
        ),
        (
            "transactions",
            "declined: the Qdrant REST API has no transaction endpoint and VectorDriver::execute \
             refuses writes, so isolation_levels is empty rather than declaring a level nothing honours"
                .to_string(),
        ),
        (
            "ddl_atomicity",
            "declined: no DDL path exists on this provider — execute is refused before the wire — so \
             the map is empty and an absent key resolves to Unknown"
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
            "confirmed: levels is empty because Qdrant has no database, catalog or schema level; \
             NamespaceShape::canonicalize therefore *rejects* any value supplied for an undeclared \
             level instead of dropping it"
                .to_string(),
        ),
        (
            "connection_cost",
            "confirmed: one acquire creates exactly one reqwest::Client (src/vector.rs, connect), so \
             PoolBounded{1} matches what the driver builds. The TCP socket level is *not* declared: \
             reqwest pools sockets internally and this driver exposes no bound over them"
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
pub const VECTOR_SESSION_CONTINUITY: SessionContinuity = SessionContinuity::Leased;
