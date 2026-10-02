//! The declared capability surface and namespace shape of the mysql driver
//! family, matching `docs/architecture/platform/driver-capability-migration.md`
//! §2.1.
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

/// mysql's declared capability set, shared by all six factories of this crate.
/// `precise_cancel` is passed in rather than hard-coded: it mirrors the
/// factory's own `supports_query_execution_cancel()` declaration, so a factory
/// that stops advertising precise cancellation stops claiming the cell with it.
pub(crate) fn mysql_capability_set(precise_cancel: bool) -> CapabilitySet {
    let mut capabilities = CapabilitySet::default();
    // A transaction only pins a connection temporarily, and §2.1 事实二 forbids
    // any driver from claiming a stateful session on that basis.
    capabilities.stateful_session = Availability::Unsupported;
    // `USE` switches the database on the live connection
    // (`MysqlDriver::build_use_database_sql`), so the switch itself is in place;
    // `change_context` still degrades to `RequiresReplacement` whenever the
    // read-back does not confirm it.
    capabilities.namespace_switch = NamespaceSwitch::InPlace;
    // The database is read back off the wire, but neither the session's
    // autocommit flag nor its identity is read, so this stays `Partial` and
    // `ObservationConfidence::Partial` — never `Confirmed`.
    capabilities.context_observation = ContextObservation::Partial;
    capabilities.transaction_observation = TransactionObservationCap::Partial;
    // The driver issues no prepared-statement / temporary-table / user-variable
    // handles to the host, so there is nothing to report.
    capabilities.session_scoped_handles = SessionScopedHandleSupport::Unsupported;
    // No verified path back to an initialization baseline: a pool of size one
    // is not evidence of reset (§6.1 / CM-19), so `reset_resource` always
    // answers `Discard`.
    capabilities.reset_for_reuse = ResetForReuse::Unsupported;
    // `MysqlDriver::supports_query_execution_cancel` is `true` and
    // `cancel_query_with_execution` issues a targeted `KILL QUERY`.
    capabilities.precise_cancel = if precise_cancel {
        PreciseCancelSupport::Supported
    } else {
        PreciseCancelSupport::Unsupported
    };
    // `begin_read_snapshot` issues `START TRANSACTION WITH CONSISTENT SNAPSHOT,
    // READ ONLY`, which spans the whole database.
    capabilities.snapshots = SnapshotSupport::PerDatabase;
    capabilities.transactions = TransactionSupport {
        // Only the level the driver actually issues (`SET TRANSACTION
        // ISOLATION LEVEL REPEATABLE READ`). Every other level is refused by
        // `begin_transaction` instead of being silently dropped.
        isolation_levels: vec!["REPEATABLE READ".to_string()],
        // `begin_transaction` sends a bare `START TRANSACTION`; no
        // `SAVEPOINT` is ever issued.
        savepoints: Availability::Unsupported,
        // One per connection handle: the driver refuses a second BEGIN on the
        // same connection.
        max_open_transactions: Some(1),
    };
    capabilities.ddl_atomicity = ddl_atomicity_support(DdlAtomicity::AutoCommitPerStatement);
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

/// mysql's namespace is the database; there is no catalog or schema level.
pub(crate) fn mysql_namespace_shape() -> NamespaceShape {
    NamespaceShape {
        levels: vec![
            NamespaceLevel::present(NamespaceLevelKind::Database, false),
            NamespaceLevel::absent(NamespaceLevelKind::Catalog),
            NamespaceLevel::absent(NamespaceLevelKind::Schema),
        ],
        path_segments: vec![NamespacePathSegment {
            position: 0,
            kind: NamespaceLevelKind::Database,
            meaning: "database".to_string(),
        }],
        // Database name folding is filesystem dependent on MySQL (case
        // sensitive on Linux, folded on macOS/Windows), so neither side is
        // claimed here.
        case_rules: CaseRules {
            unquoted: CaseFolding::Unknown,
            quoted: CaseFolding::Unknown,
        },
        canonical_id_rules: CanonicalIdRules {
            case_sensitive: false,
            delimiter: ".".to_string(),
        },
        aliases: BTreeMap::new(),
    }
}
