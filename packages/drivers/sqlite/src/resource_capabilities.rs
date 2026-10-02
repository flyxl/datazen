//! The declared capability surface and namespace shape of the sqlite driver,
//! matching `docs/architecture/platform/driver-capability-migration.md` §2.1.
//!
//! Kept apart from the provider itself so the declarations can be read — and
//! tested — without wading through the connection plumbing. Every cell below is
//! either read off the driver's own code or explicitly refused; see
//! `driver-capability-migration.md` §6.1 for the cells that may never be
//! opened on the strength of a pool size.

use std::collections::BTreeMap;

use datazen_driver_api::capabilities::{
    Availability, CapabilitySet, ContextObservation, DdlAtomicitySupport, NamespaceSwitch,
    PreciseCancelSupport, ResetForReuse, SessionScopedHandleSupport, SnapshotSupport,
    TransactionObservation as TransactionObservationCap, TransactionSupport,
};
use datazen_driver_api::namespace::{
    CanonicalIdRules, CaseFolding, CaseRules, NamespaceLevel, NamespaceLevelKind,
    NamespacePathSegment, NamespaceShape,
};
use datazen_driver_api::DdlAtomicity;

/// `DdlAtomicitySupport::by_operation` has no canonical key vocabulary in
/// `driver-api` — the contract only says an unknown key must fail closed. The
/// keys used here are the snake_case names of
/// `datazen_driver_api::schema_migration::MigrationOperation`, which is the only
/// in-tree convention (`driver-api/tests/support` uses `create_table`). Any other
/// key falls closed to `Unknown`, never to `Transactional`.
const MIGRATION_OPERATIONS: [&str; 33] = [
    "create_table",
    "drop_table",
    "add_column",
    "drop_column",
    "alter_column_type",
    "set_nullable",
    "set_default",
    "set_comment",
    "set_table_options",
    "set_auto_increment",
    "add_primary_key",
    "drop_primary_key",
    "create_index",
    "drop_index",
    "add_foreign_key",
    "drop_foreign_key",
    "add_check_constraint",
    "drop_check_constraint",
    "create_view",
    "replace_view",
    "drop_view",
    "create_routine",
    "replace_routine",
    "drop_routine",
    "create_trigger",
    "replace_trigger",
    "drop_trigger",
    "create_sequence",
    "replace_sequence",
    "drop_sequence",
    "create_type",
    "replace_type",
    "drop_type",
];

/// sqlite's declared capability set. It is fixed, not parameterised: the one
/// factory in this crate backs exactly one provider id and has no `supports_*`
/// hook of its own that could widen a cell.
pub(crate) fn sqlite_capability_set() -> CapabilitySet {
    let mut capabilities = CapabilitySet::default();
    // `connect` builds a pool of size one (`sqlite.rs::connect`), and §2.1 事实二
    // plus §6.1 forbid reading that as a stateful session: a one-connection pool
    // is a scheduling decision, not a session the host can pin state onto. The
    // descriptor agrees — it reports `SessionContinuity::Leased`.
    capabilities.stateful_session = Availability::Unsupported;
    // The database is chosen by the file path at `connect` time and travels with
    // the statement (`SqlTarget::qualify_sql_target`), so there is no session
    // state to switch: `change_context` answers `Unsupported`.
    capabilities.namespace_switch = NamespaceSwitch::Unsupported;
    // The attached databases come back from `PRAGMA database_list`, but neither
    // an identity nor an autocommit flag is ever read, so this is `Partial` and
    // `ObservationConfidence::Partial` — never `Confirmed`.
    capabilities.context_observation = ContextObservation::Partial;
    capabilities.transaction_observation = TransactionObservationCap::Partial;
    // No prepared-statement / temporary-table / user-variable handle is handed
    // to the host, so there is nothing to report.
    capabilities.session_scoped_handles = SessionScopedHandleSupport::Unsupported;
    // No verified path back to an initialization baseline. A pool of size one is
    // not evidence of reset (§6.1 / CM-19), so `reset_resource` always answers
    // `Discard` and the caller re-acquires.
    capabilities.reset_for_reuse = ResetForReuse::Unsupported;
    // There is no statement registry and no interrupt primitive: the driver
    // owns exactly one in-process connection and never runs a statement it
    // could later address. `cancel_query` says so with an error instead of
    // claiming a cancellation that did not happen.
    capabilities.precise_cancel = PreciseCancelSupport::Unsupported;
    // A bare `BEGIN` is a deferred transaction over the whole file with no
    // read-only or point-in-time variant.
    capabilities.snapshots = SnapshotSupport::Unsupported;
    capabilities.transactions = TransactionSupport {
        // `begin_transaction` sends a bare `BEGIN`, which selects no particular
        // isolation level; naming one would be claiming something not issued.
        // Any requested level is therefore refused up front.
        isolation_levels: Vec::new(),
        // No `SAVEPOINT` is ever issued by this driver.
        savepoints: Availability::Unsupported,
        // One per connection handle: `begin_transaction` sends `BEGIN` on the
        // single pooled connection, and a second one would fail on the wire.
        max_open_transactions: Some(1),
    };
    // `SqliteDriver::ddl_atomicity` answers `Transactional` and the reviewed
    // rebuild path runs inside one transaction.
    capabilities.ddl_atomicity = ddl_atomicity_support(DdlAtomicity::Transactional);
    capabilities
}

/// One driver-wide DDL atomicity value, filed under every migration operation.
pub(crate) fn ddl_atomicity_support(atomicity: DdlAtomicity) -> DdlAtomicitySupport {
    let mut support = DdlAtomicitySupport::default();
    for operation in MIGRATION_OPERATIONS {
        support
            .by_operation
            .insert(operation.to_string(), atomicity);
    }
    support
}

/// The operation names this provider files its DDL atomicity under.
#[cfg(test)]
pub(crate) fn migration_operation_keys() -> &'static [&'static str; 33] {
    &MIGRATION_OPERATIONS
}

/// sqlite's namespace is one file, so one database. There is no catalog and no
/// schema level, and `ATTACH` is only ever issued from tests, never by the
/// production path — an attached alias therefore is not a namespace level this
/// provider advertises.
pub(crate) fn sqlite_namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![
            NamespaceLevel::present(NamespaceLevelKind::Database, false),
            NamespaceLevel::absent(NamespaceLevelKind::Catalog),
            NamespaceLevel::absent(NamespaceLevelKind::Schema),
        ],
        path_segments: vec![NamespacePathSegment {
            position: 0,
            kind: NamespaceLevelKind::Database,
            meaning: "database file".to_string(),
        }],
        // SQLite identifier folding is a property of the running library build
        // (`SQLITE_CASE_SENSITIVE_LIKE` and friends), not of the file, so
        // neither side is claimed here.
        case_rules: CaseRules {
            unquoted: CaseFolding::Unknown,
            quoted: CaseFolding::Unknown,
        },
        canonical_id_rules: CanonicalIdRules {
            // Two names may alias the same file; the driver resolves that with
            // device/inode identity, not with a string rule.
            case_sensitive: false,
            delimiter: ".".to_string(),
        },
        aliases: BTreeMap::new(),
    }
}
